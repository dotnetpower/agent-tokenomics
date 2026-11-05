use crate::gateway::Gateway;
use crate::store::JsonlWriter;
use rand::RngCore;
use serde_json::Value;
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::Ordering;
use std::sync::Arc;

pub const HELPER_NAME: &str = "tokprof-ebpf";

pub fn which_exe(name: &str) -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_var) {
        let cand = dir.join(name);
        if cand.is_file() {
            return Some(cand);
        }
        #[cfg(windows)]
        {
            let cand_exe = dir.join(format!("{name}.exe"));
            if cand_exe.is_file() {
                return Some(cand_exe);
            }
        }
    }
    None
}

pub fn find_helper(explicit: Option<&str>, auto_build: bool) -> Option<PathBuf> {
    if let Some(exp) = explicit {
        if !exp.is_empty() && Path::new(exp).is_file() {
            return fs::canonicalize(exp).ok().or_else(|| Some(PathBuf::from(exp)));
        }
    }
    if let Ok(env_h) = std::env::var("TOKPROF_EBPF_HELPER") {
        if !env_h.is_empty() && Path::new(&env_h).is_file() {
            return fs::canonicalize(&env_h)
                .ok()
                .or_else(|| Some(PathBuf::from(env_h)));
        }
    }
    if let Some(w) = which_exe(HELPER_NAME) {
        return Some(w);
    }
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(dir) = exe_path.parent() {
            let sibling = dir.join(HELPER_NAME);
            if sibling.is_file() {
                return Some(sibling);
            }
        }
    }
    let repo_cand = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("ebpf")
        .join("bin")
        .join(HELPER_NAME);
    if repo_cand.is_file() {
        return Some(repo_cand);
    }
    if auto_build {
        let ebpf_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("ebpf");
        if ebpf_dir.join("go.mod").is_file() {
            let home = std::env::var("HOME").unwrap_or_default();
            let go_cands = [
                PathBuf::from(format!("{home}/sdk/go/bin/go")),
                PathBuf::from("/usr/local/go/bin/go"),
                which_exe("go").unwrap_or_default(),
            ];
            for go in go_cands {
                if go.is_file() {
                    let _ = std::fs::create_dir_all(repo_cand.parent().unwrap_or(&ebpf_dir));
                    if Command::new(&go)
                        .args(["build", "-o", &repo_cand.to_string_lossy(), "./cmd/tokprof-ebpf"])
                        .current_dir(&ebpf_dir)
                        .status()
                        .map(|s| s.success())
                        .unwrap_or(false)
                        && repo_cand.is_file()
                    {
                        return Some(repo_cand);
                    }
                }
            }
        }
    }
    None
}

use std::fs;

pub fn is_root() -> bool {
    #[cfg(unix)]
    {
        Command::new("id")
            .arg("-u")
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.trim() == "0")
            .unwrap_or(false)
    }
    #[cfg(not(unix))]
    {
        false
    }
}

pub fn ebpf_can_run(explicit: Option<&str>, interactive: bool) -> Option<PathBuf> {
    if !cfg!(target_os = "linux") {
        return None;
    }
    let exe = find_helper(explicit, true)?;
    if is_root() {
        return Some(exe);
    }
    which_exe("sudo")?;
    let ok = Command::new("sudo")
        .args(["-n", &exe.to_string_lossy(), "status"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if ok {
        return Some(exe);
    }
    if interactive
        && std::io::IsTerminal::is_terminal(&std::io::stdin())
        && std::io::IsTerminal::is_terminal(&std::io::stderr())
        && Command::new("sudo")
            .arg("-v")
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    {
        let ok2 = Command::new("sudo")
            .args(["-n", &exe.to_string_lossy(), "status"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if ok2 {
            return Some(exe);
        }
    }
    None
}

#[allow(clippy::too_many_arguments)]
pub fn helper_command(
    helper: &Path,
    pids: &str,
    comms: &str,
    everything: bool,
    duration: Option<&str>,
    use_sudo: bool,
    plain: bool,
    ignore_pid: Option<u32>,
) -> Vec<String> {
    let mut cmd = Vec::new();
    if use_sudo && !is_root() && which_exe("sudo").is_some() {
        cmd.push("sudo".to_string());
        cmd.push("-n".to_string());
    }
    cmd.push(helper.to_string_lossy().into_owned());
    cmd.push("trace".to_string());
    cmd.push("--exit-on-stdin-eof".to_string());
    if !plain {
        cmd.push("--no-plain".to_string());
    }
    if let Some(ip) = ignore_pid {
        cmd.push("--ignore-pid".to_string());
        cmd.push(ip.to_string());
    }
    if !pids.is_empty() {
        cmd.push("--pid".to_string());
        cmd.push(pids.to_string());
    }
    if !comms.is_empty() {
        cmd.push("--comm".to_string());
        cmd.push(comms.to_string());
    }
    if everything {
        cmd.push("--all".to_string());
    }
    if let Some(dur) = duration {
        if !dur.is_empty() {
            cmd.push("--duration".to_string());
            cmd.push(dur.to_string());
        }
    }
    cmd
}

pub fn ingest_lines<R: BufRead>(reader: R, gw: &Gateway) -> usize {
    let mut count = 0usize;
    for line_res in reader.lines() {
        let Ok(line) = line_res else { break };
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Ok(msg) = serde_json::from_str::<Value>(trimmed) {
            gw.ingest_message(&msg);
            count += 1;
        }
    }
    count
}

#[allow(clippy::too_many_arguments)]
pub fn run_ebpf(
    out_path: &str,
    pids: &str,
    comms: &str,
    everything: bool,
    helper: Option<&str>,
    duration: Option<&str>,
    components: HashMap<String, String>,
    use_tiktoken: bool,
    use_sudo: bool,
    plain: bool,
    quiet: bool,
) -> i32 {
    if !cfg!(target_os = "linux") {
        eprintln!(
            "tokprof: eBPF capture is Linux-only. On this OS use `tokprof run` or `tokprof start` (gateway and hooks)."
        );
        return 2;
    }
    if pids.is_empty() && comms.is_empty() && !everything {
        eprintln!("tokprof: choose what to watch: --pid, --comm or --all");
        return 2;
    }
    let Some(exe) = find_helper(helper, true) else {
        eprintln!(
            "tokprof: {HELPER_NAME} not found. Build it with: (cd profiler/ebpf && go build -o bin/{HELPER_NAME} ./cmd/{HELPER_NAME}), or pass --helper PATH."
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
    let w_clone = writer.clone();
    let gw = Gateway::new(
        Vec::new(),
        Arc::new(move |rec| w_clone.write_record(&rec)),
        salt,
        components,
        Some(hex::encode(run_b)),
        Default::default(),
        use_tiktoken,
        None,
        None,
        false,
    );
    let cmd = helper_command(
        &exe,
        pids,
        comms,
        everything,
        duration,
        use_sudo,
        plain,
        Some(std::process::id()),
    );
    if !quiet {
        eprintln!("tokprof: profiling to {out_path} via eBPF; press Ctrl+C to stop");
    }
    let mut child = match Command::new(&cmd[0])
        .args(&cmd[1..])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            eprintln!("tokprof: cannot start the helper: {e}");
            writer.close();
            return 127;
        }
    };
    crate::session::clear_sig_stop();
    crate::session::install_signal_handlers();
    let done_reading = Arc::new( std::sync::atomic::AtomicBool::new(false));
    if let Some(mut stdin) = child.stdin.take() {
        let done_cl = done_reading.clone();
        std::thread::spawn(move || {
            while !crate::session::sig_stop_requested() && !done_cl.load(Ordering::SeqCst) {
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            let _ = stdin.flush();
            drop(stdin);
        });
    }
    if let Some(stdout) = child.stdout.take() {
        let mut reader = BufReader::new(stdout);
        ingest_lines(&mut reader, &gw);
    }
    done_reading.store(true, Ordering::SeqCst);
    let wait_deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while std::time::Instant::now() < wait_deadline {
        if let Ok(Some(_)) = child.try_wait() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    let _ = child.kill();
    let _ = child.wait();
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
    0
}
