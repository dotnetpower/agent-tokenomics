use crate::analyze::{compare, summarize, to_legacy};
use crate::detect::explain;
use crate::doctor::run_doctor;
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
use std::io::{IsTerminal, Read};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const SUBCOMMANDS: &[&str] = &[
    "profile", "run", "shell", "ebpf", "mitm", "explain", "report", "compare", "export", "start",
    "serve", "status", "env", "stop", "doctor", "selftest",
];

fn default_out() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!(".tokprof/run-{secs}.jsonl")
}

fn find_latest_jsonl(dir: &str) -> Option<String> {
    if let Ok(entries) = fs::read_dir(dir) {
        let mut cands: Vec<(SystemTime, PathBuf)> = entries
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| {
                p.is_file()
                    && p.extension()
                        .and_then(|s| s.to_str())
                        .map(|s| s == "jsonl")
                        .unwrap_or(false)
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
    let p = path_opt?;
    let raw = fs::read_to_string(p).ok()?;
    let val: Value = serde_json::from_str(&raw).ok()?;
    val.as_object().cloned()
}

fn open_in_browser(path: &str) {
    let abs = fs::canonicalize(path)
        .unwrap_or_else(|_| PathBuf::from(path))
        .to_string_lossy()
        .into_owned();
    let url = format!("file://{abs}");
    #[cfg(target_os = "macos")]
    let _ = Command::new("open").arg(&url).spawn();
    #[cfg(target_os = "linux")]
    let _ = Command::new("xdg-open").arg(&url).spawn();
    #[cfg(windows)]
    let _ = Command::new("cmd").args(["/C", "start", "", &url]).spawn();
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
            }
        }
    }
    eprintln!("tokprof: HTML dashboard written to {dest}");
    if open_browser {
        open_in_browser(&dest);
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

#[allow(clippy::too_many_arguments)]
fn run_report(
    path_opt: Option<&str>,
    fmt: &str,
    gap: f64,
    prices: Option<&str>,
    out_file: Option<&str>,
    html_arg: Option<&str>,
    csv_arg: Option<&str>,
    open_browser: bool,
    watch: bool,
    full_prompts: bool,
) -> i32 {
    let resolved_path = match path_opt.filter(|s| !s.is_empty()) {
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
        loop {
            print!("\x1b[2J\x1b[H");
            if Path::new(&resolved_path).exists() {
                let _ = run_report(
                    Some(&resolved_path),
                    "pretty",
                    gap,
                    prices,
                    None,
                    None,
                    None,
                    false,
                    false,
                    full_prompts,
                );
            } else {
                println!("Waiting for first LLM call...");
            }
            eprintln!("\nWatching {resolved_path} (Ctrl+C to stop)...");
            thread::sleep(Duration::from_secs(1));
        }
    }

    let mut stats = ReadStats::default();
    let records = read_jsonl(&resolved_path, Some(&mut stats));
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
            open_in_browser(of);
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
    insecure_upstream: bool,
    no_auto_env: bool,
    record_hosts: bool,
    no_content: bool,
    tokenizer: Option<String>,
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
            "--insecure-upstream" => opts.insecure_upstream = true,
            "--no-auto-env" => opts.no_auto_env = true,
            "--record-hosts" => opts.record_hosts = true,
            "--no-content" => opts.no_content = true,
            "--tokenizer" => {
                i += 1;
                opts.tokenizer = pre.get(i).cloned();
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
            Some(out),
            if pretty { "pretty" } else { "text" },
            5.0,
            opts.prices.as_deref(),
            None,
            html_eff,
            csv_eff,
            opts.open,
            false,
            opts.full_prompts,
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
                    let dur = opts
                        .duration
                        .as_deref()
                        .and_then(|s| s.trim_end_matches('s').parse::<f64>().ok());
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
            let dur = opts
                .duration
                .as_deref()
                .and_then(|s| s.trim_end_matches('s').parse::<f64>().ok());
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
            opts.positionals.first().map(|s| s.as_str()),
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
        ),
        "compare" => {
            if opts.positionals.len() < 2 {
                eprintln!("tokprof: compare requires <before.jsonl> <after.jsonl>");
                return 2;
            }
            let before = read_jsonl(&opts.positionals[0], None);
            let after = read_jsonl(&opts.positionals[1], None);
            let res = compare(&before, &after, opts.gap);
            if opts.json_flag {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&res).unwrap_or_default()
                );
            } else {
                println!("{}", render_compare(&res));
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
                session::status(&st_file)
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
        "doctor" => run_doctor(),
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
