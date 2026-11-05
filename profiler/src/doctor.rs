use crate::counting::get_counter;
use crate::ebpf::{find_helper, is_root, which_exe};
use crate::mitm::find_mitmdump;
use std::net::TcpListener;
use std::path::Path;

fn port_free() -> bool {
    TcpListener::bind(("127.0.0.1", 0)).is_ok()
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
    if !is_root() {
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
    let redirected: Vec<&str> = ["OPENAI_BASE_URL", "ANTHROPIC_BASE_URL", "AZURE_OPENAI_ENDPOINT"]
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

    println!("tokprof {} doctor (Rust)", env!("CARGO_PKG_VERSION"));
    for (name, ok, detail) in &rows {
        println!("  [{}] {name}: {detail}", if *ok { "ok" } else { "--" });
    }
    println!();
    println!("Available capture backends:");
    println!(
        "  gateway and in-process hooks (tokprof run / shell): yes, for processes started under tokprof"
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
    0
}
