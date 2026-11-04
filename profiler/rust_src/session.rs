use crate::ebpf::{ebpf_can_run, helper_command, ingest_lines};
use crate::gateway::{Gateway, GatewayServer};
use crate::runner::{build_routes, current_env_map, hook_env};
use crate::store::{atomic_write_0600, JsonlWriter};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};

static SIG_STOP: AtomicBool = AtomicBool::new(false);

#[cfg(unix)]
extern "C" fn handle_sig(_sig: i32) {
    SIG_STOP.store(true, Ordering::SeqCst);
}

#[cfg(unix)]
pub fn install_signal_handlers() {
    extern "C" {
        fn signal(sig: i32, handler: extern "C" fn(i32)) -> usize;
    }
    unsafe {
        let _ = signal(2, handle_sig); // SIGINT
        let _ = signal(15, handle_sig); // SIGTERM
    }
}

#[cfg(not(unix))]
pub fn install_signal_handlers() {}
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const OVERRIDDEN: &[&str] = &[
    "OPENAI_BASE_URL",
    "ANTHROPIC_BASE_URL",
    "AZURE_OPENAI_ENDPOINT",
    "PYTHONPATH",
    "NODE_OPTIONS",
    "TOKPROF_PORT",
    "TOKPROF_TOKEN",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionState {
    pub pid: u32,
    pub port: u16,
    pub out: String,
    pub started: f64,
    #[serde(default)]
    pub token: String,
    #[serde(default)]
    pub ebpf: Option<String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[serde(default)]
    pub originals: BTreeMap<String, Option<String>>,
}

pub fn state_path(override_path: Option<&str>) -> String {
    if let Some(p) = override_path {
        if !p.is_empty() {
            return p.to_string();
        }
    }
    let base = std::env::var("TOKPROF_HOME").unwrap_or_else(|_| {
        let home = std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .unwrap_or_else(|_| ".".to_string());
        Path::new(&home)
            .join(".tokprof")
            .to_string_lossy()
            .into_owned()
    });
    Path::new(&base)
        .join("session.json")
        .to_string_lossy()
        .into_owned()
}

pub fn read_state(path: &str) -> Option<SessionState> {
    let raw = fs::read_to_string(path).ok()?;
    serde_json::from_str::<SessionState>(&raw).ok()
}

pub fn pid_alive(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    #[cfg(target_os = "linux")]
    {
        Path::new(&format!("/proc/{pid}")).exists()
    }
    #[cfg(all(unix, not(target_os = "linux")))]
    {
        Command::new("kill")
            .args(["-0", &pid.to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }
    #[cfg(windows)]
    {
        Command::new("tasklist")
            .args(["/FI", &format!("PID eq {pid}"), "/NH"])
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.contains(&pid.to_string()))
            .unwrap_or(false)
    }
}

pub fn health(port: u16) -> Option<Value> {
    if port == 0 {
        return None;
    }
    let addr = format!("127.0.0.1:{port}");
    let mut sock = TcpStream::connect_timeout(&addr.parse().ok()?, Duration::from_secs(2)).ok()?;
    let _ = sock.set_read_timeout(Some(Duration::from_secs(2)));
    let _ = sock.set_write_timeout(Some(Duration::from_secs(2)));
    sock.write_all(
        format!("GET /__tokprof/health HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n")
            .as_bytes(),
    )
    .ok()?;
    let mut resp = String::new();
    sock.read_to_string(&mut resp).ok()?;
    let (_, body) = resp.split_once("\r\n\r\n")?;
    serde_json::from_str::<Value>(body.trim()).ok()
}

fn post_shutdown(port: u16, token: &str) -> bool {
    let Ok(addr) = format!("127.0.0.1:{port}").parse() else {
        return false;
    };
    let Ok(mut sock) = TcpStream::connect_timeout(&addr, Duration::from_secs(2)) else {
        return false;
    };
    let _ = sock.set_read_timeout(Some(Duration::from_secs(2)));
    let _ = sock.set_write_timeout(Some(Duration::from_secs(2)));
    let req = format!(
        "POST /__tokprof/shutdown HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nX-Tokprof-Token: {token}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
    );
    if sock.write_all(req.as_bytes()).is_err() {
        return false;
    }
    let mut buf = [0u8; 256];
    let _ = sock.read(&mut buf);
    true
}

pub fn serve(
    out: &str,
    state_file: &str,
    extra_maps: &[String],
    use_tiktoken: bool,
    use_ebpf: bool,
    pids: &str,
    comms: &str,
) -> i32 {
    let env_map = current_env_map();
    let (routes, overrides, azure_hosts) = match build_routes(&env_map, extra_maps, None) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("tokprof: {e}");
            return 2;
        }
    };
    let writer = match JsonlWriter::new(out) {
        Ok(w) => Arc::new(w),
        Err(e) => {
            eprintln!("tokprof: cannot open {out}: {e}");
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
        routes,
        Arc::new(move |rec| w_clone.write_record(&rec)),
        salt,
        HashMap::new(),
        Some(hex::encode(run_b)),
        azure_hosts,
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
    let port = srv.port;

    let mut ebpf_child = None;
    let mut ebpf_thread = None;
    let mut ebpf_scope = None;

    if use_ebpf {
        if let Some(exe) = ebpf_can_run(None, false) {
            let everything = pids.is_empty() && comms.is_empty();
            let cmd = helper_command(
                &exe,
                pids,
                comms,
                everything,
                None,
                true,
                true,
                Some(std::process::id()),
            );
            if let Ok(mut child) = Command::new(&cmd[0])
                .args(&cmd[1..])
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
            {
                let (tx, rx) = mpsc::channel();
                if let Some(stderr) = child.stderr.take() {
                    thread::spawn(move || {
                        let rdr = BufReader::new(stderr);
                        for ln in rdr.lines().map_while(Result::ok) {
                            if ln.trim().ends_with("ready") {
                                let _ = tx.send(true);
                            }
                        }
                    });
                }
                let ready = rx
                    .recv_timeout(Duration::from_secs(10))
                    .unwrap_or(false)
                    && child.try_wait().ok().flatten().is_none();
                if ready {
                    if let Some(stdout) = child.stdout.take() {
                        let gw_ebpf = gw.clone();
                        ebpf_thread = Some(thread::spawn(move || {
                            let mut rdr = BufReader::new(stdout);
                            ingest_lines(&mut rdr, &gw_ebpf);
                        }));
                    }
                    ebpf_scope = Some(if !pids.is_empty() {
                        format!("pid={pids}")
                    } else if !comms.is_empty() {
                        format!("comm={comms}")
                    } else {
                        "all processes & Docker containers".to_string()
                    });
                    ebpf_child = Some(child);
                } else {
                    let _ = child.kill();
                    let _ = child.wait();
                }
            }
        }
    }

    let mut originals = BTreeMap::new();
    for &k in OVERRIDDEN {
        originals.insert(k.to_string(), env_map.get(k).cloned());
    }
    let mut child_env = BTreeMap::new();
    for (k, v) in &overrides {
        child_env.insert(k.clone(), v.replace("{port}", &port.to_string()));
    }
    for (k, v) in hook_env(&env_map, port, &token) {
        child_env.insert(k, v);
    }

    let abs_out = fs::canonicalize(out)
        .unwrap_or_else(|_| PathBuf::from(out))
        .to_string_lossy()
        .into_owned();
    let started_ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0);
    let st_obj = SessionState {
        pid: std::process::id(),
        port,
        out: abs_out,
        started: started_ts,
        token,
        ebpf: ebpf_scope,
        env: child_env,
        originals,
    };
    if let Ok(json_text) = serde_json::to_string(&st_obj) {
        let _ = atomic_write_0600(state_file, &json_text);
    }

    SIG_STOP.store(false, Ordering::SeqCst);
    install_signal_handlers();
    while !gw.shutdown_flag.load(Ordering::SeqCst) && !SIG_STOP.load(Ordering::SeqCst) {
        thread::sleep(Duration::from_millis(25));
    }

    if let Some(mut c) = ebpf_child {
        if let Some(stdin) = c.stdin.take() {
            drop(stdin);
        }
        if let Some(jh) = ebpf_thread {
            let _ = jh.join();
        }
        let _ = c.kill();
        let _ = c.wait();
    }
    srv.stop();
    writer.close();
    let _ = fs::remove_file(state_file);
    0
}

pub fn start(
    out: &str,
    state_file: &str,
    extra_maps: &[String],
    use_tiktoken: bool,
    use_ebpf_opt: Option<bool>,
    pids: &str,
    comms: &str,
) -> i32 {
    if let Some(existing) = read_state(state_file) {
        if pid_alive(existing.pid) && health(existing.port).is_some() {
            eprintln!(
                "tokprof: a session is already running (pid {}, port {}); use `tokprof status`",
                existing.pid, existing.port
            );
            return 1;
        }
        let _ = fs::remove_file(state_file);
    }
    let mut use_ebpf = use_ebpf_opt.unwrap_or(cfg!(target_os = "linux"));
    if use_ebpf {
        use_ebpf = ebpf_can_run(None, true).is_some();
    }

    let self_exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("tokprof"));
    let mut cmd = Command::new(self_exe);
    cmd.args(["serve", "--out", out, "--state", state_file]);
    for m in extra_maps {
        cmd.args(["--map", m]);
    }
    if !use_tiktoken {
        cmd.arg("--no-tiktoken");
    }
    if use_ebpf {
        cmd.arg("--ebpf");
    } else {
        cmd.arg("--no-ebpf");
    }
    if !pids.is_empty() {
        cmd.args(["--pid", pids]);
    }
    if !comms.is_empty() {
        cmd.args(["--comm", comms]);
    }
    cmd.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if let Err(e) = cmd.spawn() {
        eprintln!("tokprof: failed to spawn background session: {e}");
        return 1;
    }

    let deadline = Instant::now() + Duration::from_secs(15);
    let mut sleep_ms = 10u64;
    while Instant::now() < deadline {
        if let Some(st) = read_state(state_file) {
            if health(st.port).is_some() {
                let ebpf_info = st
                    .ebpf
                    .as_ref()
                    .map(|s| format!(", eBPF: {s}"))
                    .unwrap_or_default();
                println!(
                    "tokprof: session started (pid {}, gateway 127.0.0.1:{}{})",
                    st.pid, st.port, ebpf_info
                );
                println!("  recording to: {}", st.out);
                if st.ebpf.is_some() {
                    println!(
                        "  Running host processes, background services, and Docker containers are captured automatically."
                    );
                    println!(
                        "  Check progress: tokprof status   |   Live view: tokprof report --watch   |   Finish & report: tokprof stop"
                    );
                } else {
                    println!(
                        "  Route NEW processes through it in this shell with:  eval \"$(tokprof env)\""
                    );
                    println!("  Stop and view report with: tokprof stop");
                }
                return 0;
            }
        }
        thread::sleep(Duration::from_millis(sleep_ms));
        sleep_ms = (sleep_ms * 3 / 2).min(80);
    }
    eprintln!(
        "tokprof: the session did not start in time; run `tokprof serve` in the foreground to see errors"
    );
    1
}

pub fn status(state_file: &str) -> i32 {
    let Some(st) = read_state(state_file) else {
        println!("tokprof: no active session");
        return 1;
    };
    if !pid_alive(st.pid) {
        println!("tokprof: no active session");
        return 1;
    }
    let h = health(st.port);
    let reqs = h
        .as_ref()
        .and_then(|v| v.get("requests"))
        .and_then(|v| v.as_u64())
        .map(|n| n.to_string())
        .unwrap_or_else(|| "?".to_string());
    let rec = h
        .as_ref()
        .and_then(|v| v.get("recorded"))
        .and_then(|v| v.as_u64())
        .map(|n| n.to_string())
        .unwrap_or_else(|| "?".to_string());
    let errs = h
        .as_ref()
        .and_then(|v| v.get("record_errors"))
        .and_then(|v| v.as_u64())
        .map(|n| n.to_string())
        .unwrap_or_else(|| "?".to_string());
    let ebpf_info = st
        .ebpf
        .as_ref()
        .map(|s| format!(", eBPF: {s}"))
        .unwrap_or_default();
    println!(
        "tokprof: active session pid {}, gateway 127.0.0.1:{}{}, recording to {}",
        st.pid, st.port, ebpf_info, st.out
    );
    println!("  requests seen: {reqs}, calls recorded: {rec}, record errors: {errs}");
    0
}

fn quote_shell(shell: &str, value: &str) -> String {
    match shell {
        "powershell" => format!("'{}'", value.replace('\'', "''")),
        "cmd" => value.to_string(),
        _ => format!("'{}'", value.replace('\'', "'\"'\"'")),
    }
}

pub fn render_env(st: &SessionState, shell: &str, unset: bool) -> String {
    let mut lines = Vec::new();
    if unset {
        for (k, v_opt) in &st.originals {
            match v_opt {
                None => {
                    let cmd = match shell {
                        "powershell" => format!("Remove-Item Env:{k} -ErrorAction SilentlyContinue"),
                        "cmd" => format!("set {k}="),
                        "fish" => format!("set -e {k}"),
                        _ => format!("unset {k}"),
                    };
                    lines.push(cmd);
                }
                Some(v) => {
                    let cmd = match shell {
                        "powershell" => format!("$env:{k} = {}", quote_shell(shell, v)),
                        "cmd" => format!("set {k}={v}"),
                        "fish" => format!("set -gx {k} {}", quote_shell(shell, v)),
                        _ => format!("export {k}={}", quote_shell(shell, v)),
                    };
                    lines.push(cmd);
                }
            }
        }
    } else {
        for (k, v) in &st.env {
            let cmd = match shell {
                "powershell" => format!("$env:{k} = {}", quote_shell(shell, v)),
                "cmd" => format!("set {k}={v}"),
                "fish" => format!("set -gx {k} {}", quote_shell(shell, v)),
                _ => format!("export {k}={}", quote_shell(shell, v)),
            };
            lines.push(cmd);
        }
    }
    lines.join("\n")
}

pub fn detect_shell() -> &'static str {
    if cfg!(windows) {
        if std::env::var("PSModulePath").is_ok() && std::env::var("PROMPT").is_err() {
            "powershell"
        } else {
            "cmd"
        }
    } else {
        let sh = std::env::var("SHELL").unwrap_or_else(|_| "bash".to_string());
        if sh.ends_with("fish") {
            "fish"
        } else {
            "bash"
        }
    }
}

pub fn env_cmd(state_file: &str, shell_opt: Option<&str>, unset: bool) -> i32 {
    let Some(st) = read_state(state_file) else {
        eprintln!("tokprof: no active session; start one with `tokprof start`");
        return 1;
    };
    if !pid_alive(st.pid) {
        eprintln!("tokprof: no active session; start one with `tokprof start`");
        return 1;
    }
    let sh = match shell_opt {
        Some(s) => s,
        None => detect_shell(),
    };
    println!("{}", render_env(&st, sh, unset));
    0
}

pub fn stop(state_file: &str, force: bool) -> i32 {
    let Some(st) = read_state(state_file) else {
        println!("tokprof: no active session");
        return 1;
    };
    if !pid_alive(st.pid) {
        let _ = fs::remove_file(state_file);
        println!("tokprof: no active session");
        return 1;
    }
    let h = health(st.port);
    let age_opt = h
        .as_ref()
        .and_then(|v| v.get("last_request_age_s"))
        .and_then(|v| v.as_f64());
    if !force {
        if let Some(age) = age_opt {
            if age < 30.0 {
                eprintln!(
                    "tokprof: the gateway handled a request {age:.0}s ago; processes started with its environment would fail after it stops. Stop them first, or use --force."
                );
                return 2;
            }
        }
    }
    if !st.token.is_empty() {
        let _ = post_shutdown(st.port, &st.token);
    }
    let deadline = Instant::now() + Duration::from_secs(15);
    let term_at = Instant::now() + Duration::from_secs(3);
    let mut killed = false;
    let mut sleep_ms = 10u64;
    while Instant::now() < deadline && pid_alive(st.pid) {
        if !killed && (st.token.is_empty() || Instant::now() >= term_at) {
            killed = true;
            #[cfg(unix)]
            {
                let _ = Command::new("kill")
                    .args(["-TERM", &st.pid.to_string()])
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status();
            }
            #[cfg(windows)]
            {
                let _ = Command::new("taskkill")
                    .args(["/PID", &st.pid.to_string(), "/F"])
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status();
            }
        }
        thread::sleep(Duration::from_millis(sleep_ms));
        sleep_ms = (sleep_ms * 3 / 2).min(80);
    }
    let _ = fs::remove_file(state_file);
    println!("tokprof: session stopped. Recorded data: {}", st.out);
    if st.ebpf.is_none() || age_opt.is_some() {
        println!(
            "Processes that were started with the session environment still point at the stopped gateway; restart them, or restore this shell with: eval \"$(tokprof env --unset)\" (run it BEFORE stopping next time)"
        );
    }
    0
}
