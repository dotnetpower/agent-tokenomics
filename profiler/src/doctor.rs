use crate::appinsights::is_cloud_shell;
use crate::counting::get_counter;
use crate::ebpf::{find_helper, is_root, which_exe};
use crate::mitm::find_mitmdump;
use std::net::TcpListener;
use std::path::Path;

fn port_free() -> bool {
    TcpListener::bind(("127.0.0.1", 0)).is_ok()
}

fn has_linux_bpf_caps() -> bool {
    let Ok(status) = std::fs::read_to_string("/proc/self/status") else {
        return false;
    };
    for line in status.lines() {
        if let Some(hex_s) = line.strip_prefix("CapEff:") {
            if let Ok(mask) = u64::from_str_radix(hex_s.trim(), 16) {
                const CAP_SYS_ADMIN: u64 = 1 << 21;
                const CAP_PERFMON: u64 = 1 << 38;
                const CAP_BPF: u64 = 1 << 39;
                return (mask & CAP_SYS_ADMIN) != 0
                    || ((mask & CAP_BPF) != 0 && (mask & CAP_PERFMON) != 0);
            }
        }
    }
    false
}

fn ebpf_status() -> (bool, String) {
    if !cfg!(target_os = "linux") {
        return (false, "eBPF tap is Linux-only".to_string());
    }
    let mut ok = true;
    let mut notes = Vec::new();
    if !Path::new("/sys/kernel/btf/vmlinux").exists() {
        ok = false;
        notes.push("no kernel BTF (/sys/kernel/btf/vmlinux)".to_string());
    }
    if find_helper(None, false).is_none() {
        ok = false;
        notes.push(
            "tokprof-ebpf helper not found (build: cd profiler/ebpf && go build -o bin/tokprof-ebpf ./cmd/tokprof-ebpf)"
                .to_string(),
        );
    }
    if !is_root() && !has_linux_bpf_caps() {
        notes.push("not root: eBPF needs sudo or CAP_BPF/CAP_PERFMON".to_string());
    }
    let detail = if notes.is_empty() {
        "ready".to_string()
    } else {
        notes.join("; ")
    };
    (ok, detail)
}

pub fn run_doctor() -> i32 {
    run_doctor_with_format(false)
}

pub fn run_doctor_with_format(as_json: bool) -> i32 {
    let mut rows: Vec<(&str, bool, String)> = Vec::new();
    rows.push((
        "platform",
        true,
        format!("{} ({}) [native Rust]", std::env::consts::OS, std::env::consts::ARCH),
    ));
    rows.push((
        "local port available",
        port_free(),
        "gateway binds 127.0.0.1 on a free port".to_string(),
    ));
    let counter = get_counter(Some("gpt-4o"), true);
    rows.push(("token counter", true, counter.method));
    let node = which_exe("node");
    rows.push((
        "node (for Node-based agents)",
        node.is_some(),
        node.map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|| "not found; not required".to_string()),
    ));
    let redirected: Vec<&str> = [
        "OPENAI_BASE_URL",
        "ANTHROPIC_BASE_URL",
        "AZURE_OPENAI_ENDPOINT",
        "GOOGLE_GEMINI_BASE_URL",
        "GEMINI_BASE_URL",
        "OPENROUTER_BASE_URL",
        "OLLAMA_HOST",
    ]
    .into_iter()
    .filter(|v| std::env::var(v).map(|s| !s.is_empty()).unwrap_or(false))
    .collect();
    rows.push((
        "existing base-URL env vars",
        true,
        if redirected.is_empty() {
            "none".to_string()
        } else {
            format!("{} (kept as upstream)", redirected.join(", "))
        },
    ));
    let (ebpf_ok, ebpf_note) = ebpf_status();
    rows.push(("eBPF TLS tap (Linux)", ebpf_ok, ebpf_note));
    let mitm = find_mitmdump(None);
    rows.push((
        "mitmproxy (tokprof mitm; Windows/macOS local capture)",
        mitm.is_some(),
        mitm.as_ref()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|| {
                "not found; optional. Install outside your project: uv tool install mitmproxy"
                    .to_string()
            }),
    ));
    let cloud_shell = is_cloud_shell();
    let az_cli = which_exe("az");
    let has_native_az = std::env::var("TOKPROF_AZURE_TOKEN")
        .map(|s| !s.trim().is_empty())
        .unwrap_or(false)
        || std::env::var("IDENTITY_ENDPOINT")
            .map(|s| !s.trim().is_empty())
            .unwrap_or(false)
        || std::env::var("AZURE_CLIENT_ID")
            .map(|s| !s.trim().is_empty())
            .unwrap_or(false);
    rows.push((
        "Azure CLI / Cloud Shell / Managed Identity (tokprof pull)",
        az_cli.is_some() || cloud_shell || has_native_az,
        format!(
            "az={}; native_env={}; cloud_shell={} (Web Preview ports 8080, 8000-8090)",
            az_cli
                .as_ref()
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_else(|| "not found".to_string()),
            if has_native_az { "yes" } else { "no" },
            if cloud_shell { "yes" } else { "no" }
        ),
    ));

    if as_json {
        let checks: Vec<serde_json::Value> = rows
            .iter()
            .map(|(name, ok, detail)| {
                serde_json::json!({
                    "check": name,
                    "ok": ok,
                    "detail": detail,
                })
            })
            .collect();
        let doc = serde_json::json!({
            "version": env!("CARGO_PKG_VERSION"),
            "checks": checks,
            "backends": {
                "gateway_hooks": true,
                "otlp_http": true,
                "ebpf": ebpf_ok,
                "mitm": mitm.is_some(),
                "azure_pull": az_cli.is_some() || has_native_az,
            }
        });
        println!("{}", serde_json::to_string_pretty(&doc).unwrap_or_default());
        return 0;
    }

    println!("tokprof {} doctor (Rust)", env!("CARGO_PKG_VERSION"));
    for (name, ok, detail) in &rows {
        println!("  [{}] {name}: {detail}", if *ok { "ok" } else { "--" });
    }
    println!();
    println!("Available capture backends:");
    println!(
        "  gateway, OTLP (/v1/traces, /v1/logs), and in-process hooks (tokprof run / shell): yes"
    );
    println!(
        "  eBPF tap for running processes (tokprof ebpf): {}",
        if ebpf_ok { "possible" } else { "not available" }
    );
    println!(
        "  mitmproxy local capture (tokprof mitm): {}",
        if mitm.is_some() {
            "possible"
        } else {
            "install mitmproxy first"
        }
    );
    println!(
        "  Azure Application Insights / Log Analytics pull (tokprof pull / import): {}",
        if az_cli.is_some() || has_native_az {
            "ready (via az CLI, Managed Identity/SP, or --token)"
        } else {
            "ready with --token or tokprof import"
        }
    );
    0
}
