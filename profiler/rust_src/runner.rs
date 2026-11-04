use crate::detect::parse_url_parts;
use crate::gateway::{Gateway, GatewayServer, Route};
use crate::store::{atomic_write_0600, JsonlWriter};
use rand::RngCore;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

pub const DEFAULT_OPENAI: &str = "https://api.openai.com/v1";
pub const DEFAULT_ANTHROPIC: &str = "https://api.anthropic.com";

const EMBED_HOOK_PY: &str = include_str!("../src/tokprof/boot/_tokprof_hook.py");
const EMBED_SITECUSTOMIZE_PY: &str = include_str!("../src/tokprof/boot/sitecustomize.py");
const EMBED_NODE_PRELOAD_JS: &str = include_str!("../src/tokprof/boot/node_preload.js");
const EMBED_MITM_ADDON_PY: &str = include_str!("../src/tokprof/boot/mitm_addon.py");

const HANDLED_VARS: &[&str] = &[
    "OPENAI_BASE_URL",
    "ANTHROPIC_BASE_URL",
    "AZURE_OPENAI_ENDPOINT",
];

const LLM_NAME_TOKENS: &[&str] = &[
    "OPENAI",
    "ANTHROPIC",
    "CLAUDE",
    "LLM",
    "VLLM",
    "OLLAMA",
    "LITELLM",
    "GEMINI",
    "MISTRAL",
    "COHERE",
    "GROQ",
    "DEEPSEEK",
    "TOGETHER",
    "OPENROUTER",
    "INFERENCE",
    "FOUNDRY",
    "AOAI",
    "LMSTUDIO",
    "SGLANG",
    "TGI",
];

const URL_SUFFIXES: &[&str] = &[
    "_BASE_URL",
    "_API_BASE",
    "_API_BASE_URL",
    "_API_URL",
    "_ENDPOINT",
    "_API_ENDPOINT",
    "_URL",
    "_PROXY_URL",
];

const NOT_ENDPOINTS: &[&str] = &[
    "DOWNLOAD",
    "WEIGHT",
    "CHECKPOINT",
    "WEBHOOK",
    "DATABASE",
    "STORAGE",
    "BLOB",
    "REDIS",
    "PROXY_URL_",
    "TELEMETRY",
    "DOCS",
    "DOCUMENTATION",
    "HOMEPAGE",
    "REPO",
    "GITHUB",
    "SENTRY",
    "OTEL",
    "TRACING",
];

pub fn materialize_boot_dir() -> PathBuf {
    let base = std::env::var("TOKPROF_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME")
                .or_else(|_| std::env::var("USERPROFILE"))
                .unwrap_or_else(|_| "/tmp".to_string());
            PathBuf::from(home).join(".tokprof")
        })
        .join("boot");
    let _ = fs::create_dir_all(&base);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = fs::metadata(&base) {
            let mut perms = meta.permissions();
            perms.set_mode(0o700);
            let _ = fs::set_permissions(&base, perms);
        }
    }
    for (name, content) in [
        ("_tokprof_hook.py", EMBED_HOOK_PY),
        ("sitecustomize.py", EMBED_SITECUSTOMIZE_PY),
        ("node_preload.js", EMBED_NODE_PRELOAD_JS),
        ("mitm_addon.py", EMBED_MITM_ADDON_PY),
    ] {
        let target = base.join(name);
        let need_write = fs::read_to_string(&target)
            .map(|existing| existing != content)
            .unwrap_or(true);
        if need_write {
            let _ = atomic_write_0600(&target.to_string_lossy(), content);
        }
    }
    base
}

pub fn discover_env_endpoints(env_map: &BTreeMap<String, String>) -> Vec<(String, String)> {
    let mut found = Vec::new();
    for (name, raw_val) in env_map {
        let up = name.to_ascii_uppercase();
        if HANDLED_VARS.contains(&name.as_str())
            || !URL_SUFFIXES.iter().any(|s| up.ends_with(s))
        {
            continue;
        }
        if !LLM_NAME_TOKENS.iter().any(|tok| up.contains(tok))
            || NOT_ENDPOINTS.iter().any(|bad| up.contains(bad))
        {
            continue;
        }
        let val = raw_val.trim();
        if !val.starts_with("http://") && !val.starts_with("https://") {
            continue;
        }
        if val.split("://").nth(1).unwrap_or("").contains('@') {
            continue;
        }
        let (_, host, _, _) = parse_url_parts(val);
        if host.is_empty() {
            continue;
        }
        found.push((name.clone(), val.to_string()));
    }
    if let Some(host_raw) = env_map.get("OLLAMA_HOST") {
        let host = host_raw.trim();
        if !host.is_empty() && !found.iter().any(|(k, _)| k == "OLLAMA_HOST") {
            let val = if host.starts_with("http://") || host.starts_with("https://") {
                host.to_string()
            } else {
                format!("http://{host}")
            };
            let (_, h, _, _) = parse_url_parts(&val);
            if !h.is_empty() && h != "0.0.0.0" {
                found.push(("OLLAMA_HOST".to_string(), val));
            }
        }
    }
    found
}

pub type RouteConfig = (Vec<Route>, BTreeMap<String, String>, HashSet<String>);

pub fn build_routes(
    env_map: &BTreeMap<String, String>,
    extra_maps: &[String],
    auto_opt: Option<bool>,
) -> Result<RouteConfig, String> {
    let mut routes = Vec::new();
    let mut overrides = BTreeMap::new();

    let oa = env_map
        .get("OPENAI_BASE_URL")
        .filter(|s| !s.is_empty())
        .map(|s| s.as_str())
        .unwrap_or(DEFAULT_OPENAI);
    let (_, _, _, oa_p) = parse_url_parts(oa);
    let oa_path = oa_p.trim_end_matches('/');
    routes.push(Route::from_urls(&format!("/openai{oa_path}"), oa));
    overrides.insert(
        "OPENAI_BASE_URL".to_string(),
        format!("http://127.0.0.1:{{port}}/openai{oa_path}"),
    );

    let an = env_map
        .get("ANTHROPIC_BASE_URL")
        .filter(|s| !s.is_empty())
        .map(|s| s.as_str())
        .unwrap_or(DEFAULT_ANTHROPIC);
    let (_, _, _, an_p) = parse_url_parts(an);
    let an_path = an_p.trim_end_matches('/');
    routes.push(Route::from_urls(&format!("/anthropic{an_path}"), an));
    overrides.insert(
        "ANTHROPIC_BASE_URL".to_string(),
        format!("http://127.0.0.1:{{port}}/anthropic{an_path}"),
    );

    let mut azure_hosts = HashSet::new();
    if let Some(az) = env_map.get("AZURE_OPENAI_ENDPOINT").filter(|s| !s.is_empty()) {
        let (_, host, _, _) = parse_url_parts(az);
        if !host.is_empty() {
            azure_hosts.insert(host.clone());
            overrides.insert(
                "AZURE_OPENAI_ENDPOINT".to_string(),
                format!("http://127.0.0.1:{{port}}/azure/{host}"),
            );
        }
    }

    let auto = auto_opt.unwrap_or_else(|| {
        env_map.get("TOKPROF_NO_AUTO_ENV").map(|s| s.as_str()) != Some("1")
    });
    let mapped_vars: HashSet<String> = extra_maps
        .iter()
        .filter_map(|s| s.split_once('=').map(|(k, _)| k.to_string()))
        .collect();

    if auto {
        for (j, (var, upstream)) in discover_env_endpoints(env_map).into_iter().enumerate() {
            if mapped_vars.contains(&var) {
                continue;
            }
            let (_, _, _, p) = parse_url_parts(&upstream);
            let path = p.trim_end_matches('/');
            let prefix = format!("/auto{j}{path}");
            routes.push(Route::from_urls(&prefix, &upstream));
            overrides.insert(var, format!("http://127.0.0.1:{{port}}{prefix}"));
        }
    }

    for (i, spec) in extra_maps.iter().enumerate() {
        let Some((var, upstream)) = spec.split_once('=') else {
            return Err(format!("--map expects ENV_VAR=UPSTREAM_URL, got {spec:?}"));
        };
        if var.is_empty() || upstream.is_empty() {
            return Err(format!("--map expects ENV_VAR=UPSTREAM_URL, got {spec:?}"));
        }
        let (_, _, _, p) = parse_url_parts(upstream);
        let path = p.trim_end_matches('/');
        let prefix = format!("/map{i}{path}");
        routes.push(Route::from_urls(&prefix, upstream));
        overrides.insert(var.to_string(), format!("http://127.0.0.1:{{port}}{prefix}"));
    }

    Ok((routes, overrides, azure_hosts))
}

pub fn hook_env(
    base_env: &BTreeMap<String, String>,
    port: u16,
    token: &str,
) -> BTreeMap<String, String> {
    let boot_dir = materialize_boot_dir();
    let boot_str = boot_dir.to_string_lossy().into_owned();
    let sep = if cfg!(windows) { ";" } else { ":" };
    let existing_py = base_env.get("PYTHONPATH").cloned().unwrap_or_default();
    let py_path = if existing_py.is_empty() {
        boot_str.clone()
    } else {
        format!("{boot_str}{sep}{existing_py}")
    };

    let preload = boot_dir
        .join("node_preload.js")
        .to_string_lossy()
        .replace('\\', "/");
    let flag = format!("--require \"{preload}\"");
    let existing_node = base_env.get("NODE_OPTIONS").cloned().unwrap_or_default();
    let node_opts = if existing_node.contains(&flag) {
        existing_node
    } else if existing_node.is_empty() {
        flag
    } else {
        format!("{existing_node} {flag}")
    };

    let mut out = BTreeMap::new();
    out.insert("TOKPROF_PORT".to_string(), port.to_string());
    out.insert("TOKPROF_TOKEN".to_string(), token.to_string());
    out.insert("PYTHONPATH".to_string(), py_path);
    out.insert("NODE_OPTIONS".to_string(), node_opts);
    out
}

pub fn current_env_map() -> BTreeMap<String, String> {
    std::env::vars().collect()
}

#[allow(clippy::too_many_arguments)]
pub fn run_profiled(
    cmd: &[String],
    out_path: &str,
    components: HashMap<String, String>,
    extra_maps: &[String],
    use_tiktoken: bool,
    quiet: bool,
    gateway_env: bool,
    hooks: bool,
) -> i32 {
    if cmd.is_empty() {
        eprintln!("tokprof: no command specified");
        return 2;
    }
    let env_map = current_env_map();
    let (routes, overrides, azure_hosts) = match build_routes(&env_map, extra_maps, None) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("tokprof: {e}");
            return 2;
        }
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
    let mut run_bytes = [0u8; 4];
    rand::thread_rng().fill_bytes(&mut run_bytes);
    let mut tok_bytes = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut tok_bytes);
    let run_id = hex::encode(run_bytes);
    let ingest_token = hex::encode(tok_bytes);

    let w_clone = writer.clone();
    let gw = Gateway::new(
        routes,
        Arc::new(move |rec| w_clone.write_record(&rec)),
        salt,
        components,
        Some(run_id),
        azure_hosts,
        use_tiktoken,
        Some(ingest_token.clone()),
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

    let mut child_env = env_map.clone();
    if gateway_env {
        for (k, v) in &overrides {
            child_env.insert(k.clone(), v.replace("{port}", &port.to_string()));
        }
    }
    if hooks {
        for (k, v) in hook_env(&env_map, port, &ingest_token) {
            child_env.insert(k, v);
        }
    }
    child_env.insert("TOKPROF_ACTIVE".to_string(), "1".to_string());

    if !quiet {
        let mut how = Vec::new();
        if gateway_env {
            let extra: Vec<&str> = overrides
                .keys()
                .filter(|k| !HANDLED_VARS.contains(&k.as_str()))
                .map(|s| s.as_str())
                .collect();
            if extra.is_empty() {
                how.push("LLM base URLs redirected".to_string());
            } else {
                how.push(format!(
                    "LLM base URLs redirected (including {})",
                    extra.join(", ")
                ));
            }
        }
        if hooks {
            how.push("in-process hooks for Python/Node".to_string());
        }
        eprintln!(
            "tokprof: profiling to {out_path} (gateway 127.0.0.1:{port}; {})",
            if how.is_empty() {
                "no capture enabled".to_string()
            } else {
                how.join(", ")
            }
        );
    }

    crate::session::install_signal_handlers();
    let mut command = Command::new(&cmd[0]);
    if cmd.len() > 1 {
        command.args(&cmd[1..]);
    }
    command.envs(&child_env);
    let code = match command.status() {
        Ok(st) => st.code().unwrap_or(1),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            eprintln!("tokprof: command not found: {}", cmd[0]);
            127
        }
        Err(e) => {
            eprintln!("tokprof: failed to run {}: {e}", cmd[0]);
            1
        }
    };

    let deadline = std::time::Instant::now() + Duration::from_millis(150);
    while gw.stats.active.load(Ordering::SeqCst) > 0 && std::time::Instant::now() < deadline {
        thread::sleep(Duration::from_millis(5));
    }
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
    code
}

pub fn default_shell() -> Vec<String> {
    if cfg!(windows) {
        vec![std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".to_string())]
    } else {
        vec![std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string())]
    }
}
