use crate::counting::get_counter;
use crate::detect::{parse_url_parts, Detector, Verdict};
use crate::model::CallRecord;
use crate::parse_request::parse_request;
use crate::parse_response::{find_subslice, parse_response_body, SSEParser};
use crate::segment::{build_record, hash_text, BuildRecordArgs};
use crate::upstream::{connect_tcp_timeout, connect_via_proxy, make_upstream_tls_config, proxy_for};
use flate2::read::{DeflateDecoder, GzDecoder, ZlibDecoder};
use rustls::pki_types::ServerName;
use rustls::{ClientConfig, ClientConnection, StreamOwned};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub const MAX_HEAD: usize = 64 * 1024;
pub const MAX_BODY: usize = 64 * 1024 * 1024;
pub const DEFAULT_AZURE_SUFFIXES: &[&str] = &[
    ".openai.azure.com",
    ".cognitiveservices.azure.com",
    ".services.ai.azure.com",
];

const HOP_BY_HOP: &[&str] = &[
    "connection",
    "proxy-connection",
    "keep-alive",
    "transfer-encoding",
    "te",
    "trailer",
    "upgrade",
];

#[derive(Debug, Clone)]
pub struct Route {
    pub prefix: String,
    pub scheme: String,
    pub host: String,
    pub port: u16,
    pub base_path: String,
}

impl Route {
    pub fn from_urls(prefix: &str, upstream: &str) -> Self {
        let (scheme, host, port, path) = parse_url_parts(upstream);
        Self {
            prefix: prefix.trim_end_matches('/').to_string(),
            scheme,
            host,
            port,
            base_path: path.trim_end_matches('/').to_string(),
        }
    }
}

pub fn parse_content_length(cl: Option<&str>) -> Option<usize> {
    let first = cl?.split(',').next()?.trim();
    first.parse::<usize>().ok()
}

pub struct BodyFramer {
    pub chunked: bool,
    pub remaining: Option<usize>,
    pub done: bool,
    buf: Vec<u8>,
    pos: usize,
    chunk_left: usize,
    state: &'static str,
}

impl BodyFramer {
    pub fn new(headers: &HashMap<String, String>, no_body: bool) -> Self {
        let te = headers
            .get("transfer-encoding")
            .map(|s| s.to_ascii_lowercase())
            .unwrap_or_default();
        let chunked = te.contains("chunked");
        let remaining = if chunked {
            None
        } else {
            parse_content_length(headers.get("content-length").map(|s| s.as_str()))
        };
        let done = no_body || remaining == Some(0);
        Self {
            chunked,
            remaining,
            done,
            buf: Vec::new(),
            pos: 0,
            chunk_left: 0,
            state: "size",
        }
    }

    pub fn feed(&mut self, data: &[u8]) -> Vec<u8> {
        if self.done || data.is_empty() {
            return Vec::new();
        }
        if self.chunked {
            return self.feed_chunked(data);
        }
        if let Some(rem) = self.remaining {
            if data.len() <= rem {
                self.remaining = Some(rem - data.len());
                if self.remaining == Some(0) {
                    self.done = true;
                }
                return data.to_vec();
            }
            let take = data[..rem].to_vec();
            self.remaining = Some(0);
            self.done = true;
            return take;
        }
        data.to_vec()
    }

    fn feed_chunked(&mut self, data: &[u8]) -> Vec<u8> {
        if self.buf.len().saturating_sub(self.pos) + data.len() > MAX_BODY {
            self.done = true;
            self.buf.clear();
            self.pos = 0;
            return Vec::new();
        }
        self.buf.extend_from_slice(data);
        let mut out = Vec::new();
        while self.pos < self.buf.len() {
            match self.state {
                "size" => {
                    let Some(rel_crlf) = find_subslice(&self.buf[self.pos..], b"\r\n") else {
                        if self.buf.len() - self.pos > 4096 {
                            self.done = true;
                        }
                        break;
                    };
                    let line_end = self.pos + rel_crlf;
                    let raw_line = &self.buf[self.pos..line_end];
                    let size_slice = match raw_line.iter().position(|&b| b == b';') {
                        Some(semi) => &raw_line[..semi],
                        None => raw_line,
                    };
                    self.pos = line_end + 2;
                    let size_str = std::str::from_utf8(size_slice).unwrap_or("").trim();
                    let Ok(sz) = usize::from_str_radix(size_str, 16) else {
                        self.done = true;
                        break;
                    };
                    if sz > MAX_BODY {
                        self.done = true;
                        break;
                    }
                    if sz == 0 {
                        self.state = "trailer";
                    } else {
                        self.chunk_left = sz;
                        self.state = "data";
                    }
                }
                "data" => {
                    let avail = self.buf.len() - self.pos;
                    if avail == 0 {
                        break;
                    }
                    let take = avail.min(self.chunk_left);
                    out.extend_from_slice(&self.buf[self.pos..self.pos + take]);
                    self.pos += take;
                    self.chunk_left -= take;
                    if self.chunk_left == 0 {
                        self.state = "crlf";
                    }
                }
                "crlf" => {
                    if self.buf.len() - self.pos < 2 {
                        break;
                    }
                    self.pos += 2;
                    self.state = "size";
                }
                "trailer" => {
                    let Some(rel_crlf) = find_subslice(&self.buf[self.pos..], b"\r\n") else {
                        break;
                    };
                    let line_len = rel_crlf;
                    self.pos += rel_crlf + 2;
                    if line_len == 0 {
                        self.done = true;
                        break;
                    }
                }
                _ => break,
            }
        }
        if self.pos > 65_536 || self.pos == self.buf.len() {
            self.buf.drain(..self.pos);
            self.pos = 0;
        }
        out
    }
}

pub fn decode_content_encoding(data: &[u8], encoding: Option<&str>) -> Vec<u8> {
    let enc = encoding.unwrap_or("").trim().to_ascii_lowercase();
    if enc.is_empty() || enc == "identity" {
        return data.to_vec();
    }
    let cap = 32 * 1024 * 1024usize;
    if enc.contains("gzip") {
        let mut dec = GzDecoder::new(data).take(cap as u64);
        let mut out = Vec::new();
        if dec.read_to_end(&mut out).is_ok() {
            return out;
        }
    } else if enc.contains("deflate") {
        let mut zdec = ZlibDecoder::new(data).take(cap as u64);
        let mut out = Vec::new();
        if zdec.read_to_end(&mut out).is_ok() && !out.is_empty() {
            return out;
        }
        out.clear();
        let mut rdec = DeflateDecoder::new(data).take(cap as u64);
        if rdec.read_to_end(&mut out).is_ok() {
            return out;
        }
    } else if enc.contains("br") {
        let mut dec = brotli::Decompressor::new(data, 4096).take(cap as u64);
        let mut out = Vec::new();
        if dec.read_to_end(&mut out).is_ok() {
            return out;
        }
    } else if enc.contains("zstd") {
        if let Ok(dec) = ruzstd::StreamingDecoder::new(data) {
            let mut out = Vec::new();
            if dec.take(cap as u64).read_to_end(&mut out).is_ok() {
                return out;
            }
        }
    }
    data.to_vec()
}

fn read_head<R: Read>(reader: &mut R, limit: usize) -> std::io::Result<(Vec<u8>, Vec<u8>)> {
    let mut buf = Vec::with_capacity(4096);
    let mut chunk = [0u8; 16384];
    loop {
        let n = reader.read(&mut chunk)?;
        if n == 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "EOF while reading HTTP head",
            ));
        }
        let search_from = buf.len().saturating_sub(3);
        buf.extend_from_slice(&chunk[..n]);
        if let Some(rel) = find_subslice(&buf[search_from..], b"\r\n\r\n") {
            let i = search_from + rel;
            let head = buf[..i].to_vec();
            let leftover = buf[i + 4..].to_vec();
            return Ok((head, leftover));
        }
        if buf.len() > limit {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "HTTP head too large",
            ));
        }
    }
}

fn parse_head(raw: &[u8]) -> (String, Vec<(String, String)>, HashMap<String, String>) {
    let text = String::from_utf8_lossy(raw);
    let mut lines = text.split("\r\n");
    let first = lines.next().unwrap_or("").to_string();
    let mut headers = Vec::new();
    let mut hdict = HashMap::new();
    for ln in lines {
        if ln.is_empty() {
            continue;
        }
        if let Some((k, v)) = ln.split_once(':') {
            let ks = k.trim().to_string();
            let vs = v.trim().to_string();
            hdict.insert(ks.to_ascii_lowercase(), vs.clone());
            headers.push((ks, vs));
        }
    }
    (first, headers, hdict)
}

fn read_request_body<R: Read>(
    reader: &mut R,
    hd: &HashMap<String, String>,
    prefix: &[u8],
) -> std::io::Result<Vec<u8>> {
    let te = hd
        .get("transfer-encoding")
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_default();
    if te.contains("chunked") {
        let mut framer = BodyFramer::new(hd, false);
        let mut out = Vec::new();
        if !prefix.is_empty() {
            out.extend_from_slice(&framer.feed(prefix));
        }
        let mut chunk = [0u8; 32768];
        while !framer.done {
            let n = reader.read(&mut chunk)?;
            if n == 0 {
                break;
            }
            out.extend_from_slice(&framer.feed(&chunk[..n]));
            if out.len() > MAX_BODY {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "request body too large",
                ));
            }
        }
        return Ok(out);
    }
    let n = parse_content_length(hd.get("content-length").map(|s| s.as_str())).unwrap_or(0);
    if n > MAX_BODY {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "request body too large",
        ));
    }
    if prefix.len() >= n {
        return Ok(prefix[..n].to_vec());
    }
    let mut out = Vec::with_capacity(n);
    out.extend_from_slice(prefix);
    let mut rem = n - prefix.len();
    let mut chunk = [0u8; 32768];
    while rem > 0 {
        let to_read = rem.min(chunk.len());
        let got = reader.read(&mut chunk[..to_read])?;
        if got == 0 {
            break;
        }
        out.extend_from_slice(&chunk[..got]);
        rem -= got;
    }
    Ok(out)
}

enum UpstreamIo {
    Plain(TcpStream),
    Tls(Box<StreamOwned<ClientConnection, TcpStream>>),
}

impl Read for UpstreamIo {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self {
            UpstreamIo::Plain(s) => s.read(buf),
            UpstreamIo::Tls(s) => s.read(buf),
        }
    }
}

impl Write for UpstreamIo {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        match self {
            UpstreamIo::Plain(s) => s.write(buf),
            UpstreamIo::Tls(s) => s.write(buf),
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        match self {
            UpstreamIo::Plain(s) => s.flush(),
            UpstreamIo::Tls(s) => s.flush(),
        }
    }
}

pub struct GatewayStats {
    pub requests: AtomicUsize,
    pub recorded: AtomicUsize,
    pub record_errors: AtomicUsize,
    pub bypassed: AtomicUsize,
    pub dropped: AtomicUsize,
    pub active: AtomicUsize,
    pub last_request_ts: Mutex<Option<f64>>,
}

impl Default for GatewayStats {
    fn default() -> Self {
        Self {
            requests: AtomicUsize::new(0),
            recorded: AtomicUsize::new(0),
            record_errors: AtomicUsize::new(0),
            bypassed: AtomicUsize::new(0),
            dropped: AtomicUsize::new(0),
            active: AtomicUsize::new(0),
            last_request_ts: Mutex::new(None),
        }
    }
}

pub struct Gateway {
    pub routes: Vec<Route>,
    pub sink: Arc<dyn Fn(CallRecord) + Send + Sync>,
    pub salt: Vec<u8>,
    pub components: HashMap<String, String>,
    pub run_id: Option<String>,
    pub azure_hosts: HashSet<String>,
    pub azure_suffixes: Vec<String>,
    pub host: String,
    pub port: AtomicUsize,
    pub use_tiktoken: bool,
    pub ingest_token: Option<String>,
    pub detector: Arc<Detector>,
    pub record_hosts: bool,
    pub shutdown_flag: Arc<AtomicBool>,
    pub stats: GatewayStats,
    tls_cfg: Arc<ClientConfig>,
    warned: Mutex<HashSet<String>>,
}

impl Gateway {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        mut routes: Vec<Route>,
        sink: Arc<dyn Fn(CallRecord) + Send + Sync>,
        salt: Vec<u8>,
        components: HashMap<String, String>,
        run_id: Option<String>,
        azure_hosts: HashSet<String>,
        use_tiktoken: bool,
        ingest_token: Option<String>,
        detector: Option<Arc<Detector>>,
        record_hosts: bool,
    ) -> Arc<Self> {
        routes.sort_by_key(|b| std::cmp::Reverse(b.prefix.len()));
        let rec_hosts =
            record_hosts || std::env::var("TOKPROF_RECORD_HOSTS").ok().as_deref() == Some("1");
        Arc::new(Self {
            routes,
            sink,
            salt,
            components,
            run_id,
            azure_hosts,
            azure_suffixes: DEFAULT_AZURE_SUFFIXES
                .iter()
                .map(|s| (*s).to_string())
                .collect(),
            host: "127.0.0.1".to_string(),
            port: AtomicUsize::new(0),
            use_tiktoken,
            ingest_token,
            detector: detector.unwrap_or_else(|| Arc::new(Detector::default())),
            record_hosts: rec_hosts,
            shutdown_flag: Arc::new(AtomicBool::new(false)),
            stats: GatewayStats::default(),
            tls_cfg: make_upstream_tls_config(),
            warned: Mutex::new(HashSet::new()),
        })
    }

    fn warn_once(&self, key: &str, msg: &str) {
        if let Ok(mut w) = self.warned.lock() {
            if w.insert(key.to_string()) {
                eprintln!("tokprof: {msg}");
            }
        }
    }

    fn valid_token(&self, provided: Option<&str>) -> bool {
        let Some(ref expected) = self.ingest_token else {
            return false;
        };
        let Some(prov) = provided else {
            return false;
        };
        if expected.len() != prov.len() {
            return false;
        }
        let mut diff = 0u8;
        for (a, b) in expected.bytes().zip(prov.bytes()) {
            diff |= a ^ b;
        }
        diff == 0
    }

    fn resolve(&self, path: &str) -> Option<(String, String, u16, String)> {
        let (bare, query_opt) = match path.split_once('?') {
            Some((b, q)) => (b, Some(q)),
            None => (path, None),
        };
        if let Some(rest) = bare.strip_prefix("/azure/") {
            let (host, tail) = rest.split_once('/').unwrap_or((rest, ""));
            if self.azure_hosts.contains(host)
                || self.azure_suffixes.iter().any(|suf| host.ends_with(suf))
            {
                let mut p = format!("/{tail}");
                if let Some(q) = query_opt {
                    p.push('?');
                    p.push_str(q);
                }
                return Some(("https".to_string(), host.to_string(), 443, p));
            }
            return None;
        }
        for r in &self.routes {
            if bare == r.prefix || bare.starts_with(&format!("{}/", r.prefix)) {
                let tail = &bare[r.prefix.len()..];
                let joined = format!("{}{}", r.base_path, tail);
                let mut p = if joined.is_empty() {
                    "/".to_string()
                } else {
                    joined
                };
                if let Some(q) = query_opt {
                    p.push('?');
                    p.push_str(q);
                }
                return Some((r.scheme.clone(), r.host.clone(), r.port, p));
            }
        }
        None
    }

    fn open_upstream(&self, scheme: &str, host: &str, port: u16) -> Result<UpstreamIo, String> {
        let timeout = Duration::from_secs(30);
        if scheme != "https" {
            let s = connect_tcp_timeout(host, port, timeout).map_err(|e| e.to_string())?;
            return Ok(UpstreamIo::Plain(s));
        }
        let tcp = if let Some(proxy) = proxy_for(host) {
            connect_via_proxy(&proxy, host, port, timeout).map_err(|e| e.to_string())?
        } else {
            connect_tcp_timeout(host, port, timeout).map_err(|e| e.to_string())?
        };
        let sname = ServerName::try_from(host.to_string()).map_err(|e| e.to_string())?;
        let conn = ClientConnection::new(self.tls_cfg.clone(), sname).map_err(|e| e.to_string())?;
        let mut tls_stream = StreamOwned::new(conn, tcp);
        // Complete TLS handshake eagerly so certificate errors surface on connect
        if let Err(e) = tls_stream.conn.complete_io(&mut tls_stream.sock) {
            return Err(format!("TLS handshake failed: {e} (CERTIFICATE_VERIFY_FAILED)"));
        }
        Ok(UpstreamIo::Tls(Box::new(tls_stream)))
    }

    fn respond(
        writer: &mut TcpStream,
        status: u16,
        reason: &str,
        body: &[u8],
        ctype: &str,
    ) -> std::io::Result<()> {
        let head = format!(
            "HTTP/1.1 {status} {reason}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        );
        writer.write_all(head.as_bytes())?;
        writer.write_all(body)?;
        writer.flush()
    }

    pub fn handle_connection(self: &Arc<Self>, mut stream: TcpStream) {
        self.stats.active.fetch_add(1, Ordering::SeqCst);
        let _ = stream.set_read_timeout(Some(Duration::from_secs(60)));
        let _ = stream.set_write_timeout(Some(Duration::from_secs(60)));
        if let Err(e) = self.serve_one(&mut stream) {
            self.stats.dropped.fetch_add(1, Ordering::SeqCst);
            self.warn_once(
                &format!("drop:{:?}", e.kind()),
                &format!("client connection ended early ({e})"),
            );
        }
        let _ = stream.shutdown(std::net::Shutdown::Both);
        self.stats.active.fetch_sub(1, Ordering::SeqCst);
    }

    fn now_secs() -> f64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs_f64())
            .unwrap_or(0.0)
    }

    fn serve_one(self: &Arc<Self>, stream: &mut TcpStream) -> std::io::Result<()> {
        let (raw_head, leftover) = read_head(stream, MAX_HEAD)?;
        let (first, headers, hd) = parse_head(&raw_head);
        let mut parts = first.split(' ');
        let method = parts.next().unwrap_or("").to_string();
        let target = parts.next().unwrap_or("").to_string();
        self.stats.requests.fetch_add(1, Ordering::SeqCst);

        if target.starts_with("/__tokprof/health") {
            let now = Self::now_secs();
            let age = self
                .stats
                .last_request_ts
                .lock()
                .ok()
                .and_then(|g| *g)
                .map(|ts| (now - ts).max(0.0));
            let body = serde_json::to_vec(&json!({
                "ok": true,
                "last_request_age_s": age,
                "requests": self.stats.requests.load(Ordering::SeqCst),
                "recorded": self.stats.recorded.load(Ordering::SeqCst),
                "record_errors": self.stats.record_errors.load(Ordering::SeqCst),
                "bypassed": self.stats.bypassed.load(Ordering::SeqCst),
            }))
            .unwrap_or_default();
            return Self::respond(stream, 200, "OK", &body, "application/json");
        }

        if target.starts_with("/__tokprof/shutdown") && method == "POST" {
            if !self.valid_token(hd.get("x-tokprof-token").map(|s| s.as_str())) {
                return Self::respond(stream, 403, "Forbidden", b"tokprof: bad shutdown token", "text/plain");
            }
            self.shutdown_flag.store(true, Ordering::SeqCst);
            return Self::respond(stream, 204, "No Content", b"", "text/plain");
        }

        if target.starts_with("/__tokprof/ingest") && method == "POST" {
            let body = read_request_body(stream, &hd, &leftover)?;
            if !self.valid_token(hd.get("x-tokprof-token").map(|s| s.as_str())) {
                return Self::respond(stream, 403, "Forbidden", b"tokprof: bad ingest token", "text/plain");
            }
            if let Ok(msg) = serde_json::from_slice::<Value>(&body) {
                self.ingest_message(&msg);
            } else {
                self.stats.record_errors.fetch_add(1, Ordering::SeqCst);
            }
            return Self::respond(stream, 204, "No Content", b"", "text/plain");
        }

        if let Ok(mut g) = self.stats.last_request_ts.lock() {
            *g = Some(Self::now_secs());
        }
        if hd
            .get("expect")
            .map(|s| s.eq_ignore_ascii_case("100-continue"))
            .unwrap_or(false)
        {
            stream.write_all(b"HTTP/1.1 100 Continue\r\n\r\n")?;
            stream.flush()?;
        }
        let body = read_request_body(stream, &hd, &leftover)?;
        let Some((scheme, host, port, upath)) = self.resolve(&target) else {
            self.stats.bypassed.fetch_add(1, Ordering::SeqCst);
            return Self::respond(
                stream,
                404,
                "Not Found",
                b"tokprof: no upstream route for this path",
                "text/plain",
            );
        };

        let mut verdict_opt: Option<Verdict> = None;
        let mut parsed_opt = None;
        if method == "POST" && !body.is_empty() {
            if let Ok(obj) = serde_json::from_slice::<Value>(&body) {
                let bare_path = upath.split('?').next().unwrap_or(&upath);
                let v = self
                    .detector
                    .inspect(&method, &host, port, bare_path, Some(&obj));
                if (v.llm || v.candidate) && obj.is_object() {
                    if let Some(ref api) = v.api {
                        if let Ok(mut pr) = parse_request(api, &obj) {
                            if pr.model.is_none() && v.model_hint.is_some() {
                                pr.model = v.model_hint.clone();
                            }
                            pr.stream = pr.stream || v.stream_hint;
                            parsed_opt = Some(pr);
                        }
                    }
                }
                verdict_opt = Some(v);
            }
        }

        let t0_ts = Self::now_secs();
        let t0_inst = Instant::now();
        let mut up = match self.open_upstream(&scheme, &host, port) {
            Ok(u) => u,
            Err(e) => {
                let mut hint = "";
                if e.contains("CERTIFICATE_VERIFY_FAILED") || e.contains("InvalidCertificate") {
                    hint = " (the endpoint's certificate is not trusted: for a private CA pass --upstream-ca FILE or set SSL_CERT_FILE)";
                }
                let msg = format!("tokprof: upstream connect failed: {e}{hint}");
                return Self::respond(stream, 502, "Bad Gateway", msg.as_bytes(), "text/plain");
            }
        };

        let host_hdr = if port != 80 && port != 443 {
            format!("{host}:{port}")
        } else {
            host.clone()
        };
        let mut fwd = vec![format!("{method} {upath} HTTP/1.1"), format!("Host: {host_hdr}")];
        for (k, v) in &headers {
            let kl = k.to_ascii_lowercase();
            if !HOP_BY_HOP.contains(&kl.as_str())
                && !matches!(
                    kl.as_str(),
                    "host" | "content-length" | "accept-encoding" | "expect"
                )
            {
                fwd.push(format!("{k}: {v}"));
            }
        }
        fwd.push("Accept-Encoding: identity".to_string());
        fwd.push("Connection: close".to_string());
        if !body.is_empty() || matches!(method.as_str(), "POST" | "PUT" | "PATCH") {
            fwd.push(format!("Content-Length: {}", body.len()));
        }
        let req_head_bytes = format!("{}\r\n\r\n", fwd.join("\r\n"));
        up.write_all(req_head_bytes.as_bytes())?;
        up.write_all(&body)?;
        up.flush()?;

        let status: Option<i64>;
        let ttfb_ms: Option<i64>;
        let is_sse: bool;
        let mut collected = Vec::new();
        let mut sse_parser: Option<SSEParser> = None;
        let mut err_str: Option<String> = None;
        let resp_encoding: Option<String>;

        let mut head_res = read_head(&mut up, MAX_HEAD);
        while let Ok((ref rhead_raw, ref pending)) = head_res {
            let (sline, _, _) = parse_head(rhead_raw);
            let code = sline
                .split_whitespace()
                .nth(1)
                .and_then(|s| s.parse::<i64>().ok());
            if code == Some(100) && pending.is_empty() {
                head_res = read_head(&mut up, MAX_HEAD);
            } else {
                break;
            }
        }
        match head_res {
            Ok((rhead_raw, mut pending)) => {
                ttfb_ms = Some(t0_inst.elapsed().as_millis() as i64);
                let (sline, rheaders, rhd) = parse_head(&rhead_raw);
                status = sline
                    .split_whitespace()
                    .nth(1)
                    .and_then(|s| s.parse::<i64>().ok());
                resp_encoding = rhd.get("content-encoding").cloned();

                let mut out_hdr = vec![sline];
                for (k, v) in &rheaders {
                    let kl = k.to_ascii_lowercase();
                    if !matches!(kl.as_str(), "connection" | "proxy-connection" | "keep-alive") {
                        out_hdr.push(format!("{k}: {v}"));
                    }
                }
                out_hdr.push("Connection: close".to_string());
                stream.write_all(format!("{}\r\n\r\n", out_hdr.join("\r\n")).as_bytes())?;
                stream.flush()?;

                is_sse = rhd
                    .get("content-type")
                    .map(|c| c.contains("text/event-stream"))
                    .unwrap_or(false);
                let is_compressed = resp_encoding
                    .as_deref()
                    .map(|e| {
                        let el = e.trim().to_ascii_lowercase();
                        !el.is_empty() && el != "identity"
                    })
                    .unwrap_or(false);
                if let Some(ref pr) = parsed_opt {
                    if is_sse && !is_compressed {
                        sse_parser = Some(SSEParser::new(&pr.provider));
                    }
                }
                let no_body = method == "HEAD"
                    || matches!(status, Some(204 | 304))
                    || status.map(|c| (100..200).contains(&c)).unwrap_or(false);
                let mut framer = BodyFramer::new(&rhd, no_body);
                let mut buf = [0u8; 65536];
                while !framer.done {
                    let data = if !pending.is_empty() {
                        std::mem::take(&mut pending)
                    } else {
                        match up.read(&mut buf) {
                            Ok(0) => break,
                            Ok(n) => buf[..n].to_vec(),
                            Err(e) => {
                                err_str = Some(format!("stream_interrupted:{:?}", e.kind()));
                                break;
                            }
                        }
                    };
                    stream.write_all(&data)?;
                    stream.flush()?;
                    if parsed_opt.is_some() {
                        let decoded = framer.feed(&data);
                        if let Some(ref mut sp) = sse_parser {
                            sp.feed(&decoded);
                        }
                        if (sse_parser.is_none()
                            || verdict_opt.as_ref().map(|v| v.candidate).unwrap_or(false))
                            && collected.len() < MAX_BODY
                        {
                            let room = MAX_BODY.saturating_sub(collected.len());
                            collected.extend_from_slice(&decoded[..decoded.len().min(room)]);
                        }
                    } else {
                        let _ = framer.feed(&data);
                    }
                }
            }
            Err(e) => {
                let msg = format!("tokprof: upstream closed the connection: {e}");
                return Self::respond(stream, 502, "Bad Gateway", msg.as_bytes(), "text/plain");
            }
        }

        let latency_ms = t0_inst.elapsed().as_millis() as i64;
        if let (Some(parsed), Some(mut verdict)) = (parsed_opt, verdict_opt) {
            let decompressed = decode_content_encoding(&collected, resp_encoding.as_deref());
            let text = String::from_utf8_lossy(&decompressed).into_owned();
            let response = if let Some(mut sp) = sse_parser {
                sp.finish()
            } else {
                parse_response_body(&parsed.provider, &text, is_sse)
            };
            if self.decide(&mut verdict, &host, port, &upath, &text, is_sse) {
                self.record_call(
                    &parsed,
                    Some(&response),
                    &target,
                    status,
                    t0_ts,
                    ttfb_ms,
                    err_str,
                    Some(latency_ms),
                    Some(&verdict),
                    &host,
                    port,
                );
            }
        }
        Ok(())
    }

    fn decide(
        &self,
        verdict: &mut Verdict,
        host: &str,
        port: u16,
        path: &str,
        text: &str,
        sse: bool,
    ) -> bool {
        let bare = path.split('?').next().unwrap_or(path);
        if verdict.candidate
            && !verdict.llm
            && self
                .detector
                .confirm(verdict, host, port, bare, text, sse)
                .is_none()
        {
            return false;
        }
        self.detector.accept(verdict, host, port, bare);
        true
    }

    pub fn ingest_message(&self, msg: &Value) {
        let Some(obj) = msg.as_object() else {
            return;
        };
        let path = obj.get("path").and_then(|v| v.as_str()).unwrap_or("");
        let mut host = obj
            .get("host")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let mut port = obj
            .get("port")
            .and_then(|v| {
                v.as_u64()
                    .or_else(|| v.as_str().and_then(|s| s.trim().parse::<u64>().ok()))
            })
            .map(|n| n as u16)
            .unwrap_or(0);
        if port == 0 && !host.is_empty() {
            if host.starts_with('[') {
                if let Some(end_b) = host.find(']') {
                    let h_part = host[1..end_b].to_string();
                    if let Some(p_str) = host[end_b + 1..].strip_prefix(':') {
                        if let Ok(pn) = p_str.parse::<u16>() {
                            port = pn;
                            host = h_part;
                        }
                    }
                }
            } else if let Some((h_part, p_str)) = host.rsplit_once(':') {
                if let Ok(pn) = p_str.parse::<u16>() {
                    port = pn;
                    host = h_part.to_string();
                }
            }
        }
        let gw_port = self.port.load(Ordering::SeqCst) as u16;
        if matches!(host.as_str(), "127.0.0.1" | "localhost") && gw_port != 0 && port == gw_port {
            return;
        }
        let req_obj = match obj.get("request") {
            Some(Value::String(s)) => serde_json::from_str::<Value>(s).ok(),
            Some(v @ Value::Object(_)) => Some(v.clone()),
            _ => None,
        };
        let Some(req_val) = req_obj else {
            return;
        };
        if !req_val.is_object() {
            return;
        }
        let method = obj.get("method").and_then(|v| v.as_str()).unwrap_or("POST");
        let mut verdict = self
            .detector
            .inspect(method, &host, port, path, Some(&req_val));
        if !(verdict.llm || verdict.candidate) {
            return;
        }
        let Some(ref api) = verdict.api else {
            return;
        };
        let Ok(mut parsed) = parse_request(api, &req_val) else {
            return;
        };
        if parsed.model.is_none() && verdict.model_hint.is_some() {
            parsed.model = verdict.model_hint.clone();
        }
        parsed.stream = parsed.stream || verdict.stream_hint;
        let resp_text = obj.get("response").and_then(|v| v.as_str()).unwrap_or("");
        let sse = obj.get("sse").and_then(|v| v.as_bool()).unwrap_or(false);
        let response = parse_response_body(&parsed.provider, resp_text, sse);
        if !self.decide(&mut verdict, &host, port, path, resp_text, sse) {
            return;
        }
        let ts = obj
            .get("ts")
            .and_then(|v| v.as_f64())
            .unwrap_or_else(Self::now_secs);
        let status = obj.get("status").and_then(|v| {
            v.as_i64()
                .or_else(|| v.as_f64().map(|f| f.round() as i64))
                .or_else(|| v.as_str().and_then(|s| s.trim().parse::<i64>().ok()))
        });
        let ttfb_ms = obj
            .get("ttfb_ms")
            .and_then(|v| v.as_f64())
            .map(|f| f.round() as i64);
        let latency_ms = obj
            .get("latency_ms")
            .and_then(|v| v.as_f64())
            .map(|f| f.round() as i64);
        let err = obj
            .get("error")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        self.record_call(
            &parsed,
            Some(&response),
            path,
            status,
            ts,
            ttfb_ms,
            err,
            latency_ms,
            Some(&verdict),
            &host,
            port,
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn record_call(
        &self,
        parsed: &crate::parse_request::ParsedRequest,
        response: Option<&crate::parse_response::ParsedResponse>,
        target: &str,
        status: Option<i64>,
        t0: f64,
        ttfb_ms: Option<i64>,
        err: Option<String>,
        latency_ms: Option<i64>,
        verdict: Option<&Verdict>,
        host: &str,
        port: u16,
    ) {
        let counter = get_counter(parsed.model.as_deref(), self.use_tiktoken);
        let bare_ep = target.split('?').next().unwrap_or(target);
        let mut rec = build_record(BuildRecordArgs {
            parsed,
            response,
            counter: &counter,
            salt: &self.salt,
            components: Some(&self.components),
            endpoint: bare_ep,
            status,
            latency_ms,
            ttfb_ms,
            run: self.run_id.clone(),
            error: err,
            ts: Some(t0),
            include_text: None,
        });
        if let Some(v) = verdict {
            if !v.host_class.is_empty() {
                rec.host_class = Some(v.host_class.clone());
            }
            if !host.is_empty() {
                rec.host_id = Some(hash_text(&format!("{host}:{port}"), &self.salt));
                if self.record_hosts {
                    rec.host = Some(if port != 0 {
                        format!("{host}:{port}")
                    } else {
                        host.to_string()
                    });
                }
            }
            let ev: Vec<String> = v.evidence.iter().take(6).cloned().collect();
            rec.detect = Some(json!({
                "score": v.score,
                "evidence": ev,
                "forced": if v.forced { Value::Bool(true) } else { Value::Null },
            }));
        }
        (self.sink)(rec);
        self.stats.recorded.fetch_add(1, Ordering::SeqCst);
    }
}

pub struct GatewayServer {
    pub gateway: Arc<Gateway>,
    pub port: u16,
    stop: Arc<AtomicBool>,
    handle: Option<thread::JoinHandle<()>>,
}

impl GatewayServer {
    pub fn start(gateway: Arc<Gateway>) -> std::io::Result<Self> {
        let listener = TcpListener::bind((gateway.host.as_str(), 0))?;
        let port = listener.local_addr()?.port();
        gateway.port.store(port as usize, Ordering::SeqCst);
        let _ = listener.set_nonblocking(true);
        let stop = gateway.shutdown_flag.clone();
        let stop_thread = stop.clone();
        let gw_clone = gateway.clone();

        let handle = thread::Builder::new()
            .name("tokprof-gateway".to_string())
            .spawn(move || {
                while !stop_thread.load(Ordering::SeqCst) {
                    match listener.accept() {
                        Ok((stream, _)) => {
                            if gw_clone.stats.active.load(Ordering::SeqCst) >= 256 {
                                let _ = stream.shutdown(std::net::Shutdown::Both);
                                continue;
                            }
                            let _ = stream.set_nonblocking(false);
                            let gw = gw_clone.clone();
                            let _ = thread::Builder::new()
                                .name("tokprof-conn".to_string())
                                .spawn(move || {
                                    gw.handle_connection(stream);
                                });
                        }
                        Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(5));
                        }
                        Err(_) => {
                            thread::sleep(Duration::from_millis(10));
                        }
                    }
                }
            })?;

        Ok(Self {
            gateway,
            port,
            stop,
            handle: Some(handle),
        })
    }

    pub fn stop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        let deadline = Instant::now() + Duration::from_secs(3);
        while self.gateway.stats.active.load(Ordering::SeqCst) > 0 && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(5));
        }
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

impl Drop for GatewayServer {
    fn drop(&mut self) {
        self.stop();
    }
}
