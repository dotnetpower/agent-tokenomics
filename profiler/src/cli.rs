use crate::analyze::{compare, summarize, to_legacy};
use crate::appinsights::{
    build_app_insights_kql_ext, build_dependencies_only_kql_ext, checkpoint_path,
    cloud_shell_persistent_dir, inspect_records_content, is_cloud_shell, load_pull_checkpoint,
    merge_and_upgrade_records, obtain_azure_bearer_token, parse_app_insights_payload,
    query_azure_logs_with_resilience, report_candidate_ports, resolve_azure_cloud_endpoints,
    save_pull_checkpoint, strip_records_content, IngestOptions, PullCheckpoint,
};
use crate::detect::explain;
use crate::ebpf::run_ebpf;
use crate::help_text::render_detailed_help;
use crate::mitm::{run_mitm, TRUST_HELP};
use crate::report::{
    render_compare, render_cost, render_csv, render_html, render_json, render_markdown,
    render_pretty, render_text,
};
use crate::runner::{default_shell, run_profiled};
use crate::selftest::run_selftest;
use crate::session;
use crate::store::{atomic_write_0600, read_jsonl, ReadStats};
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::fs;
use std::io::{IsTerminal, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const SUBCOMMANDS: &[&str] = &[
    "profile",
    "run",
    "shell",
    "ebpf",
    "mitm",
    "pull",
    "import",
    "explain",
    "report",
    "compare",
    "export",
    "start",
    "serve",
    "serve-report",
    "status",
    "env",
    "stop",
    "clean",
    "doctor",
    "selftest",
];

fn default_out() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    if let Some(cs_dir) = cloud_shell_persistent_dir() {
        return cs_dir
            .join(format!("run-{secs}.jsonl"))
            .to_string_lossy()
            .into_owned();
    }
    format!(".tokprof/run-{secs}.jsonl")
}

fn parse_duration_secs_f64(spec: &str, default_s: f64) -> f64 {
    let s = spec.trim().to_ascii_lowercase();
    if s.is_empty() {
        return default_s;
    }
    if let Some(ms) = s.strip_suffix("ms") {
        return ms
            .trim()
            .parse::<f64>()
            .map(|v| (v / 1000.0).max(0.01))
            .unwrap_or(default_s);
    }
    if let Some(m) = s.strip_suffix('m') {
        return m
            .trim()
            .parse::<f64>()
            .map(|v| (v * 60.0).max(0.01))
            .unwrap_or(default_s);
    }
    if let Some(sec) = s.strip_suffix('s') {
        return sec
            .trim()
            .parse::<f64>()
            .map(|v| v.max(0.01))
            .unwrap_or(default_s);
    }
    s.parse::<f64>().map(|v| v.max(0.01)).unwrap_or(default_s)
}

fn write_records_atomic(out: &str, records: &[crate::model::CallRecord]) -> std::io::Result<()> {
    if out.to_ascii_lowercase().ends_with(".gz") {
        let tmp = format!("{}.{}.tmp.gz", out, std::process::id());
        let writer = crate::store::JsonlWriter::new(&tmp)?;
        for r in records {
            writer.write_record(r);
        }
        writer.close();
        if fs::rename(&tmp, out).is_err() {
            let bytes = fs::read(&tmp)?;
            let _ = fs::remove_file(&tmp);
            fs::write(out, bytes)?;
        }
        return Ok(());
    }
    let mut buf = String::with_capacity(records.len() * 512);
    for r in records {
        if let Ok(line) = serde_json::to_string(r) {
            buf.push_str(&line);
            buf.push('\n');
        }
    }
    atomic_write_0600(out, &buf)
}

fn find_latest_jsonl(dir: &str) -> Option<String> {
    if let Ok(entries) = fs::read_dir(dir) {
        let mut cands: Vec<(SystemTime, PathBuf)> = entries
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| {
                let nm = p.file_name().and_then(|s| s.to_str()).unwrap_or("");
                p.is_file() && (nm.ends_with(".jsonl") || nm.ends_with(".jsonl.gz"))
            })
            .map(|p| {
                let mt = fs::metadata(&p)
                    .and_then(|m| m.modified())
                    .unwrap_or(UNIX_EPOCH);
                (mt, p)
            })
            .collect();
        cands.sort_by_key(|b| std::cmp::Reverse(b.0));
        if let Some((_, p)) = cands.first() {
            return Some(p.to_string_lossy().into_owned());
        }
    }
    let st_path = session::state_path(None);
    if let Some(st) = session::read_state(&st_path) {
        if Path::new(&st.out).is_file() {
            return Some(st.out);
        }
    }
    None
}

fn load_components(specs: &[String]) -> Result<HashMap<String, String>, String> {
    let mut out = HashMap::new();
    for spec in specs {
        let Some((name, val)) = spec.split_once('=') else {
            return Err(format!(
                "--component expects NAME=@FILE or NAME=TEXT, got {spec:?}"
            ));
        };
        if name.is_empty() || val.is_empty() {
            return Err(format!(
                "--component expects NAME=@FILE or NAME=TEXT, got {spec:?}"
            ));
        }
        let text = if let Some(path) = val.strip_prefix('@') {
            fs::read_to_string(path).map_err(|e| format!("cannot read {path}: {e}"))?
        } else {
            val.to_string()
        };
        out.insert(name.to_string(), text);
    }
    Ok(out)
}

fn load_prices(path_opt: Option<&str>) -> Option<Map<String, Value>> {
    let p = path_opt?.trim();
    if p.is_empty() {
        return None;
    }
    if matches!(p, "auto" | "builtin" | "default") {
        return Some(crate::analyze::builtin_prices());
    }
    let raw = fs::read_to_string(p).ok()?;
    let val: Value = serde_json::from_str(&raw).ok()?;
    let user_map = val.as_object()?;
    let mut merged = crate::analyze::builtin_prices();
    for (k, v) in user_map {
        merged.insert(k.clone(), v.clone());
    }
    Some(merged)
}

fn open_url_in_browser(url: &str) {
    if std::env::var("CI").is_ok()
        || std::env::var("GITHUB_ACTIONS").is_ok()
        || std::env::var("TOKPROF_NO_BROWSER").is_ok()
        || is_cloud_shell()
    {
        return;
    }
    #[cfg(target_os = "macos")]
    let _ = Command::new("open")
        .arg(url)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
    #[cfg(target_os = "linux")]
    let _ = Command::new("xdg-open")
        .arg(url)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
    #[cfg(windows)]
    let _ = Command::new("cmd")
        .args(["/C", "start", "", url])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
}

fn probe_report_port(port: u16, expected_dir: &str, file_name: &str) -> bool {
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let Ok(mut s) = TcpStream::connect_timeout(&addr, Duration::from_millis(120)) else {
        return false;
    };
    let _ = s.set_read_timeout(Some(Duration::from_millis(200)));
    let _ = s.set_write_timeout(Some(Duration::from_millis(200)));
    let req = format!(
        "GET /__tokprof/report-dir HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n"
    );
    if s.write_all(req.as_bytes()).is_ok() {
        let mut resp = String::new();
        if s.read_to_string(&mut resp).is_ok() && resp.starts_with("HTTP/1.1 200") {
            if let Some((_, body)) = resp.split_once("\r\n\r\n") {
                return body.trim() == expected_dir;
            }
        }
    }
    // Fallback: check if an existing static HTTP server on 18765 is already serving this file
    if port == 18765 {
        if let Ok(mut s2) = TcpStream::connect_timeout(&addr, Duration::from_millis(120)) {
            let _ = s2.set_read_timeout(Some(Duration::from_millis(200)));
            let _ = s2.set_write_timeout(Some(Duration::from_millis(200)));
            let req2 = format!(
                "HEAD /{file_name} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n"
            );
            if s2.write_all(req2.as_bytes()).is_ok() {
                let mut r2 = String::new();
                if s2.read_to_string(&mut r2).is_ok()
                    && (r2.starts_with("HTTP/1.0 200") || r2.starts_with("HTTP/1.1 200"))
                {
                    return true;
                }
            }
        }
    }
    false
}

pub fn ensure_report_server(html_path: &str) -> Option<String> {
    if std::env::var("CI").is_ok()
        || std::env::var("GITHUB_ACTIONS").is_ok()
        || std::env::var("TOKPROF_NO_SERVER").is_ok()
    {
        return None;
    }
    let abs = fs::canonicalize(html_path).unwrap_or_else(|_| PathBuf::from(html_path));
    let dir = abs.parent()?;
    let dir_str = dir.to_string_lossy().into_owned();
    let file_name = abs.file_name()?.to_string_lossy().into_owned();
    let cloud_shell = is_cloud_shell();
    let cand_ports = report_candidate_ports(None, cloud_shell);

    for &port in &cand_ports {
        if probe_report_port(port, &dir_str, &file_name) {
            return Some(format!("http://127.0.0.1:{port}/{file_name}"));
        }
    }

    let mut chosen_port = None;
    for &port in &cand_ports {
        if let Ok(listener) = TcpListener::bind(("127.0.0.1", port)) {
            drop(listener);
            chosen_port = Some(port);
            break;
        }
    }
    let port = chosen_port?;
    let self_exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("tokprof"));
    let _ = Command::new(self_exe)
        .args([
            "serve-report",
            "--out",
            &dir_str,
            "--state",
            &port.to_string(),
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;

    let deadline = Instant::now() + Duration::from_millis(600);
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    while Instant::now() < deadline {
        if TcpStream::connect_timeout(&addr, Duration::from_millis(50)).is_ok() {
            return Some(format!("http://127.0.0.1:{port}/{file_name}"));
        }
        thread::sleep(Duration::from_millis(20));
    }
    Some(format!("http://127.0.0.1:{port}/{file_name}"))
}

pub fn serve_report_http(dir: &str, port: u16) -> i32 {
    let Ok(listener) = TcpListener::bind(("127.0.0.1", port)) else {
        return 1;
    };
    let _ = listener.set_nonblocking(true);
    let canon_dir = fs::canonicalize(dir)
        .unwrap_or_else(|_| PathBuf::from(dir))
        .to_string_lossy()
        .into_owned();
    let mut last_active = Instant::now();
    let idle_limit = Duration::from_secs(1800);

    while last_active.elapsed() < idle_limit {
        match listener.accept() {
            Ok((mut stream, _)) => {
                last_active = Instant::now();
                let _ = stream.set_nonblocking(false);
                let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
                let _ = stream.set_write_timeout(Some(Duration::from_secs(2)));
                let mut buf = [0u8; 4096];
                let Ok(n) = stream.read(&mut buf) else {
                    continue;
                };
                if n == 0 {
                    continue;
                }
                let req = String::from_utf8_lossy(&buf[..n]);
                let first_line = req.lines().next().unwrap_or("");
                let mut parts = first_line.split_whitespace();
                let method = parts.next().unwrap_or("GET");
                let raw_target = parts.next().unwrap_or("/");
                let path_part = raw_target.split('?').next().unwrap_or("/");

                 if path_part == "/__tokprof/mtime" {
                    let latest_m = fs::metadata(Path::new(&canon_dir).join("latest.html"))
                        .and_then(|m| m.modified())
                        .ok()
                        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                        .map(|d| d.as_millis())
                        .unwrap_or(0);
                    let body = format!("{{\"mtime_ms\":{latest_m}}}");
                    let head = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nCache-Control: no-store\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        body.len()
                    );
                    let _ = stream.write_all(head.as_bytes());
                    let _ = stream.write_all(body.as_bytes());
                    continue;
                }
                if path_part == "/__tokprof/report-dir" {
                    let body = canon_dir.as_bytes();
                    let head = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        body.len()
                    );
                    let _ = stream.write_all(head.as_bytes());
                    let _ = stream.write_all(body);
                    continue;
                }
                if path_part == "/__tokprof/shutdown" && method == "POST" {
                    let _ = stream.write_all(b"HTTP/1.1 204 No Content\r\nConnection: close\r\n\r\n");
                    break;
                }

                let trimmed = path_part.trim_start_matches('/');
                let rel_name = if trimmed.is_empty() {
                    "latest.html"
                } else {
                    trimmed
                };
                if rel_name.contains("..")
                    || rel_name.contains('/')
                    || rel_name.contains('\\')
                    || !(rel_name.ends_with(".html") || rel_name.ends_with(".csv"))
                {
                    let _ = stream.write_all(b"HTTP/1.1 403 Forbidden\r\nX-Content-Type-Options: nosniff\r\nContent-Length: 9\r\nConnection: close\r\n\r\nForbidden");
                    continue;
                }
                let base_dir_path = Path::new(&canon_dir);
                let file_path = base_dir_path.join(rel_name);
                let Ok(canon_file) = fs::canonicalize(&file_path) else {
                    let _ = stream.write_all(b"HTTP/1.1 404 Not Found\r\nX-Content-Type-Options: nosniff\r\nContent-Length: 9\r\nConnection: close\r\n\r\nNot Found");
                    continue;
                };
                if !canon_file.starts_with(base_dir_path) {
                    let _ = stream.write_all(b"HTTP/1.1 403 Forbidden\r\nX-Content-Type-Options: nosniff\r\nContent-Length: 9\r\nConnection: close\r\n\r\nForbidden");
                    continue;
                }
                match fs::read(&canon_file) {
                    Ok(bytes) => {
                        let ctype = if rel_name.ends_with(".html") {
                            "text/html; charset=utf-8"
                        } else {
                            "text/csv; charset=utf-8"
                        };
                        let head = format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: {ctype}\r\nX-Content-Type-Options: nosniff\r\nCache-Control: no-store\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            bytes.len()
                        );
                        let _ = stream.write_all(head.as_bytes());
                        if method != "HEAD" {
                            let _ = stream.write_all(&bytes);
                        }
                    }
                    Err(_) => {
                        let _ = stream.write_all(b"HTTP/1.1 404 Not Found\r\nX-Content-Type-Options: nosniff\r\nContent-Length: 9\r\nConnection: close\r\n\r\nNot Found");
                    }
                }
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(40));
            }
            Err(_) => {
                thread::sleep(Duration::from_millis(40));
            }
        }
    }
    0
}

#[allow(clippy::too_many_arguments)]
fn write_html_dashboard(
    jsonl_path: &str,
    html_arg: &str,
    gap: f64,
    prices_opt: Option<&str>,
    open_browser: bool,
    records_opt: Option<&[crate::model::CallRecord]>,
    summary_opt: Option<&crate::analyze::SummaryReport>,
) -> Option<String> {
    let owned_recs;
    let records = match records_opt {
        Some(r) => r,
        None => {
            if !Path::new(jsonl_path).exists() {
                return None;
            }
            owned_recs = read_jsonl(jsonl_path, None);
            &owned_recs
        }
    };
    if records.is_empty() {
        return None;
    }
    let stem = jsonl_path.strip_suffix(".jsonl").unwrap_or(jsonl_path);
    let dest = if html_arg.is_empty() {
        format!("{stem}.html")
    } else {
        html_arg.to_string()
    };
    let pmap = load_prices(prices_opt);
    let content = render_html(
        records,
        gap,
        pmap.as_ref(),
        "tokprof — LLM Token Profile",
        summary_opt,
    );
    let _ = atomic_write_0600(&dest, &content);
    let mut serve_target = dest.clone();
    if html_arg.is_empty() {
        let parent = Path::new(jsonl_path).parent();
        if parent
            .and_then(|p| p.file_name())
            .and_then(|s| s.to_str())
            == Some(".tokprof")
        {
            if let Some(p) = parent {
                let latest = p.join("latest.html").to_string_lossy().into_owned();
                let _ = atomic_write_0600(&latest, &content);
                serve_target = latest;
            }
        }
    }
    eprintln!("tokprof: HTML dashboard written to {dest}");
    if let Some(url) = ensure_report_server(&serve_target) {
        eprintln!("tokprof: Report server live at {url}");
        if is_cloud_shell() {
            let port_str = url
                .split("127.0.0.1:")
                .nth(1)
                .and_then(|s| s.split('/').next())
                .unwrap_or("8080");
            eprintln!(
                "tokprof: Azure Cloud Shell detected — open [Web Preview] -> [Configure] on port {port_str}, or run: download {dest}"
            );
        }
        open_url_in_browser(&url);
    } else if is_cloud_shell() {
        eprintln!(
            "tokprof: Azure Cloud Shell detected — download the HTML dashboard with: download {dest}"
        );
    } else if open_browser {
        let abs = fs::canonicalize(&dest)
            .unwrap_or_else(|_| PathBuf::from(&dest))
            .to_string_lossy()
            .into_owned();
        open_url_in_browser(&format!("file://{abs}"));
    }
    Some(dest)
}

fn write_csv_report(
    jsonl_path: &str,
    csv_arg: &str,
    gap: f64,
    records_opt: Option<&[crate::model::CallRecord]>,
    summary_opt: Option<&crate::analyze::SummaryReport>,
) -> Option<String> {
    let owned_recs;
    let records = match records_opt {
        Some(r) => r,
        None => {
            if !Path::new(jsonl_path).exists() {
                return None;
            }
            owned_recs = read_jsonl(jsonl_path, None);
            &owned_recs
        }
    };
    if records.is_empty() {
        return None;
    }
    let stem = jsonl_path.strip_suffix(".jsonl").unwrap_or(jsonl_path);
    let dest = if csv_arg.is_empty() {
        format!("{stem}.csv")
    } else {
        csv_arg.to_string()
    };
    let content = format!("{}\n", render_csv(records, gap, summary_opt));
    let _ = atomic_write_0600(&dest, &content);
    if csv_arg.is_empty() {
        let parent = Path::new(jsonl_path).parent();
        if parent
            .and_then(|p| p.file_name())
            .and_then(|s| s.to_str())
            == Some(".tokprof")
        {
            if let Some(p) = parent {
                let latest = p.join("latest.csv").to_string_lossy().into_owned();
                let _ = atomic_write_0600(&latest, &content);
            }
        }
    }
    eprintln!("tokprof: CSV report written to {dest}");
    Some(dest)
}

fn read_text_or_gz(path: &str) -> std::io::Result<String> {
    if path == "-" {
        let mut s = String::new();
        std::io::stdin().read_to_string(&mut s)?;
        return Ok(s);
    }
    let raw = fs::read(path)?;
    if path.to_ascii_lowercase().ends_with(".gz")
        || (raw.len() >= 2 && raw[0] == 0x1f && raw[1] == 0x8b)
    {
        let mut dec = flate2::read::MultiGzDecoder::new(&raw[..]);
        let mut out = String::new();
        dec.read_to_string(&mut out)?;
        return Ok(out);
    }
    Ok(String::from_utf8_lossy(&raw).into_owned())
}

#[allow(clippy::too_many_arguments)]
fn run_report(
    paths: &[String],
    fmt: &str,
    gap: f64,
    prices: Option<&str>,
    out_file: Option<&str>,
    html_arg: Option<&str>,
    csv_arg: Option<&str>,
    open_browser: bool,
    watch: bool,
    full_prompts: bool,
    conv_filter: Option<&str>,
) -> i32 {
    let resolved_path = match paths.first().map(|s| s.as_str()).filter(|s| !s.is_empty()) {
        Some(p) => p.to_string(),
        None => match find_latest_jsonl(".tokprof") {
            Some(p) => p,
            None => {
                eprintln!(
                    "tokprof: no recorded JSONL file found in .tokprof/; pass a file path or run `tokprof profile -- <cmd>` first"
                );
                return 2;
            }
        },
    };

    if watch {
        use std::io::{BufReader, Seek, SeekFrom};
        let mut offset = 0u64;
        let mut acc_records: Vec<crate::model::CallRecord> = Vec::new();
        let mut rendered_once = false;
        let price_map = load_prices(prices);
        let use_color = std::io::stdout().is_terminal() && std::env::var("NO_COLOR").is_err();
        loop {
            let meta_len = fs::metadata(&resolved_path).map(|m| m.len()).ok();
            let mut changed = false;
            if let Some(cur_len) = meta_len {
                if cur_len < offset {
                    offset = 0;
                    acc_records.clear();
                    changed = true;
                }
                if cur_len > offset {
                    if resolved_path.ends_with(".gz") {
                        acc_records = read_jsonl(&resolved_path, None);
                        offset = cur_len;
                        changed = true;
                    } else if let Ok(mut f) = fs::File::open(&resolved_path) {
                        if f.seek(SeekFrom::Start(offset)).is_ok() {
                            let mut rdr = BufReader::new(f);
                            let delta = crate::store::read_jsonl_from_bufread(&mut rdr, None);
                            offset = cur_len;
                            if !delta.is_empty() {
                                acc_records.extend(delta);
                                changed = true;
                            }
                        }
                    }
                }
            }
            if changed || !rendered_once {
                rendered_once = true;
                print!("\x1b[H\x1b[J");
                if acc_records.is_empty() {
                    println!("Waiting for first LLM call in {resolved_path}...");
                } else {
                    let sum = summarize(&acc_records, gap);
                    println!(
                        "{}",
                        render_pretty(
                            &acc_records,
                            gap,
                            use_color,
                            price_map.as_ref(),
                            full_prompts,
                            Some(&sum),
                        )
                    );
                }
                eprintln!("\nWatching {resolved_path} ({} calls; Ctrl+C to stop)...", acc_records.len());
            }
            if session::sig_stop_requested() {
                break;
            }
            thread::sleep(Duration::from_millis(500));
        }
        return 0;
    }

    let mut stats = ReadStats::default();
    let mut records = if paths.len() > 1 {
        let mut merged = Vec::new();
        for p in paths {
            merged.extend(read_jsonl(p, Some(&mut stats)));
        }
        merged
    } else {
        read_jsonl(&resolved_path, Some(&mut stats))
    };
    if let Some(cf) = conv_filter.map(|s| s.trim()).filter(|s| !s.is_empty()) {
        records.retain(|r| {
            r.conv.as_deref().is_some_and(|c| c.contains(cf))
                || r.run.as_deref().is_some_and(|rn| rn.contains(cf))
                || r.id.contains(cf)
        });
    }
    if std::env::var("TOKPROF_NO_CONTENT").ok().as_deref() == Some("1") {
        strip_records_content(&mut records);
    }
    let price_map = load_prices(prices);
    let summary = if records.is_empty() {
        None
    } else {
        Some(summarize(&records, gap))
    };
    let use_color = std::io::stdout().is_terminal() && std::env::var("NO_COLOR").is_err();

    let text = match fmt {
        "html" => render_html(
            &records,
            gap,
            price_map.as_ref(),
            "tokprof — LLM Token Profile",
            summary.as_ref(),
        ),
        "csv" => render_csv(&records, gap, summary.as_ref()),
        "pretty" => render_pretty(
            &records,
            gap,
            use_color,
            price_map.as_ref(),
            full_prompts,
            summary.as_ref(),
        ),
        "text" => render_text(&records, gap, full_prompts, summary.as_ref()),
        "md" => render_markdown(&records, gap, summary.as_ref()),
        "json" => render_json(&records, gap, summary.as_ref()),
        _ => render_pretty(
            &records,
            gap,
            use_color,
            price_map.as_ref(),
            full_prompts,
            summary.as_ref(),
        ),
    };

    if let Some(of) = out_file.filter(|s| !s.is_empty()) {
        let with_nl = if text.ends_with('\n') {
            text.clone()
        } else {
            format!("{text}\n")
        };
        let _ = atomic_write_0600(of, &with_nl);
    } else {
        println!("{text}");
        if let Some(ref pmap) = price_map {
            if !records.is_empty() && matches!(fmt, "text" | "md") {
                println!("\n{}", render_cost(&records, pmap));
            }
        }
    }

    if let Some(ha) = html_arg {
        write_html_dashboard(
            &resolved_path,
            ha,
            gap,
            prices,
            open_browser,
            Some(&records),
            summary.as_ref(),
        );
    } else if open_browser && fmt == "html" {
        if let Some(of) = out_file {
            if let Some(url) = ensure_report_server(of) {
                eprintln!("tokprof: Report server live at {url}");
                open_url_in_browser(&url);
            }
        }
    }

    if let Some(ca) = csv_arg {
        write_csv_report(&resolved_path, ca, gap, Some(&records), summary.as_ref());
    }

    if stats.bad_lines > 0 {
        eprintln!("\n({} unreadable line(s) skipped)", stats.bad_lines);
    }
    0
}

#[derive(Default)]
struct CliOpts {
    cmd: Option<String>,
    lang: Option<String>,
    help: bool,
    version: bool,
    pid: String,
    comm: String,
    all: bool,
    duration: Option<String>,
    out: Option<String>,
    state: Option<String>,
    session_name: Option<String>,
    html: Option<String>,
    no_html: bool,
    csv: Option<String>,
    no_csv: bool,
    open: bool,
    prices: Option<String>,
    components: Vec<String>,
    maps: Vec<String>,
    no_tiktoken: bool,
    no_hooks: bool,
    no_gateway_env: bool,
    report: bool,
    no_report: bool,
    helper: Option<String>,
    no_sudo: bool,
    no_plain: bool,
    local: String,
    mode: String,
    mitmdump: Option<String>,
    trust_help: bool,
    body_file: Option<String>,
    response_file: Option<String>,
    sse: bool,
    format: String,
    watch: bool,
    full_prompts: bool,
    gap: f64,
    json_flag: bool,
    ebpf: bool,
    no_ebpf: bool,
    shell_kind: Option<String>,
    unset: bool,
    force: bool,
    llm_hosts: Vec<String>,
    llm_paths: Vec<String>,
    exclude_hosts: Vec<String>,
    upstream_ca: Option<String>,
    upstream_timeout: Option<String>,
    insecure_upstream: bool,
    no_auto_env: bool,
    record_hosts: bool,
    no_content: bool,
    usage_only: bool,
    limit: Option<usize>,
    tokenizer: Option<String>,
    workspace: Option<String>,
    app_insights: Option<String>,
    resource_id: Option<String>,
    cloud: Option<String>,
    since: Option<String>,
    conv: Option<String>,
    endpoint: Option<String>,
    token: Option<String>,
    kql: Option<String>,
    print_kql: bool,
    wait_ingest: Option<String>,
    poll_interval: Option<String>,
    min_calls: Option<usize>,
    resume: bool,
    listen: Option<String>,
    port: Option<u16>,
    max_regression: Option<f64>,
    redact_pii: bool,
    redact_pattern: Option<String>,
    keep: Option<usize>,
    positionals: Vec<String>,
    command_tail: Vec<String>,
}

fn parse_args(argv: &[String]) -> Result<CliOpts, String> {
    let mut opts = CliOpts {
        format: "pretty".to_string(),
        gap: 5.0,
        ..Default::default()
    };

    let (raw_pre, tail) = if let Some(idx) = argv.iter().position(|x| x == "--") {
        (&argv[..idx], argv[idx + 1..].to_vec())
    } else {
        (argv, Vec::new())
    };
    opts.command_tail = tail;

    let mut pre = Vec::with_capacity(raw_pre.len() + 4);
    let mut subcmd_seen: Option<String> = None;
    let mut in_inline_cmd = false;
    for tok in raw_pre {
        if !in_inline_cmd {
            if tok.starts_with("--") && tok.contains('=') {
                if let Some((k, v)) = tok.split_once('=') {
                    pre.push(k.to_string());
                    pre.push(v.to_string());
                    continue;
                }
            }
            if subcmd_seen.is_none() && !tok.starts_with('-') && SUBCOMMANDS.contains(&tok.as_str()) {
                subcmd_seen = Some(tok.clone());
                pre.push(tok.clone());
                continue;
            }
            if matches!(subcmd_seen.as_deref(), Some("profile" | "run" | "shell"))
                && !tok.starts_with('-')
            {
                let prev = pre.last().map(|s| s.as_str()).unwrap_or("");
                let prev_takes_val = matches!(
                    prev,
                    "-l"
                        | "--lang"
                        | "--pid"
                        | "--comm"
                        | "--duration"
                        | "--out"
                        | "--prices"
                        | "--component"
                        | "--map"
                        | "--llm-host"
                        | "--llm-path"
                        | "--exclude-host"
                        | "--upstream-ca"
                        | "--upstream-timeout"
                        | "--tokenizer"
                );
                if !prev_takes_val {
                    in_inline_cmd = true;
                }
            }
        }
        pre.push(tok.clone());
    }

    let mut i = 0usize;
    while i < pre.len() {
        let tok = &pre[i];
        match tok.as_str() {
            "-h" | "--help" => opts.help = true,
            "--version" => opts.version = true,
            "-l" | "--lang" => {
                i += 1;
                opts.lang = pre.get(i).cloned();
            }
            _ if tok.starts_with("--lang=") => {
                opts.lang = tok.split_once('=').map(|(_, v)| v.to_string());
            }
            "--pid" => {
                i += 1;
                opts.pid = pre.get(i).cloned().unwrap_or_default();
            }
            "--comm" => {
                i += 1;
                opts.comm = pre.get(i).cloned().unwrap_or_default();
            }
            "--all" => opts.all = true,
            "--duration" => {
                i += 1;
                opts.duration = pre.get(i).cloned();
            }
            "--out" => {
                i += 1;
                opts.out = pre.get(i).cloned();
            }
            "--state" => {
                i += 1;
                opts.state = pre.get(i).cloned();
            }
            "--session" | "--session-name" => {
                i += 1;
                opts.session_name = pre.get(i).cloned();
            }
            "--html" => {
                if let Some(nxt) = pre.get(i + 1) {
                    if !nxt.starts_with('-') && !SUBCOMMANDS.contains(&nxt.as_str()) {
                        opts.html = Some(nxt.clone());
                        i += 1;
                    } else {
                        opts.html = Some(String::new());
                    }
                } else {
                    opts.html = Some(String::new());
                }
            }
            "--no-html" => opts.no_html = true,
            "--csv" => {
                if let Some(nxt) = pre.get(i + 1) {
                    if !nxt.starts_with('-') && !SUBCOMMANDS.contains(&nxt.as_str()) {
                        opts.csv = Some(nxt.clone());
                        i += 1;
                    } else {
                        opts.csv = Some(String::new());
                    }
                } else {
                    opts.csv = Some(String::new());
                }
            }
            "--no-csv" => opts.no_csv = true,
            "--open" => opts.open = true,
            "--prices" => {
                i += 1;
                opts.prices = pre.get(i).cloned();
            }
            "--component" => {
                i += 1;
                if let Some(v) = pre.get(i) {
                    opts.components.push(v.clone());
                }
            }
            "--map" => {
                i += 1;
                if let Some(v) = pre.get(i) {
                    opts.maps.push(v.clone());
                }
            }
            "--no-tiktoken" => opts.no_tiktoken = true,
            "--no-hooks" => opts.no_hooks = true,
            "--no-gateway-env" => opts.no_gateway_env = true,
            "--report" => opts.report = true,
            "--no-report" => opts.no_report = true,
            "--helper" => {
                i += 1;
                opts.helper = pre.get(i).cloned();
            }
            "--no-sudo" => opts.no_sudo = true,
            "--no-plain" => opts.no_plain = true,
            "--local" => {
                i += 1;
                opts.local = pre.get(i).cloned().unwrap_or_default();
            }
            "--mode" => {
                i += 1;
                opts.mode = pre.get(i).cloned().unwrap_or_default();
            }
            "--mitmdump" => {
                i += 1;
                opts.mitmdump = pre.get(i).cloned();
            }
            "--trust-help" => opts.trust_help = true,
            "--body" => {
                i += 1;
                opts.body_file = pre.get(i).cloned();
            }
            "--response" => {
                i += 1;
                opts.response_file = pre.get(i).cloned();
            }
            "--sse" => opts.sse = true,
            "--format" => {
                i += 1;
                if let Some(v) = pre.get(i) {
                    opts.format = v.clone();
                }
            }
            "--watch" => opts.watch = true,
            "--full-prompts" => opts.full_prompts = true,
            "--gap" => {
                i += 1;
                if let Some(v) = pre.get(i) {
                    opts.gap = v.parse::<f64>().unwrap_or(5.0);
                }
            }
            "--json" => opts.json_flag = true,
            "--ebpf" => opts.ebpf = true,
            "--no-ebpf" => opts.no_ebpf = true,
            "--shell" => {
                i += 1;
                opts.shell_kind = pre.get(i).cloned();
            }
            "--unset" => opts.unset = true,
            "--force" => opts.force = true,
            "--llm-host" => {
                i += 1;
                if let Some(v) = pre.get(i) {
                    opts.llm_hosts.push(v.clone());
                }
            }
            "--llm-path" => {
                i += 1;
                if let Some(v) = pre.get(i) {
                    opts.llm_paths.push(v.clone());
                }
            }
            "--exclude-host" => {
                i += 1;
                if let Some(v) = pre.get(i) {
                    opts.exclude_hosts.push(v.clone());
                }
            }
            "--upstream-ca" => {
                i += 1;
                opts.upstream_ca = pre.get(i).cloned();
            }
            "--upstream-timeout" => {
                i += 1;
                opts.upstream_timeout = pre.get(i).cloned();
            }
            "--insecure-upstream" => opts.insecure_upstream = true,
            "--no-auto-env" => opts.no_auto_env = true,
            "--record-hosts" => opts.record_hosts = true,
            "--no-content" => opts.no_content = true,
            "--usage-only" => opts.usage_only = true,
            "--limit" => {
                i += 1;
                opts.limit = pre.get(i).and_then(|s| s.parse::<usize>().ok());
            }
            "--tokenizer" => {
                i += 1;
                opts.tokenizer = pre.get(i).cloned();
            }
            "--workspace" => {
                i += 1;
                opts.workspace = pre.get(i).cloned();
            }
            "--app-insights" => {
                i += 1;
                opts.app_insights = pre.get(i).cloned();
            }
            "--resource-id" => {
                i += 1;
                opts.resource_id = pre.get(i).cloned();
            }
            "--cloud" => {
                i += 1;
                opts.cloud = pre.get(i).cloned();
            }
            "--since" => {
                i += 1;
                opts.since = pre.get(i).cloned();
            }
            "--conv" => {
                i += 1;
                opts.conv = pre.get(i).cloned();
            }
            "--endpoint" => {
                i += 1;
                opts.endpoint = pre.get(i).cloned();
            }
            "--token" => {
                i += 1;
                opts.token = pre.get(i).cloned();
            }
            "--kql" => {
                i += 1;
                opts.kql = pre.get(i).cloned();
            }
            "--print-kql" => opts.print_kql = true,
            "--wait-ingest" => {
                i += 1;
                opts.wait_ingest = pre.get(i).cloned();
            }
            "--poll-interval" => {
                i += 1;
                opts.poll_interval = pre.get(i).cloned();
            }
            "--min-calls" => {
                i += 1;
                opts.min_calls = pre.get(i).and_then(|s| s.parse::<usize>().ok());
            }
            "--resume" => opts.resume = true,
            "--listen" => {
                i += 1;
                opts.listen = pre.get(i).cloned();
            }
            "--port" => {
                i += 1;
                opts.port = pre.get(i).and_then(|s| s.parse::<u16>().ok());
            }
            "--max-regression" | "--fail-on-regression" => {
                i += 1;
                opts.max_regression = pre
                    .get(i)
                    .and_then(|s| s.trim_end_matches('%').parse::<f64>().ok());
            }
            "--redact-pii" => opts.redact_pii = true,
            "--redact-pattern" => {
                i += 1;
                opts.redact_pattern = pre.get(i).cloned();
            }
            "--keep" => {
                i += 1;
                opts.keep = pre.get(i).and_then(|s| s.parse::<usize>().ok());
            }
            _ => {
                if opts.cmd.is_none() && !tok.starts_with('-') && SUBCOMMANDS.contains(&tok.as_str()) {
                    opts.cmd = Some(tok.clone());
                } else if matches!(opts.cmd.as_deref(), Some("profile" | "run" | "shell"))
                    && opts.command_tail.is_empty()
                {
                    opts.command_tail.extend_from_slice(&pre[i..]);
                    break;
                } else {
                    opts.positionals.push(tok.clone());
                }
            }
        }
        i += 1;
    }

    if opts.cmd.is_none() && !opts.command_tail.is_empty() {
        opts.cmd = Some("profile".to_string());
    }
    Ok(opts)
}

fn apply_detect_env(opts: &CliOpts) {
    if !opts.llm_hosts.is_empty() {
        let prev = std::env::var("TOKPROF_LLM_HOSTS").unwrap_or_default();
        let combined = if prev.is_empty() {
            opts.llm_hosts.join(",")
        } else {
            format!("{prev},{}", opts.llm_hosts.join(","))
        };
        std::env::set_var("TOKPROF_LLM_HOSTS", combined);
    }
    if !opts.llm_paths.is_empty() {
        let prev = std::env::var("TOKPROF_LLM_PATHS").unwrap_or_default();
        let combined = if prev.is_empty() {
            opts.llm_paths.join(",")
        } else {
            format!("{prev},{}", opts.llm_paths.join(","))
        };
        std::env::set_var("TOKPROF_LLM_PATHS", combined);
    }
    if !opts.exclude_hosts.is_empty() {
        let prev = std::env::var("TOKPROF_EXCLUDE_HOSTS").unwrap_or_default();
        let combined = if prev.is_empty() {
            opts.exclude_hosts.join(",")
        } else {
            format!("{prev},{}", opts.exclude_hosts.join(","))
        };
        std::env::set_var("TOKPROF_EXCLUDE_HOSTS", combined);
    }
    if let Some(ref ca) = opts.upstream_ca {
        let sep = if cfg!(windows) { ";" } else { ":" };
        let prev = std::env::var("TOKPROF_UPSTREAM_CA").unwrap_or_default();
        let combined = if prev.is_empty() {
            ca.clone()
        } else {
            format!("{prev}{sep}{ca}")
        };
        std::env::set_var("TOKPROF_UPSTREAM_CA", combined);
    }
    if let Some(ref lg) = opts.lang {
        std::env::set_var("TOKPROF_LANG", lg);
    }
    if opts.redact_pii {
        std::env::set_var("TOKPROF_REDACT_PII", "1");
    }
    if let Some(ref rp) = opts.redact_pattern {
        std::env::set_var("TOKPROF_REDACT_PATTERN", rp);
    }
    if let Some(ref sn) = opts.session_name {
        std::env::set_var("TOKPROF_SESSION", sn);
    }
    if let Some(ref dur) = opts.duration {
        std::env::set_var("TOKPROF_MAX_DURATION", dur);
    }
    if let Some(ref ut) = opts.upstream_timeout {
        std::env::set_var("TOKPROF_UPSTREAM_TIMEOUT", ut);
    }
    if opts.insecure_upstream {
        std::env::set_var("TOKPROF_INSECURE_UPSTREAM", "1");
    }
    if opts.no_auto_env {
        std::env::set_var("TOKPROF_NO_AUTO_ENV", "1");
    }
    if opts.record_hosts {
        std::env::set_var("TOKPROF_RECORD_HOSTS", "1");
    }
    if opts.no_content {
        std::env::set_var("TOKPROF_NO_CONTENT", "1");
    }
    if let Some(ref tok) = opts.tokenizer {
        std::env::set_var("TOKPROF_TOKENIZER", tok);
    }
    if let Some(ref l) = opts.listen {
        std::env::set_var("TOKPROF_LISTEN", l);
    }
    if let Some(p) = opts.port {
        std::env::set_var("TOKPROF_REPORT_PORT", p.to_string());
    }
    if let Some(ref t) = opts.token {
        std::env::set_var("TOKPROF_INGEST_TOKEN", t);
    }
}

fn finish_run(out: &str, opts: &CliOpts, default_report: bool, pretty: bool) {
    if !Path::new(out).exists() {
        return;
    }
    let want_report = (opts.report || default_report) && !opts.no_report;
    let html_eff = if default_report && opts.html.is_none() && !opts.no_html {
        Some("")
    } else if opts.no_html {
        None
    } else {
        opts.html.as_deref()
    };
    let csv_eff = if default_report && opts.csv.is_none() && !opts.no_csv {
        Some("")
    } else if opts.no_csv {
        None
    } else {
        opts.csv.as_deref()
    };

    if want_report {
        println!();
        let _ = run_report(
            &[out.to_string()],
            if pretty { "pretty" } else { "text" },
            5.0,
            opts.prices.as_deref(),
            None,
            html_eff,
            csv_eff,
            opts.open,
            false,
            opts.full_prompts,
            opts.conv.as_deref(),
        );
        return;
    }
    if html_eff.is_some() || csv_eff.is_some() {
        let recs = read_jsonl(out, None);
        let summary = if recs.is_empty() {
            None
        } else {
            Some(summarize(&recs, 5.0))
        };
        if let Some(ha) = html_eff {
            write_html_dashboard(
                out,
                ha,
                5.0,
                opts.prices.as_deref(),
                opts.open,
                Some(&recs),
                summary.as_ref(),
            );
        }
        if let Some(ca) = csv_eff {
            write_csv_report(out, ca, 5.0, Some(&recs), summary.as_ref());
        }
        return;
    }
    eprintln!("tokprof: view the breakdown with: tokprof report   (or: tokprof report --html --csv)");
}

pub fn run_cli(argv: &[String]) -> i32 {
    let opts = match parse_args(argv) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("tokprof: {e}");
            return 2;
        }
    };

    if opts.version {
        println!("tokprof {}", env!("CARGO_PKG_VERSION"));
        return 0;
    }

    if opts.cmd.is_none() || opts.help {
        println!(
            "{}",
            render_detailed_help(opts.lang.as_deref(), opts.cmd.as_deref())
        );
        return 0;
    }

    apply_detect_env(&opts);
    let subcmd = opts.cmd.as_deref().unwrap_or("");

    match subcmd {
        "profile" => {
            let out = opts.out.clone().unwrap_or_else(default_out);
            let comps = match load_components(&opts.components) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("tokprof: {e}");
                    return 2;
                }
            };
            let code = if !opts.pid.is_empty() || !opts.comm.is_empty() || opts.all {
                if cfg!(target_os = "linux") {
                    run_ebpf(
                        &out,
                        &opts.pid,
                        &opts.comm,
                        opts.all,
                        opts.helper.as_deref(),
                        opts.duration.as_deref(),
                        comps,
                        !opts.no_tiktoken,
                        !opts.no_sudo,
                        !opts.no_plain,
                        false,
                    )
                } else {
                    let local_spec = [&opts.pid, &opts.comm]
                        .into_iter()
                        .filter(|s| !s.is_empty())
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(",");
                    let dur = session::parse_session_duration_secs(opts.duration.as_deref())
                        .map(|s| s as f64);
                    run_mitm(
                        &out,
                        &local_spec,
                        "",
                        opts.mitmdump.as_deref(),
                        comps,
                        !opts.no_tiktoken,
                        &[],
                        dur,
                        false,
                    )
                }
            } else {
                if opts.command_tail.is_empty() {
                    eprintln!(
                        "tokprof: profile needs a command (for example: tokprof profile -- python agent.py) or --pid/--comm"
                    );
                    return 2;
                }
                run_profiled(
                    &opts.command_tail,
                    &out,
                    comps,
                    &opts.maps,
                    !opts.no_tiktoken,
                    false,
                    !opts.no_gateway_env,
                    !opts.no_hooks,
                )
            };
            finish_run(&out, &opts, true, true);
            code
        }
        "run" | "shell" => {
            let cmd = if subcmd == "run" {
                if opts.command_tail.is_empty() {
                    eprintln!(
                        "tokprof: run needs a command after --, for example: tokprof run -- python agent.py"
                    );
                    return 2;
                }
                opts.command_tail.clone()
            } else if opts.command_tail.is_empty() {
                default_shell()
            } else {
                opts.command_tail.clone()
            };
            let out = opts.out.clone().unwrap_or_else(default_out);
            let comps = match load_components(&opts.components) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("tokprof: {e}");
                    return 2;
                }
            };
            let code = run_profiled(
                &cmd,
                &out,
                comps,
                &opts.maps,
                !opts.no_tiktoken,
                false,
                !opts.no_gateway_env,
                !opts.no_hooks,
            );
            finish_run(&out, &opts, false, false);
            code
        }
        "ebpf" => {
            let out = opts.out.clone().unwrap_or_else(default_out);
            let comps = match load_components(&opts.components) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("tokprof: {e}");
                    return 2;
                }
            };
            let code = run_ebpf(
                &out,
                &opts.pid,
                &opts.comm,
                opts.all,
                opts.helper.as_deref(),
                opts.duration.as_deref(),
                comps,
                !opts.no_tiktoken,
                !opts.no_sudo,
                !opts.no_plain,
                false,
            );
            finish_run(&out, &opts, false, false);
            code
        }
        "mitm" => {
            if opts.trust_help {
                println!("{TRUST_HELP}");
                return 0;
            }
            let out = opts.out.clone().unwrap_or_else(default_out);
            let comps = match load_components(&opts.components) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("tokprof: {e}");
                    return 2;
                }
            };
            let dur = session::parse_session_duration_secs(opts.duration.as_deref())
                .map(|s| s as f64);
            let code = run_mitm(
                &out,
                &opts.local,
                &opts.mode,
                opts.mitmdump.as_deref(),
                comps,
                !opts.no_tiktoken,
                &opts.command_tail,
                dur,
                false,
            );
            finish_run(&out, &opts, false, false);
            code
        }
        "report" => run_report(
            &opts.positionals,
            &opts.format,
            opts.gap,
            opts.prices.as_deref(),
            opts.out.as_deref(),
            if opts.no_html {
                None
            } else {
                opts.html.as_deref()
            },
            if opts.no_csv {
                None
            } else {
                opts.csv.as_deref()
            },
            opts.open,
            opts.watch,
            opts.full_prompts,
            opts.conv.as_deref(),
        ),
        "compare" => {
            if opts.positionals.len() < 2 {
                eprintln!("tokprof: compare requires <before.jsonl> <after.jsonl>");
                return 2;
            }
            let before = read_jsonl(&opts.positionals[0], None);
            let after = read_jsonl(&opts.positionals[1], None);
            let res = compare(&before, &after, opts.gap);
            if opts.json_flag || opts.format == "json" {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&res).unwrap_or_default()
                );
            } else {
                println!("{}", render_compare(&res));
            }
            if let Some(max_reg) = opts.max_regression {
                let rel_in = res
                    .get("metrics")
                    .and_then(|m| m.get("input_per_request"))
                    .and_then(|i| i.get("relative_change_pct"))
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0);
                let err_delta = res
                    .get("quality_guardrail")
                    .and_then(|q| q.get("error_rate_delta"))
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0);
                if rel_in > max_reg || err_delta > 0.05 {
                    eprintln!(
                        "tokprof: regression gate failed (input change {rel_in:+.1}% > allowed +{max_reg:.1}% or error rate increased)"
                    );
                    return 1;
                }
            }
            0
        }
        "export" => {
            let Some(file) = opts.positionals.first() else {
                eprintln!("tokprof: export requires <file.jsonl>");
                return 2;
            };
            let recs = read_jsonl(file, None);
            let rows = to_legacy(&recs, opts.gap);
            let mut text = rows
                .iter()
                .filter_map(|r| serde_json::to_string(r).ok())
                .collect::<Vec<_>>()
                .join("\n");
            if !text.is_empty() {
                text.push('\n');
            }
            if let Some(ref of) = opts.out {
                let _ = atomic_write_0600(of, &text);
            } else {
                print!("{text}");
            }
            0
        }
        "clean" => {
            let dir = opts
                .positionals
                .first()
                .map(|s| s.as_str())
                .or(opts.out.as_deref())
                .unwrap_or(".tokprof");
            let keep = opts.keep.unwrap_or(5);
            let active_out = session::read_state(&session::state_path(opts.state.as_deref()))
                .filter(|st| session::pid_alive(st.pid))
                .map(|st| st.out);
            let mut entries: Vec<(SystemTime, PathBuf)> = fs::read_dir(dir)
                .ok()
                .into_iter()
                .flatten()
                .filter_map(Result::ok)
                .map(|e| e.path())
                .filter(|p| {
                    let nm = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
                    p.is_file()
                        && (nm.ends_with(".jsonl")
                            || nm.ends_with(".jsonl.gz")
                            || (nm.ends_with(".html") && nm != "latest.html")
                            || (nm.ends_with(".csv") && nm != "latest.csv"))
                })
                .filter(|p| {
                    let abs = fs::canonicalize(p)
                        .unwrap_or_else(|_| p.clone())
                        .to_string_lossy()
                        .into_owned();
                    active_out.as_deref() != Some(abs.as_str())
                })
                .map(|p| {
                    let mt = fs::metadata(&p)
                        .and_then(|m| m.modified())
                        .unwrap_or(UNIX_EPOCH);
                    (mt, p)
                })
                .collect();
            entries.sort_by_key(|b| std::cmp::Reverse(b.0));
            let mut removed = 0usize;
            let mut freed = 0u64;
            for (_, p) in entries.into_iter().skip(keep) {
                let sz = fs::metadata(&p).map(|m| m.len()).unwrap_or(0);
                if fs::remove_file(&p).is_ok() {
                    removed += 1;
                    freed += sz;
                }
            }
            println!(
                "tokprof: cleaned {removed} artifact(s) in {dir} ({} bytes freed, kept latest {keep})",
                crate::segment::format_with_commas(freed as usize)
            );
            0
        }
        "serve-report" => {
            let dir = opts.out.as_deref().unwrap_or(".tokprof");
            let port = opts
                .state
                .as_deref()
                .and_then(|s| s.parse::<u16>().ok())
                .unwrap_or(18765);
            serve_report_http(dir, port)
        }
        "start" | "serve" | "status" | "env" | "stop" => {
            let st_file = session::state_path(opts.state.as_deref());
            if subcmd == "start" || subcmd == "serve" {
                let out_raw = opts.out.clone().unwrap_or_else(default_out);
                let out_abs = std::env::current_dir()
                    .map(|cwd| cwd.join(&out_raw))
                    .unwrap_or_else(|_| PathBuf::from(&out_raw))
                    .to_string_lossy()
                    .into_owned();
                let use_ebpf = if opts.ebpf {
                    true
                } else if opts.no_ebpf {
                    false
                } else {
                    opts.state.is_none() && cfg!(target_os = "linux")
                };
                if subcmd == "start" {
                    session::start(
                        &out_abs,
                        &st_file,
                        &opts.maps,
                        !opts.no_tiktoken,
                        Some(use_ebpf),
                        &opts.pid,
                        &opts.comm,
                    )
                } else {
                    session::serve(
                        &out_abs,
                        &st_file,
                        &opts.maps,
                        !opts.no_tiktoken,
                        use_ebpf,
                        &opts.pid,
                        &opts.comm,
                    )
                }
            } else if subcmd == "status" {
                session::status_with_format(&st_file, opts.json_flag || opts.format == "json")
            } else if subcmd == "env" {
                session::env_cmd(&st_file, opts.shell_kind.as_deref(), opts.unset)
            } else {
                let prev = session::read_state(&st_file);
                let rc = session::stop(&st_file, opts.force);
                if rc == 0 && !opts.no_report {
                    if let Some(st) = prev {
                        if !st.out.is_empty() {
                            let auto_rep = st.ebpf.is_some() || opts.state.is_none();
                            finish_run(&st.out, &opts, auto_rep, true);
                        }
                    }
                }
                rc
            }
        }
        "pull" => {
            let cp_file = checkpoint_path(None);
            let prev_cp = if opts.resume {
                load_pull_checkpoint(&cp_file)
            } else {
                None
            };
            let eff_workspace = opts
                .workspace
                .clone()
                .or_else(|| prev_cp.as_ref().and_then(|c| c.workspace.clone()));
            let eff_app_insights = opts
                .app_insights
                .clone()
                .or_else(|| prev_cp.as_ref().and_then(|c| c.app_insights.clone()));
            let eff_resource_id = opts
                .resource_id
                .clone()
                .or_else(|| prev_cp.as_ref().and_then(|c| c.resource_id.clone()));
            let eff_cloud = opts
                .cloud
                .clone()
                .or_else(|| prev_cp.as_ref().and_then(|c| c.cloud.clone()));

            let since = opts.since.as_deref().unwrap_or("24h");
            let comm_opt = if opts.comm.is_empty() {
                None
            } else {
                Some(opts.comm.as_str())
            };
            let resume_watermark_ts = prev_cp.as_ref().and_then(|c| {
                if c.last_ts > 0.0 {
                    if c.usage_only_calls > 0 {
                        Some((c.last_ts - 300.0).max(0.0))
                    } else {
                        Some(c.last_ts)
                    }
                } else {
                    None
                }
            });
            let dep_only_kql = build_dependencies_only_kql_ext(
                since,
                opts.conv.as_deref(),
                comm_opt,
                resume_watermark_ts,
                opts.limit,
            );
            let kql = opts.kql.clone().unwrap_or_else(|| {
                if opts.usage_only {
                    dep_only_kql.clone()
                } else {
                    build_app_insights_kql_ext(
                        since,
                        opts.conv.as_deref(),
                        comm_opt,
                        resume_watermark_ts,
                        opts.limit,
                    )
                }
            });
            if opts.print_kql {
                println!("{kql}");
                return 0;
            }
            if eff_workspace.is_none() && eff_app_insights.is_none() && eff_resource_id.is_none() {
                eprintln!(
                    "tokprof: pull requires --workspace <WORKSPACE_ID>, --app-insights <APP_ID>, or --resource-id <ARM_RESOURCE_ID> (or --resume / --print-kql)"
                );
                return 2;
            }
            let (la_res, ai_res) = resolve_azure_cloud_endpoints(eff_cloud.as_deref());
            let resource = if eff_workspace.is_some() || eff_resource_id.is_some() {
                la_res
            } else {
                ai_res
            };
            let bearer = match obtain_azure_bearer_token(opts.token.as_deref(), resource) {
                Ok(t) => t,
                Err(e) => {
                    eprintln!("tokprof: {e}");
                    return 2;
                }
            };
            let comps = match load_components(&opts.components) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("tokprof: {e}");
                    return 2;
                }
            };
            let ingest_opts = IngestOptions {
                salt: b"tokprof-az".to_vec(),
                components: comps,
                use_tiktoken: !opts.no_tiktoken,
                include_text: if opts.no_content { Some(false) } else { None },
                usage_only: opts.usage_only,
                run_id: Some("az-pull".to_string()),
                conv_filter: opts.conv.clone(),
                comm_filter: if opts.comm.is_empty() {
                    None
                } else {
                    Some(opts.comm.clone())
                },
            };

            let out = opts
                .out
                .clone()
                .or_else(|| prev_cp.as_ref().map(|c| c.out_path.clone()))
                .unwrap_or_else(default_out);

            let mut accumulated_records = if opts.resume && Path::new(&out).is_file() {
                read_jsonl(&out, None)
            } else {
                Vec::new()
            };

            // Cap wait duration at 25m (1500s) in Cloud Shell to stay safely under the 30m idle timeout
            let max_wait_s = if let Some(ref w) = opts.wait_ingest {
                let raw_s = parse_duration_secs_f64(w, 0.0);
                if is_cloud_shell() {
                    raw_s.min(1500.0)
                } else {
                    raw_s
                }
            } else {
                0.0
            };
            let poll_s = opts
                .poll_interval
                .as_deref()
                .map(|p| parse_duration_secs_f64(p, 5.0))
                .unwrap_or(5.0);
            let min_calls = opts.min_calls.unwrap_or(1);

            let start_inst = Instant::now();
            let mut total_upgraded = 0usize;
            let mut last_stats;
            let mut poll_idx = 0usize;

            loop {
                poll_idx += 1;
                let (raw_resp, used_rbac_fallback) = match query_azure_logs_with_resilience(
                    opts.endpoint.as_deref(),
                    eff_workspace.as_deref(),
                    eff_app_insights.as_deref(),
                    eff_resource_id.as_deref(),
                    eff_cloud.as_deref(),
                    &bearer,
                    &kql,
                    if opts.kql.is_none() {
                        Some(dep_only_kql.as_str())
                    } else {
                        None
                    },
                ) {
                    Ok(r) => r,
                    Err(e) => {
                        eprintln!("tokprof: {e}");
                        return 1;
                    }
                };
                let (batch_recs, mut st) = match parse_app_insights_payload(&raw_resp, &ingest_opts)
                {
                    Ok(res) => res,
                    Err(e) => {
                        eprintln!("tokprof: failed to parse Application Insights response: {e}");
                        return 1;
                    }
                };
                st.table_rbac_fallback = used_rbac_fallback;
                let (merged, upg) = merge_and_upgrade_records(&accumulated_records, batch_recs);
                accumulated_records = merged;
                total_upgraded += upg;
                st.upgraded_from_usage_only = total_upgraded;
                last_stats = st;

                // Persist records and checkpoint after every poll so Cloud Shell 30m timeout never loses data
                if let Err(e) = write_records_atomic(&out, &accumulated_records) {
                    eprintln!("tokprof: cannot write {out}: {e}");
                    return 2;
                }
                let usage_only_rem = accumulated_records
                    .iter()
                    .filter(|r| r.quality.as_deref() == Some("usage_only"))
                    .count();
                let cp = PullCheckpoint {
                    workspace: eff_workspace.clone(),
                    app_insights: eff_app_insights.clone(),
                    resource_id: eff_resource_id.clone(),
                    cloud: eff_cloud.clone(),
                    out_path: out.clone(),
                    last_ts: accumulated_records.last().map(|r| r.ts).unwrap_or(0.0),
                    total_calls: accumulated_records.len(),
                    usage_only_calls: usage_only_rem,
                    updated_epoch_s: SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .map(|d| d.as_secs_f64())
                        .unwrap_or(0.0),
                };
                save_pull_checkpoint(&cp, &cp_file);

                let have_enough = accumulated_records.len() >= min_calls;
                let all_settled = usage_only_rem == 0 || last_stats.table_rbac_fallback;
                if max_wait_s <= 0.0
                    || (have_enough && all_settled)
                    || start_inst.elapsed().as_secs_f64() >= max_wait_s
                {
                    break;
                }
                eprintln!(
                    "tokprof: [wait-ingest poll #{poll_idx}] {} call(s) ({} waiting for AppTraces ingestion); polling again in {:.1}s (checkpoint saved to {})...",
                    accumulated_records.len(),
                    usage_only_rem,
                    poll_s,
                    cp_file
                );
                thread::sleep(Duration::from_secs_f64(poll_s));
            }

            if last_stats.table_rbac_fallback {
                eprintln!(
                    "tokprof: NOTE: AppTraces table access was denied by RBAC; automatically fell back to AppDependencies-only query (usage-only mode)"
                );
            }
            if let Some(ref pw) = last_stats.partial_error_warning {
                eprintln!("tokprof: WARNING: Azure returned PartialError: {pw}");
            }
            eprintln!(
                "tokprof: pulled {} LLM call(s) from Azure logs ({} full-segment, {} usage-only, {} upgraded-late-traces, {} deduplicated, {} lossless-chunk, {} truncated-repaired, max lag {}ms) -> {}",
                accumulated_records.len(),
                last_stats.tier1_full_calls,
                accumulated_records
                    .iter()
                    .filter(|r| r.quality.as_deref() == Some("usage_only"))
                    .count(),
                last_stats.upgraded_from_usage_only,
                last_stats.deduplicated_spans,
                last_stats.lossless_chunk_calls,
                last_stats.truncated_repaired_calls,
                last_stats.max_ingestion_lag_ms,
                out
            );
            let ci = inspect_records_content(&accumulated_records);
            eprintln!(
                "tokprof: content verification — mode={}, segments_with_text={}/{}, stored_prompt_chars={}, stored_response_chars={}, 32k_preview_capped={}, appinsights_32kb_repaired={}",
                ci.mode,
                ci.segments_with_text,
                ci.total_segments,
                ci.stored_prompt_chars,
                ci.stored_response_chars,
                ci.preview_32k_capped_segments,
                ci.appinsights_32kb_repaired_calls
            );
            finish_run(&out, &opts, true, true);
            0
        }
        "import" => {
            let raw_input = if !opts.positionals.is_empty() {
                let mut chunks = Vec::with_capacity(opts.positionals.len());
                for file in &opts.positionals {
                    match read_text_or_gz(file) {
                        Ok(s) => chunks.push(s),
                        Err(e) => {
                            eprintln!("tokprof: cannot read {file}: {e}");
                            return 2;
                        }
                    }
                }
                chunks.join("\n")
            } else if !std::io::stdin().is_terminal() {
                let mut s = String::new();
                if std::io::stdin().read_to_string(&mut s).is_err() {
                    eprintln!("tokprof: cannot read stdin");
                    return 2;
                }
                s
            } else {
                eprintln!("tokprof: import requires <file> (JSON, NDJSON, .gz, or `-` for stdin)");
                return 2;
            };
            let comps = match load_components(&opts.components) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("tokprof: {e}");
                    return 2;
                }
            };
            let import_salt = if let Ok(s) = std::env::var("TOKPROF_SALT") {
                s.into_bytes()
            } else {
                use rand::RngCore as _;
                let mut b = vec![0u8; 16];
                rand::thread_rng().fill_bytes(&mut b);
                b
            };
            let ingest_opts = IngestOptions {
                salt: import_salt,
                components: comps,
                use_tiktoken: !opts.no_tiktoken,
                include_text: if opts.no_content { Some(false) } else { None },
                usage_only: opts.usage_only,
                run_id: Some("az-import".to_string()),
                conv_filter: opts.conv.clone(),
                comm_filter: if opts.comm.is_empty() {
                    None
                } else {
                    Some(opts.comm.clone())
                },
            };
            let (records, st) = match parse_app_insights_payload(&raw_input, &ingest_opts) {
                Ok(res) => res,
                Err(e) => {
                    eprintln!("tokprof: failed to parse input logs: {e}");
                    return 1;
                }
            };
            let out = opts.out.clone().unwrap_or_else(default_out);
            if let Err(e) = write_records_atomic(&out, &records) {
                eprintln!("tokprof: cannot open {out}: {e}");
                return 2;
            }
            eprintln!(
                "tokprof: imported {} LLM call(s) ({} full-segment, {} usage-only, {} lossless-chunk, {} truncated-repaired) -> {}",
                records.len(),
                st.tier1_full_calls,
                st.tier2_usage_only_calls,
                st.lossless_chunk_calls,
                st.truncated_repaired_calls,
                out
            );
            let ci = inspect_records_content(&records);
            eprintln!(
                "tokprof: content verification — mode={}, segments_with_text={}/{}, stored_prompt_chars={}, stored_response_chars={}, 32k_preview_capped={}, appinsights_32kb_repaired={}",
                ci.mode,
                ci.segments_with_text,
                ci.total_segments,
                ci.stored_prompt_chars,
                ci.stored_response_chars,
                ci.preview_32k_capped_segments,
                ci.appinsights_32kb_repaired_calls
            );
            finish_run(&out, &opts, opts.report, true);
            0
        }
        "explain" => {
            let Some(url) = opts.positionals.first() else {
                eprintln!("tokprof: explain requires <url>");
                return 2;
            };
            let raw_body = if let Some(ref bf) = opts.body_file {
                fs::read_to_string(bf).ok()
            } else if !std::io::stdin().is_terminal() {
                let mut s = String::new();
                std::io::stdin().read_to_string(&mut s).ok().map(|_| s)
            } else {
                None
            };
            let body_val = match raw_body {
                Some(ref s) if !s.trim().is_empty() => match serde_json::from_str::<Value>(s) {
                    Ok(v) => Some(v),
                    Err(_) => {
                        eprintln!("tokprof: the request body is not valid JSON");
                        return 2;
                    }
                },
                _ => None,
            };
            let resp_str = opts
                .response_file
                .as_ref()
                .and_then(|rf| fs::read_to_string(rf).ok());
            println!(
                "{}",
                explain(
                    url,
                    body_val.as_ref(),
                    resp_str.as_deref(),
                    opts.sse,
                    None
                )
            );
            0
        }
        "doctor" => crate::doctor::run_doctor_with_format(opts.json_flag || opts.format == "json"),
        "selftest" => run_selftest(),
        _ => {
            println!(
                "{}",
                render_detailed_help(opts.lang.as_deref(), None)
            );
            0
        }
    }
}
