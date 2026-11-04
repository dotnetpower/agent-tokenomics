use crate::ebpf::which_exe;
use crate::gateway::{Gateway, GatewayServer};
use crate::runner::materialize_boot_dir;
use crate::store::JsonlWriter;
use rand::RngCore;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

pub const TRUST_HELP: &str = "\
mitmproxy intercepts TLS with its own certificate authority, so the processes you watch must trust it.
tokprof does not install anything into your trust stores. Choose the narrowest option that works:

  Per process (preferred; nothing system-wide):
    Python (requests/httpx/aiohttp):  REQUESTS_CA_BUNDLE / SSL_CERT_FILE = ~/.mitmproxy/mitmproxy-ca-cert.pem
    Node.js:                          NODE_EXTRA_CA_CERTS = ~/.mitmproxy/mitmproxy-ca-cert.pem
    curl:                             --cacert ~/.mitmproxy/mitmproxy-ca-cert.pem
  The certificate file is created the first time mitmproxy runs.

  System-wide (only if a process ignores the variables above; remember to remove it afterwards):
    macOS:    sudo security add-trusted-cert -d -r trustRoot -k /Library/Keychains/System.keychain ~/.mitmproxy/mitmproxy-ca-cert.pem
    Windows:  certutil -addstore -f Root %USERPROFILE%\\.mitmproxy\\mitmproxy-ca-cert.cer   (administrator prompt)

Local capture mode needs administrator rights on Windows and a one-time approval of the mitmproxy system
extension on macOS; see https://docs.mitmproxy.org/stable/concepts/modes/#local-capture
";

pub fn find_mitmdump(explicit: Option<&str>) -> Option<PathBuf> {
    if let Some(exp) = explicit {
        if !exp.is_empty() && Path::new(exp).is_file() {
            return Some(PathBuf::from(exp));
        }
    }
    if let Ok(env_m) = std::env::var("TOKPROF_MITMDUMP") {
        if !env_m.is_empty() && Path::new(&env_m).is_file() {
            return Some(PathBuf::from(env_m));
        }
    }
    which_exe("mitmdump")
}

pub fn build_command(
    mitmdump: &Path,
    mode: &str,
    port: u16,
    token: &str,
    extra: &[String],
) -> Vec<String> {
    let addon = materialize_boot_dir().join("mitm_addon.py");
    let mut cmd = vec![
        mitmdump.to_string_lossy().into_owned(),
        "--mode".to_string(),
        mode.to_string(),
        "-s".to_string(),
        addon.to_string_lossy().into_owned(),
        "--set".to_string(),
        format!("tokprof_port={port}"),
        "--set".to_string(),
        format!("tokprof_token={token}"),
        "--set".to_string(),
        "termlog_verbosity=warn".to_string(),
        "--set".to_string(),
        "flow_detail=0".to_string(),
    ];
    cmd.extend_from_slice(extra);
    cmd
}

#[allow(clippy::too_many_arguments)]
pub fn run_mitm(
    out_path: &str,
    local: &str,
    mode: &str,
    mitmdump: Option<&str>,
    components: HashMap<String, String>,
    use_tiktoken: bool,
    extra: &[String],
    duration: Option<f64>,
    quiet: bool,
) -> i32 {
    if local.is_empty() == mode.is_empty() {
        eprintln!(
            "tokprof: choose exactly one of --local PROCESSES (OS-level capture) or --mode MODE (for example regular@8080)"
        );
        return 2;
    }
    let Some(exe) = find_mitmdump(mitmdump) else {
        eprintln!(
            "tokprof: mitmdump not found. Install mitmproxy outside your project, for example `uv tool install mitmproxy` or `pipx install mitmproxy`, or pass --mitmdump PATH."
        );
        return 2;
    };
    let writer = match JsonlWriter::new(out_path) {
        Ok(w) => Arc::new(w),
        Err(e) => {
            eprintln!("tokprof: cannot open {out_path}: {e}");
            return 2;
        }
    };
    let mut salt = vec![0u8; 16];
    rand::thread_rng().fill_bytes(&mut salt);
    let mut run_b = [0u8; 4];
    rand::thread_rng().fill_bytes(&mut run_b);
    let mut tok_b = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut tok_b);
    let token = hex::encode(tok_b);

    let w_clone = writer.clone();
    let gw = Gateway::new(
        Vec::new(),
        Arc::new(move |rec| w_clone.write_record(&rec)),
        salt,
        components,
        Some(hex::encode(run_b)),
        Default::default(),
        use_tiktoken,
        Some(token.clone()),
        None,
        false,
    );
    let mut srv = match GatewayServer::start(gw.clone()) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("tokprof: failed to start gateway: {e}");
            return 1;
        }
    };
    let eff_mode = if !local.is_empty() {
        format!("local:{local}")
    } else {
        mode.to_string()
    };
    let cmd = build_command(&exe, &eff_mode, srv.port, &token, extra);
    if !quiet {
        eprintln!(
            "tokprof: profiling to {out_path} via mitmproxy ({eff_mode}); press Ctrl+C to stop. Processes must trust the mitmproxy CA: tokprof mitm --trust-help"
        );
    }
    let mut child = match Command::new(&cmd[0]).args(&cmd[1..]).spawn() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("tokprof: cannot start mitmdump: {e}");
            srv.stop();
            writer.close();
            return 127;
        }
    };
    let code = if let Some(dur_s) = duration {
        let deadline = Instant::now() + Duration::from_secs_f64(dur_s.max(0.0));
        loop {
            match child.try_wait() {
                Ok(Some(st)) => break st.code().unwrap_or(0),
                Ok(None) => {
                    if Instant::now() >= deadline {
                        let _ = child.kill();
                        let _ = child.wait();
                        break 0;
                    }
                    thread::sleep(Duration::from_millis(25));
                }
                Err(_) => break 1,
            }
        }
    } else {
        child.wait().ok().and_then(|s| s.code()).unwrap_or(0)
    };

    thread::sleep(Duration::from_millis(150));
    srv.stop();
    writer.close();
    if !quiet {
        let recorded = gw.stats.recorded.load(Ordering::SeqCst);
        let errs = gw.stats.record_errors.load(Ordering::SeqCst);
        eprintln!("tokprof: {recorded} LLM call(s) recorded, {errs} recording error(s)");
        let summary = gw.detector.render_summary();
        if !summary.is_empty() {
            eprintln!("{summary}");
        }
    }
    if duration.is_some() || matches!(code, 0 | -2 | -15 | 130) {
        0
    } else {
        code
    }
}
