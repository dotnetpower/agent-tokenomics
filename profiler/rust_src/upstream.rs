use crate::detect::{cidr_match, parse_url_parts};
use base64::Engine;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{ClientConfig, DigitallySignedStruct, Error as TlsError, RootCertStore, SignatureScheme};
use std::fs::File;
use std::io::{BufReader, Read, Write};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::sync::Arc;
use std::time::Duration;

const ALWAYS_DIRECT: &[&str] = &["localhost", "127.0.0.1", "::1"];
const CA_VARIABLES: &[&str] = &[
    "SSL_CERT_FILE",
    "REQUESTS_CA_BUNDLE",
    "CURL_CA_BUNDLE",
    "NODE_EXTRA_CA_CERTS",
    "TOKPROF_UPSTREAM_CA",
];

#[derive(Debug)]
struct NoCertificateVerification;

impl ServerCertVerifier for NoCertificateVerification {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, TlsError> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, TlsError> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, TlsError> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        vec![
            SignatureScheme::RSA_PKCS1_SHA256,
            SignatureScheme::RSA_PKCS1_SHA384,
            SignatureScheme::RSA_PKCS1_SHA512,
            SignatureScheme::ECDSA_NISTP256_SHA256,
            SignatureScheme::ECDSA_NISTP384_SHA384,
            SignatureScheme::ECDSA_NISTP521_SHA512,
            SignatureScheme::RSA_PSS_SHA256,
            SignatureScheme::RSA_PSS_SHA384,
            SignatureScheme::RSA_PSS_SHA512,
            SignatureScheme::ED25519,
        ]
    }
}

pub fn make_upstream_tls_config() -> Arc<ClientConfig> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    if std::env::var("TOKPROF_INSECURE_UPSTREAM").ok().as_deref() == Some("1") {
        eprintln!(
            "tokprof: WARNING: upstream TLS certificate verification is disabled (--insecure-upstream)"
        );
        let cfg = ClientConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .expect("default TLS protocol versions")
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(NoCertificateVerification))
            .with_no_client_auth();
        return Arc::new(cfg);
    }

    let mut roots = RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    let native_res = rustls_native_certs::load_native_certs();
    for cert in native_res.certs {
        let _ = roots.add(cert);
    }

    for &var in CA_VARIABLES {
        let Ok(raw_val) = std::env::var(var) else {
            continue;
        };
        let sep = if var == "TOKPROF_UPSTREAM_CA" {
            if cfg!(windows) {
                ';'
            } else {
                ':'
            }
        } else {
            '\0'
        };
        for path in raw_val.split(sep) {
            let p = path.trim();
            if p.is_empty() {
                continue;
            }
            let Ok(file) = File::open(p) else {
                eprintln!(
                    "tokprof: {var} points to {p:?}, which is not a readable file; ignoring it"
                );
                continue;
            };
            let mut reader = BufReader::new(file);
            let mut loaded = 0usize;
            for cert in rustls_pemfile::certs(&mut reader).flatten() {
                if roots.add(cert).is_ok() {
                    loaded += 1;
                }
            }
            if loaded == 0 {
                eprintln!(
                    "tokprof: could not load CA bundle from {var} ({p:?}); ignoring it"
                );
            }
        }
    }

    let cfg = ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .expect("default TLS protocol versions")
        .with_root_certificates(roots)
        .with_no_client_auth();
    Arc::new(cfg)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProxyInfo {
    pub host: String,
    pub port: u16,
    pub auth: Option<String>,
}

fn env_first(names: &[&str]) -> Option<String> {
    for &n in names {
        if let Ok(v) = std::env::var(n) {
            if !v.trim().is_empty() {
                return Some(v);
            }
        }
    }
    None
}

fn percent_decode_str(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(hex_s) = std::str::from_utf8(&bytes[i + 1..i + 3]) {
                if let Ok(val) = u8::from_str_radix(hex_s, 16) {
                    out.push(val);
                    i += 3;
                    continue;
                }
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

pub fn proxy_for(host: &str) -> Option<ProxyInfo> {
    let raw = env_first(&["HTTPS_PROXY", "https_proxy", "ALL_PROXY", "all_proxy"])?;
    let h_clean = host.trim().trim_matches(|c| c == '[' || c == ']').to_ascii_lowercase();
    if ALWAYS_DIRECT.contains(&h_clean.as_str()) {
        return None;
    }
    if let Some(no_proxy) = env_first(&["NO_PROXY", "no_proxy"]) {
        for raw_item in no_proxy.split(',') {
            let mut item = raw_item.trim().to_ascii_lowercase();
            if item.is_empty() {
                continue;
            }
            if item.contains(':') && !item.contains('/') && !item.starts_with('[') {
                if let Some((left, _)) = item.rsplit_once(':') {
                    item = left.to_string();
                }
            }
            if item.contains('/') {
                if cidr_match(&item, &h_clean) {
                    return None;
                }
                continue;
            }
            let stripped = item.trim_start_matches('*');
            if stripped.is_empty() || stripped == "." {
                return None;
            }
            let bare = stripped.trim_start_matches('.');
            if h_clean == bare || h_clean.ends_with(&format!(".{bare}")) {
                return None;
            }
        }
    }

    let with_scheme = if raw.contains("://") {
        raw
    } else {
        format!("http://{raw}")
    };
    let (scheme, rest) = with_scheme.split_once("://")?;
    if !scheme.eq_ignore_ascii_case("http") && !scheme.is_empty() {
        return None;
    }
    let authority = rest.split('/').next().unwrap_or(rest);
    let (userinfo_opt, host_port) = match authority.rsplit_once('@') {
        Some((u, hp)) => (Some(u), hp),
        None => (None, authority),
    };
    let (_, p_host, p_port, _) = parse_url_parts(&format!("http://{host_port}"));
    if p_host.is_empty() {
        return None;
    }
    let port = if host_port.contains(':') { p_port } else { 8080 };
    let auth = userinfo_opt.map(|ui| {
        let (u_raw, p_raw) = ui.split_once(':').unwrap_or((ui, ""));
        let decoded = format!("{}:{}", percent_decode_str(u_raw), percent_decode_str(p_raw));
        let b64 = base64::engine::general_purpose::STANDARD.encode(decoded.as_bytes());
        format!("Basic {b64}")
    });
    Some(ProxyInfo {
        host: p_host,
        port,
        auth,
    })
}

pub fn connect_tcp_timeout(host: &str, port: u16, timeout: Duration) -> std::io::Result<TcpStream> {
    let addr_spec = if host.contains(':') && !host.starts_with('[') {
        format!("[{host}]:{port}")
    } else {
        format!("{host}:{port}")
    };
    let addrs: Vec<SocketAddr> = addr_spec.to_socket_addrs()?.collect();
    if addrs.is_empty() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "no socket address resolved",
        ));
    }
    let mut last_err = None;
    for addr in addrs {
        match TcpStream::connect_timeout(&addr, timeout) {
            Ok(s) => {
                let _ = s.set_read_timeout(Some(Duration::from_secs(60)));
                let _ = s.set_write_timeout(Some(Duration::from_secs(60)));
                return Ok(s);
            }
            Err(e) => last_err = Some(e),
        }
    }
    Err(last_err.unwrap_or_else(|| std::io::Error::other("connect failed")))
}

pub fn connect_via_proxy(
    proxy: &ProxyInfo,
    host: &str,
    port: u16,
    timeout: Duration,
) -> std::io::Result<TcpStream> {
    let mut sock = connect_tcp_timeout(&proxy.host, proxy.port, timeout)?;
    let mut lines = vec![
        format!("CONNECT {host}:{port} HTTP/1.1"),
        format!("Host: {host}:{port}"),
    ];
    if let Some(ref auth) = proxy.auth {
        lines.push(format!("Proxy-Authorization: {auth}"));
    }
    let req = format!("{}\r\n\r\n", lines.join("\r\n"));
    sock.write_all(req.as_bytes())?;

    let mut buf = Vec::with_capacity(512);
    let mut chunk = [0u8; 4096];
    while !buf.windows(4).any(|w| w == b"\r\n\r\n") {
        let n = sock.read(&mut chunk)?;
        if n == 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "proxy closed connection during CONNECT",
            ));
        }
        buf.extend_from_slice(&chunk[..n]);
        if buf.len() > 65_536 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "proxy response too large",
            ));
        }
    }
    let first_line_end = buf.windows(2).position(|w| w == b"\r\n").unwrap_or(buf.len());
    let status_line = String::from_utf8_lossy(&buf[..first_line_end]).into_owned();
    let code = status_line.split_whitespace().nth(1).unwrap_or("");
    if code != "200" {
        return Err(std::io::Error::new(
            std::io::ErrorKind::ConnectionRefused,
            format!("proxy refused CONNECT: {status_line}"),
        ));
    }
    Ok(sock)
}
