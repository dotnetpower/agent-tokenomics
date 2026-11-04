use crate::parse_request::{ANTHROPIC, COMPLETIONS, GEMINI, OPENAI_CHAT, OPENAI_RESPONSES};
use crate::parse_response::extract_eventstream_objects_str;
use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::net::IpAddr;
use std::sync::Mutex;

pub const ACCEPT: i32 = 50;
pub const CANDIDATE: i32 = 25;

pub static EXCLUDED_PATH_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?i)/(embeddings?|moderations?|rerank|tokenize|detokenize|count_tokens|audio/(speech|transcriptions|translations)|images?/(generations|edits|variations)|files|batches|fine_tuning|uploads|vector_stores|assistants|threads|realtime)(/|$)|:(embedContent|batchEmbedContents|countTokens)$",
    )
    .unwrap()
});

static PLAUSIBLE_KEYS_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#""(?:messages|contents|chat_history)"\s*:\s*\[|"(?:prompt|input|inputs|preamble|message)"\s*:"#)
        .unwrap()
});

static BEDROCK_HOST_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^bedrock-runtime(-fips)?\.[a-z0-9-]+\.amazonaws\.com(\.cn)?$").unwrap()
});

static HOST_SPLIT_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"[.\-_0-9]+").unwrap());

static STREAM_PATH_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i):streamGenerateContent$|/converse-stream$|-with-response-stream$|/generate_stream$")
        .unwrap()
});

static PATH_HINTS: Lazy<Vec<(Regex, &'static str)>> = Lazy::new(|| {
    vec![
        (Regex::new(r"(?i)/chat/completions$").unwrap(), OPENAI_CHAT),
        (Regex::new(r"(?i)/responses$").unwrap(), OPENAI_RESPONSES),
        (Regex::new(r"(?i)/messages$").unwrap(), ANTHROPIC),
        (
            Regex::new(r"(?i)/(?:api|v\d+)/chat$|/chat$").unwrap(),
            OPENAI_CHAT,
        ),
        (
            Regex::new(r"(?i)/(?:v\d+/)?completions?$|/generate(?:_stream)?$|/api/generate$")
                .unwrap(),
            COMPLETIONS,
        ),
        (
            Regex::new(r"(?i):(?:stream)?generateContent$").unwrap(),
            GEMINI,
        ),
        (
            Regex::new(r"(?i)/(?:converse(?:-stream)?|invoke(?:-with-response-stream)?|invocations)$")
                .unwrap(),
            "generic",
        ),
    ]
});

static MODEL_FROM_PATH_RE: Lazy<Vec<Regex>> = Lazy::new(|| {
    vec![
        Regex::new(r"(?i)/deployments/([^/]+)/").unwrap(),
        Regex::new(r"(?i)/models/([^/:?]+)(?:[:/])").unwrap(),
        Regex::new(r"(?i)/model/([^/]+)/(?:converse|invoke)").unwrap(),
    ]
});

const PROVIDERS: &[(&str, &str)] = &[
    ("api.openai.com", "openai"),
    ("openai.azure.com", "azure_openai"),
    ("cognitiveservices.azure.com", "azure_openai"),
    ("services.ai.azure.com", "azure_ai_foundry"),
    ("models.inference.ai.azure.com", "azure_ai"),
    ("api.anthropic.com", "anthropic"),
    ("generativelanguage.googleapis.com", "google_gemini"),
    ("aiplatform.googleapis.com", "google_vertex"),
    ("api.mistral.ai", "mistral"),
    ("api.groq.com", "groq"),
    ("openrouter.ai", "openrouter"),
    ("api.together.xyz", "together"),
    ("api.together.ai", "together"),
    ("api.fireworks.ai", "fireworks"),
    ("api.deepseek.com", "deepseek"),
    ("api.x.ai", "xai"),
    ("api.cohere.com", "cohere"),
    ("api.cohere.ai", "cohere"),
    ("api-inference.huggingface.co", "huggingface"),
    ("router.huggingface.co", "huggingface"),
    ("integrate.api.nvidia.com", "nvidia_nim"),
    ("api.perplexity.ai", "perplexity"),
    ("api.cerebras.ai", "cerebras"),
    ("api.sambanova.ai", "sambanova"),
    ("api.githubcopilot.com", "github_copilot"),
    ("models.github.ai", "github_models"),
];

const PRIVATE_SUFFIXES: &[&str] = &[
    ".internal",
    ".local",
    ".lan",
    ".corp",
    ".intranet",
    ".home.arpa",
    ".svc",
    ".cluster.local",
    ".private",
    ".priv",
    ".localdomain",
    ".home",
    ".intra",
];

const NAME_TOKENS: &[&str] = &[
    "vllm",
    "ollama",
    "llm",
    "llms",
    "llama",
    "llamacpp",
    "openai",
    "inference",
    "tgi",
    "litellm",
    "lmstudio",
    "triton",
    "sglang",
    "localai",
    "gpt",
    "genai",
    "aoai",
    "qwen",
    "gemma",
    "mistral",
    "deepseek",
    "phi",
    "model",
    "models",
    "ai",
];

const PORT_HINTS: &[u16] = &[8000, 8001, 8080, 11434, 4000, 1234, 30000, 5000, 9000, 3000];

pub fn normalize_path(path: &str) -> String {
    let base = path.split('?').next().unwrap_or("").split('#').next().unwrap_or("");
    let trimmed = base.trim_end_matches('/');
    if trimmed.is_empty() {
        "/".to_string()
    } else {
        trimmed.to_string()
    }
}

pub fn plausible(method: &str, path: &str, body_text: &str) -> bool {
    if !method.eq_ignore_ascii_case("POST") || body_text.is_empty() {
        return false;
    }
    let head_sample = if body_text.len() > 512 {
        let mut end = 512;
        while !body_text.is_char_boundary(end) {
            end -= 1;
        }
        &body_text[..end]
    } else {
        body_text
    };
    let head = head_sample.trim_start();
    if head.len() < 8 || !head.starts_with('{') {
        return false;
    }
    if EXCLUDED_PATH_RE.is_match(&normalize_path(path)) {
        return false;
    }
    if body_text.len() <= 81_920 {
        return PLAUSIBLE_KEYS_RE.is_match(body_text);
    }
    let mut front_end = 65_536;
    while !body_text.is_char_boundary(front_end) {
        front_end -= 1;
    }
    if PLAUSIBLE_KEYS_RE.is_match(&body_text[..front_end]) {
        return true;
    }
    let mut tail_start = body_text.len().saturating_sub(16_384);
    while !body_text.is_char_boundary(tail_start) {
        tail_start += 1;
    }
    PLAUSIBLE_KEYS_RE.is_match(&body_text[tail_start..])
}

#[derive(Debug, Clone, Default)]
pub struct HostInfo {
    pub cls: String,
    pub points: i32,
    pub reasons: Vec<String>,
}

fn is_cgnat_v4(ip: std::net::Ipv4Addr) -> bool {
    let octets = ip.octets();
    octets[0] == 100 && (octets[1] & 0b1100_0000) == 64
}

pub fn classify_host(host: &str, port: u16) -> HostInfo {
    let mut h = host
        .trim()
        .trim_matches(|c| c == '[' || c == ']')
        .to_ascii_lowercase();
    while h.ends_with('.') {
        h.pop();
    }
    if let Some((before_zone, _)) = h.split_once('%') {
        h = before_zone.to_string();
    }
    let mut info = HostInfo::default();
    if h.is_empty() {
        info.cls = "unknown".to_string();
        return info;
    }
    for &(suffix, name) in PROVIDERS {
        if h == suffix || h.ends_with(&format!(".{suffix}")) {
            info.cls = format!("provider:{name}");
            info.points = 15;
            info.reasons.push(format!("known provider domain ({name})"));
            if h.contains("privatelink") {
                info.reasons.push("private endpoint".to_string());
            }
            return info;
        }
    }
    if BEDROCK_HOST_RE.is_match(&h) {
        info.cls = "provider:aws_bedrock".to_string();
        info.points = 15;
        info.reasons
            .push("known provider domain (aws_bedrock)".to_string());
        return info;
    }
    let ip_opt = h.parse::<IpAddr>().ok();
    if let Some(ip) = ip_opt {
        match ip {
            IpAddr::V4(v4) => {
                if v4.is_loopback() {
                    info.cls = "loopback".to_string();
                } else if v4.is_private() || v4.is_link_local() || is_cgnat_v4(v4) {
                    info.cls = "private-ip".to_string();
                } else {
                    info.cls = "public".to_string();
                }
            }
            IpAddr::V6(v6) => {
                if v6.is_loopback() {
                    info.cls = "loopback".to_string();
                } else if (v6.segments()[0] & 0xfe00) == 0xfc00
                    || (v6.segments()[0] & 0xffc0) == 0xfe80
                {
                    info.cls = "private-ip".to_string();
                } else {
                    info.cls = "public".to_string();
                }
            }
        }
    } else if h == "localhost" || h.ends_with(".localhost") {
        info.cls = "loopback".to_string();
    } else if PRIVATE_SUFFIXES.iter().any(|s| h.ends_with(s))
        || h.contains(".privatelink.")
        || h.starts_with("privatelink.")
        || h == "host.docker.internal"
    {
        info.cls = "private-dns".to_string();
    } else if !h.contains('.') {
        info.cls = "local-name".to_string();
    } else {
        info.cls = "public".to_string();
    }

    if matches!(
        info.cls.as_str(),
        "loopback" | "private-ip" | "private-dns" | "local-name"
    ) {
        info.points += 5;
        info.reasons.push(format!("non-public host ({})", info.cls));
    }
    if ip_opt.is_none()
        && HOST_SPLIT_RE
            .split(&h)
            .any(|tok| NAME_TOKENS.contains(&tok))
    {
        info.points += 5;
        info.reasons
            .push("host name mentions an LLM server".to_string());
    }
    if PORT_HINTS.contains(&port) {
        info.points += 5;
        info.reasons
            .push(format!("conventional LLM server port ({port})"));
    }
    info
}

pub fn path_hint(path: &str) -> Option<&'static str> {
    let p = normalize_path(path);
    for (rx, api) in PATH_HINTS.iter() {
        if rx.is_match(&p) {
            return Some(*api);
        }
    }
    None
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(hex_str) = std::str::from_utf8(&bytes[i + 1..i + 3]) {
                if let Ok(val) = u8::from_str_radix(hex_str, 16) {
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

pub fn model_from_path(path: &str) -> Option<String> {
    let p = format!("{}/", normalize_path(path));
    for rx in MODEL_FROM_PATH_RE.iter() {
        if let Some(caps) = rx.captures(&p) {
            if let Some(m) = caps.get(1) {
                return Some(percent_decode(m.as_str()));
            }
        }
    }
    None
}

const ROLES: &[&str] = &[
    "system",
    "user",
    "assistant",
    "developer",
    "tool",
    "function",
    "model",
    "human",
    "ai",
    "chatbot",
];
const RESPONSES_KEYS: &[&str] = &[
    "instructions",
    "previous_response_id",
    "max_output_tokens",
    "reasoning",
    "store",
    "include",
    "truncation",
    "parallel_tool_calls",
];
const SAMPLING_KEYS: &[&str] = &[
    "max_tokens",
    "temperature",
    "top_p",
    "top_k",
    "stop",
    "n_predict",
    "max_new_tokens",
    "stream",
    "n",
    "logprobs",
    "frequency_penalty",
    "presence_penalty",
    "repetition_penalty",
    "min_p",
    "seed",
    "sampling_params",
    "options",
    "raw",
    "system",
    "template",
    "parameters",
    "preamble",
    "connectors",
];
const ANTHROPIC_BLOCKS: &[&str] = &[
    "tool_use",
    "tool_result",
    "thinking",
    "redacted_thinking",
    "document",
];

#[derive(Debug, Clone, Default)]
pub struct Shape {
    pub api: Option<String>,
    pub strong: bool,
    pub points: i32,
    pub reasons: Vec<String>,
}

fn has_anthropic_blocks(msgs: &[Value]) -> bool {
    for m in msgs {
        if let Some(carr) = m
            .as_object()
            .and_then(|o| o.get("content"))
            .and_then(|v| v.as_array())
        {
            let found = carr.iter().any(|b| {
                b.as_object()
                    .map(|bo| {
                        let t = bo.get("type").and_then(|v| v.as_str()).unwrap_or("");
                        ANTHROPIC_BLOCKS.contains(&t) || (t == "image" && bo.contains_key("source"))
                    })
                    .unwrap_or(false)
            });
            if found {
                return true;
            }
        }
    }
    false
}

pub fn classify_body(val: Option<&Value>) -> Shape {
    let mut sh = Shape::default();
    let Some(Value::Object(obj)) = val else {
        return sh;
    };
    if let Some(Value::Array(msgs)) = obj.get("messages") {
        let has_role = msgs.iter().any(|m| {
            m.as_object()
                .and_then(|o| o.get("role"))
                .and_then(|v| v.as_str())
                .map(|r| ROLES.contains(&r.to_ascii_lowercase().as_str()))
                .unwrap_or(false)
        });
        let has_content = msgs.iter().any(|m| {
            m.as_object()
                .map(|o| {
                    o.contains_key("content")
                        || o.contains_key("tool_calls")
                        || o.contains_key("parts")
                        || o.contains_key("message")
                })
                .unwrap_or(false)
        });
        if !msgs.is_empty() && has_role && has_content {
            sh.strong = true;
            sh.points = 60;
            sh.api = Some(OPENAI_CHAT.to_string());
            sh.reasons
                .push("body has a messages list of role/content objects".to_string());
            if obj.contains_key("anthropic_version")
                || (obj.contains_key("system") && obj.contains_key("max_tokens"))
                || has_anthropic_blocks(msgs)
            {
                sh.api = Some(ANTHROPIC.to_string());
                sh.reasons.push(
                    "Anthropic-style (anthropic_version, top-level system or content blocks)"
                        .to_string(),
                );
            }
            return sh;
        }
    }
    if let Some(msg_s) = obj.get("message").and_then(|v| v.as_str()) {
        if !msg_s.is_empty() {
            if obj.get("chat_history").map(|v| v.is_array()).unwrap_or(false)
                || obj.get("preamble").map(|v| v.is_string()).unwrap_or(false)
            {
                sh.strong = true;
                sh.points = 60;
                sh.api = Some(OPENAI_CHAT.to_string());
                sh.reasons
                    .push("body has Cohere chat_history/preamble and message".to_string());
                return sh;
            }
            if obj
                .get("model")
                .and_then(|v| v.as_str())
                .map(|s| !s.is_empty())
                .unwrap_or(false)
            {
                sh.api = Some(OPENAI_CHAT.to_string());
                sh.points = 25;
                sh.reasons
                    .push("body has model and message (Cohere chat shape: needs corroboration)".to_string());
                return sh;
            }
        }
    }
    if let Some(Value::Array(contents)) = obj.get("contents") {
        if !contents.is_empty()
            && contents.iter().any(|c| {
                c.as_object()
                    .and_then(|o| o.get("parts"))
                    .map(|v| v.is_array())
                    .unwrap_or(false)
            })
        {
            sh.strong = true;
            sh.points = 60;
            sh.api = Some(GEMINI.to_string());
            sh.reasons.push("body has Gemini contents/parts".to_string());
            return sh;
        }
    }
    if let Some(inp) = obj.get("input") {
        if !inp.is_null() && !obj.contains_key("messages") && !obj.contains_key("prompt") {
            let rkeys: Vec<&str> = RESPONSES_KEYS
                .iter()
                .copied()
                .filter(|k| obj.contains_key(*k))
                .collect();
            let item_shape = inp
                .as_array()
                .map(|arr| {
                    arr.iter().any(|i| {
                        i.as_object()
                            .map(|o| {
                                o.contains_key("role")
                                    || matches!(
                                        o.get("type").and_then(|v| v.as_str()),
                                        Some(
                                            "message"
                                                | "function_call"
                                                | "function_call_output"
                                                | "input_text"
                                        )
                                    )
                            })
                            .unwrap_or(false)
                    })
                })
                .unwrap_or(false);
            if item_shape || !rkeys.is_empty() {
                sh.strong = true;
                sh.points = 60;
                sh.api = Some(OPENAI_RESPONSES.to_string());
                let suffix = rkeys
                    .first()
                    .map(|k| format!(" with {k}"))
                    .unwrap_or_default();
                sh.reasons
                    .push(format!("body has a Responses-style input{suffix}"));
                return sh;
            }
            let bare_ok = match inp {
                Value::String(_) => true,
                Value::Array(arr) => arr
                    .iter()
                    .all(|x| x.is_string() || (x.is_i64() && !x.is_boolean())),
                _ => false,
            };
            if bare_ok {
                sh.api = Some(OPENAI_RESPONSES.to_string());
                sh.points = 25;
                sh.reasons.push(
                    "body has a bare input (Responses API or embeddings: needs corroboration)"
                        .to_string(),
                );
                return sh;
            }
        }
    }
    for key in ["prompt", "inputs"] {
        if let Some(val) = obj.get(key) {
            let non_empty = match val {
                Value::String(s) => !s.is_empty(),
                Value::Array(a) => !a.is_empty(),
                _ => false,
            };
            if non_empty {
                sh.api = Some(COMPLETIONS.to_string());
                sh.points = 25;
                sh.reasons.push(format!(
                    "body has a {key} (text completion shape: needs corroboration)"
                ));
                if obj.keys().any(|k| SAMPLING_KEYS.contains(&k.as_str())) {
                    sh.points += 10;
                    sh.reasons.push("with sampling parameters".to_string());
                }
                return sh;
            }
        }
    }
    sh
}

const USAGE_KEYS: &[&str] = &[
    "prompt_tokens",
    "completion_tokens",
    "input_tokens",
    "output_tokens",
    "total_tokens",
    "inputTokens",
    "outputTokens",
    "totalTokens",
];
const OBJECT_TYPES: &[&str] = &[
    "chat.completion",
    "chat.completion.chunk",
    "text_completion",
    "response",
];
const TYPE_VALUES: &[&str] = &[
    "message",
    "message_start",
    "content_block_delta",
    "content_block_start",
    "message_delta",
    "message-start",
    "content-delta",
    "message-end",
];

fn response_objects(text: &str, sse: bool) -> Vec<Map<String, Value>> {
    let raw = text.trim();
    if raw.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    if sse || raw.starts_with("data:") || raw.starts_with("event:") {
        for line in raw.lines() {
            if let Some(rest) = line.strip_prefix("data:") {
                let payload = rest.trim();
                if !payload.is_empty() && payload != "[DONE]" {
                    if let Ok(Value::Object(o)) = serde_json::from_str::<Value>(payload) {
                        out.push(o);
                        if out.len() >= 200 {
                            return out;
                        }
                    }
                }
            }
        }
        return out;
    }
    if let Ok(val) = serde_json::from_str::<Value>(raw) {
        match val {
            Value::Object(o) => {
                out.push(o);
                return out;
            }
            Value::Array(arr) => {
                for x in arr.into_iter().take(200) {
                    if let Value::Object(o) = x {
                        out.push(o);
                    }
                }
                return out;
            }
            _ => {}
        }
    }
    let mut ndjson = Vec::new();
    let mut ok = true;
    for line in raw.lines().take(200) {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        match serde_json::from_str::<Value>(line) {
            Ok(Value::Object(o)) => ndjson.push(o),
            _ => {
                ok = false;
                break;
            }
        }
    }
    if ok && !ndjson.is_empty() {
        return ndjson;
    }
    extract_eventstream_objects_str(raw)
        .into_iter()
        .take(200)
        .collect()
}

pub fn response_evidence(text: &str, sse: bool) -> Option<&'static str> {
    for o in response_objects(text, sse) {
        if let Some(u) = o.get("usage").and_then(|v| v.as_object()) {
            if USAGE_KEYS.iter().any(|k| u.contains_key(*k))
                || u.get("tokens")
                    .or_else(|| u.get("billed_units"))
                    .map(|v| v.is_object())
                    .unwrap_or(false)
            {
                return Some("response has token usage");
            }
        }
        if let Some(meta) = o
            .get("meta")
            .or_else(|| o.get("metadata"))
            .and_then(|v| v.as_object())
        {
            if meta
                .get("usage")
                .or_else(|| meta.get("tokens"))
                .or_else(|| meta.get("billed_units"))
                .map(|v| v.is_object())
                .unwrap_or(false)
            {
                return Some("response has token usage");
            }
        }
        if o.get("amazon-bedrock-invocationMetrics")
            .map(|v| v.is_object())
            .unwrap_or(false)
        {
            return Some("response has Bedrock invocation metrics");
        }
        if o.get("usageMetadata")
            .map(|v| v.is_object())
            .unwrap_or(false)
        {
            return Some("response has usageMetadata");
        }
        if let Some(ch) = o.get("choices").and_then(|v| v.as_array()) {
            if ch.iter().any(|c| {
                c.as_object()
                    .map(|co| {
                        co.contains_key("message")
                            || co.contains_key("delta")
                            || co.contains_key("text")
                    })
                    .unwrap_or(false)
            }) {
                return Some("response has choices");
            }
        }
        if o.get("candidates")
            .and_then(|v| v.as_array())
            .map(|a| !a.is_empty())
            .unwrap_or(false)
        {
            return Some("response has candidates");
        }
        if o.get("object")
            .and_then(|v| v.as_str())
            .map(|s| OBJECT_TYPES.contains(&s))
            .unwrap_or(false)
            || o.get("type")
                .and_then(|v| v.as_str())
                .map(|s| TYPE_VALUES.contains(&s))
                .unwrap_or(false)
        {
            return Some("response has a model-response type");
        }
        if o.get("contentBlockDelta")
            .or_else(|| o.get("contentBlockStart"))
            .map(|v| v.is_object())
            .unwrap_or(false)
        {
            return Some("response has Bedrock Converse stream blocks");
        }
        if let Some(c) = o.get("content").and_then(|v| v.as_array()) {
            if c.iter().any(|b| {
                b.as_object()
                    .and_then(|bo| bo.get("type"))
                    .and_then(|v| v.as_str())
                    .map(|t| matches!(t, "text" | "tool_use" | "thinking"))
                    .unwrap_or(false)
            }) {
                return Some("response has content blocks");
            }
        }
        if o.get("output")
            .and_then(|v| v.as_object())
            .and_then(|oo| oo.get("message"))
            .and_then(|v| v.as_object())
            .and_then(|m| m.get("content"))
            .map(|v| v.is_array())
            .unwrap_or(false)
        {
            return Some("response has Bedrock Converse output");
        }
        if let Some(out_arr) = o.get("output").and_then(|v| v.as_array()) {
            if out_arr.iter().any(|i| {
                i.as_object()
                    .and_then(|io| io.get("type"))
                    .and_then(|v| v.as_str())
                    .map(|t| matches!(t, "message" | "function_call" | "reasoning"))
                    .unwrap_or(false)
            }) {
                return Some("response has Responses output items");
            }
        }
        if o.contains_key("prompt_eval_count")
            || o.contains_key("eval_count")
            || o.contains_key("tokens_evaluated")
            || o.contains_key("generated_text")
        {
            return Some("response has self-hosted server counters");
        }
        if o.get("done").is_some()
            && (o.contains_key("response")
                || o.get("message").map(|v| v.is_object()).unwrap_or(false))
        {
            return Some("response looks like an Ollama answer");
        }
    }
    None
}

fn split_list(val: Option<String>) -> Vec<String> {
    val.unwrap_or_default()
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

#[derive(Debug, Clone, Default)]
pub struct DetectConfig {
    pub hosts: Vec<String>,
    pub paths: Vec<String>,
    pub exclude_hosts: Vec<String>,
}

impl DetectConfig {
    pub fn from_env() -> Self {
        Self {
            hosts: split_list(std::env::var("TOKPROF_LLM_HOSTS").ok()),
            paths: split_list(std::env::var("TOKPROF_LLM_PATHS").ok()),
            exclude_hosts: split_list(std::env::var("TOKPROF_EXCLUDE_HOSTS").ok()),
        }
    }
}

pub fn glob_match(pat: &str, text: &str) -> bool {
    if pat == "*" {
        return true;
    }
    let p: Vec<char> = pat.chars().collect();
    let t: Vec<char> = text.chars().collect();
    let (mut pi, mut ti) = (0usize, 0usize);
    let mut star_idx: Option<usize> = None;
    let mut match_idx = 0usize;

    while ti < t.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == t[ti]) {
            pi += 1;
            ti += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star_idx = Some(pi);
            match_idx = ti;
            pi += 1;
        } else if let Some(si) = star_idx {
            pi = si + 1;
            match_idx += 1;
            ti = match_idx;
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == '*' {
        pi += 1;
    }
    pi == p.len()
}

pub fn cidr_match(cidr: &str, host: &str) -> bool {
    let Some((net_str, prefix_str)) = cidr.split_once('/') else {
        return false;
    };
    let Ok(prefix) = prefix_str.parse::<u32>() else {
        return false;
    };
    let Ok(net_ip) = net_str.parse::<IpAddr>() else {
        return false;
    };
    let Ok(host_ip) = host.parse::<IpAddr>() else {
        return false;
    };
    match (net_ip, host_ip) {
        (IpAddr::V4(n), IpAddr::V4(h)) if prefix <= 32 => {
            if prefix == 0 {
                return true;
            }
            let mask = u32::MAX << (32 - prefix);
            (u32::from(n) & mask) == (u32::from(h) & mask)
        }
        (IpAddr::V6(n), IpAddr::V6(h)) if prefix <= 128 => {
            if prefix == 0 {
                return true;
            }
            let mask = u128::MAX << (128 - prefix);
            (u128::from(n) & mask) == (u128::from(h) & mask)
        }
        _ => false,
    }
}

pub fn host_matches(patterns: &[String], host: &str, port: u16) -> bool {
    if patterns.is_empty() {
        return false;
    }
    let mut h = host
        .trim()
        .trim_matches(|c| c == '[' || c == ']')
        .to_ascii_lowercase();
    if let Some((before_zone, _)) = h.split_once('%') {
        h = before_zone.to_string();
    }
    for pat in patterns {
        let mut p = pat.to_ascii_lowercase();
        if p.contains('/') {
            if cidr_match(&p, &h) {
                return true;
            }
            continue;
        }
        let mut want_port: Option<u16> = None;
        if p.matches(':').count() == 1 {
            if let Some((left, right)) = p.rsplit_once(':') {
                if let Ok(pn) = right.parse::<u16>() {
                    p = left.to_string();
                    want_port = Some(pn);
                }
            }
        }
        if let Some(wp) = want_port {
            if wp != port {
                continue;
            }
        }
        if glob_match(&p, &h) {
            return true;
        }
    }
    false
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Verdict {
    pub llm: bool,
    pub candidate: bool,
    pub api: Option<String>,
    pub score: i32,
    pub evidence: Vec<String>,
    pub host_class: String,
    pub model_hint: Option<String>,
    pub stream_hint: bool,
    pub forced: bool,
    pub reject: Option<String>,
}

#[derive(Debug, Clone)]
pub struct EndpointStat {
    pub host: String,
    pub port: u16,
    pub host_class: String,
    pub calls: usize,
    pub apis: BTreeMap<String, usize>,
    pub paths: BTreeMap<String, usize>,
    pub how: BTreeSet<String>,
    pub confirmed: bool,
}

#[derive(Default)]
struct DetectorState {
    known: HashMap<(String, u16, String), String>,
    endpoints: HashMap<(String, u16), EndpointStat>,
    missed: HashMap<(String, u16, String), usize>,
    rejected: HashMap<String, usize>,
}

pub struct Detector {
    pub cfg: DetectConfig,
    path_rx: Vec<Regex>,
    state: Mutex<DetectorState>,
}

impl Default for Detector {
    fn default() -> Self {
        Self::new(None)
    }
}

impl Detector {
    pub fn new(config: Option<DetectConfig>) -> Self {
        let cfg = config.unwrap_or_else(DetectConfig::from_env);
        let mut path_rx = Vec::new();
        for p in &cfg.paths {
            if let Ok(rx) = Regex::new(&format!("(?i){p}")) {
                path_rx.push(rx);
            }
        }
        Self {
            cfg,
            path_rx,
            state: Mutex::new(DetectorState::default()),
        }
    }

    fn reject(&self, mut v: Verdict, reason: impl Into<String>) -> Verdict {
        let r = reason.into();
        v.reject = Some(r.clone());
        if let Ok(mut st) = self.state.lock() {
            *st.rejected.entry(r).or_insert(0) += 1;
        }
        v
    }

    pub fn inspect(
        &self,
        method: &str,
        host: &str,
        port: u16,
        path: &str,
        body: Option<&Value>,
    ) -> Verdict {
        let mut v = Verdict::default();
        let hostinfo = classify_host(host, port);
        v.host_class = hostinfo.cls;
        if !method.eq_ignore_ascii_case("POST") {
            return self.reject(v, "not a POST request");
        }
        let npath = normalize_path(path);
        if !host.is_empty() && host_matches(&self.cfg.exclude_hosts, host, port) {
            return self.reject(v, "host excluded by --exclude-host");
        }
        if let Some(m) = EXCLUDED_PATH_RE.find(&npath) {
            let matched = m.as_str().trim_matches(|c| c == '/' || c == ':');
            return self.reject(v, format!("non-generative endpoint ({matched})"));
        }
        let shape = classify_body(body);
        let hint = path_hint(&npath);
        v.model_hint = model_from_path(&npath);
        v.stream_hint = STREAM_PATH_RE.is_match(&npath);
        let forced = (!host.is_empty() && host_matches(&self.cfg.hosts, host, port))
            || self.path_rx.iter().any(|rx| rx.is_match(&npath));

        let mut api = shape.api.clone();
        if shape.strong
            && matches!(api.as_deref(), Some(OPENAI_CHAT | ANTHROPIC))
            && matches!(hint, Some(OPENAI_CHAT | ANTHROPIC))
        {
            api = hint.map(|s| s.to_string());
        }
        v.api = api.clone();
        let Some(api_str) = api else {
            return self.reject(
                v,
                if forced {
                    "forced by --llm-host/--llm-path but the body has no recognizable LLM shape"
                } else {
                    "body has no LLM request shape"
                },
            );
        };

        let mut score = shape.points;
        v.evidence.extend(shape.reasons);
        if let Some(h) = hint {
            if h != "generic" {
                if h == api_str {
                    score += 25;
                    v.evidence.push(format!("path looks like the {h} API"));
                }
            } else {
                score += 10;
                v.evidence
                    .push("path looks like a model invocation endpoint".to_string());
            }
        }
        score += hostinfo.points;
        v.evidence.extend(hostinfo.reasons);
        let known = self
            .state
            .lock()
            .map(|st| {
                st.known
                    .contains_key(&(host.to_string(), port, npath.clone()))
            })
            .unwrap_or(false);
        if known && !shape.strong {
            score += 30;
            v.evidence.push(
                "this endpoint already returned a model answer in this session".to_string(),
            );
        }
        if forced {
            score += 100;
            v.forced = true;
            v.evidence
                .push("forced by --llm-host/--llm-path".to_string());
        }
        v.score = score;
        if score >= ACCEPT {
            v.llm = true;
        } else if score >= CANDIDATE {
            v.candidate = true;
        } else {
            return self.reject(v, "evidence too weak");
        }
        v
    }

    pub fn confirm(
        &self,
        v: &mut Verdict,
        host: &str,
        port: u16,
        path: &str,
        text: &str,
        sse: bool,
    ) -> Option<String> {
        let ev = response_evidence(text, sse);
        let Some(ev_str) = ev else {
            if let Ok(mut st) = self.state.lock() {
                let key = (host.to_string(), port, normalize_path(path));
                if st.missed.len() < 4096 || st.missed.contains_key(&key) {
                    *st.missed.entry(key).or_insert(0) += 1;
                }
            }
            return None;
        };
        v.evidence.push(ev_str.to_string());
        v.score += 40;
        v.llm = true;
        v.candidate = false;
        Some(ev_str.to_string())
    }

    pub fn accept(&self, v: &Verdict, host: &str, port: u16, path: &str) {
        let Ok(mut st) = self.state.lock() else {
            return;
        };
        let key = (host.to_string(), port);
        if !st.endpoints.contains_key(&key) && st.endpoints.len() >= 1024 {
            if let Some(first_k) = st.endpoints.keys().next().cloned() {
                st.endpoints.remove(&first_k);
            }
        }
        let ep = st.endpoints.entry(key).or_insert_with(|| EndpointStat {
            host: host.to_string(),
            port,
            host_class: v.host_class.clone(),
            calls: 0,
            apis: BTreeMap::new(),
            paths: BTreeMap::new(),
            how: BTreeSet::new(),
            confirmed: false,
        });
        ep.calls += 1;
        if let Some(ref a) = v.api {
            *ep.apis.entry(a.clone()).or_insert(0) += 1;
        }
        let np = normalize_path(path);
        if ep.paths.len() < 8 || ep.paths.contains_key(&np) {
            *ep.paths.entry(np.clone()).or_insert(0) += 1;
        }
        if let Some(first_ev) = v.evidence.first() {
            ep.how.insert(first_ev.clone());
        }
        let confirmed = v.forced
            || v.evidence.iter().any(|e| e.starts_with("response has"))
            || v.score >= 60;
        if confirmed {
            ep.confirmed = true;
            if let Some(ref a) = v.api {
                let kkey = (host.to_string(), port, np);
                if st.known.len() < 4096 || st.known.contains_key(&kkey) {
                    st.known.insert(kkey, a.clone());
                }
            }
        }
    }

    pub fn render_summary(&self) -> String {
        let Ok(st) = self.state.lock() else {
            return String::new();
        };
        let mut lines = Vec::new();
        let mut eps: Vec<&EndpointStat> = st.endpoints.values().collect();
        eps.sort_by_key(|b| std::cmp::Reverse(b.calls));
        if !eps.is_empty() {
            lines.push("tokprof: detected LLM endpoint(s):".to_string());
            for s in eps {
                let addr = if s.port != 0 && s.port != 80 && s.port != 443 {
                    format!("{}:{}", if s.host.is_empty() { "?" } else { &s.host }, s.port)
                } else if s.host.is_empty() {
                    "?".to_string()
                } else {
                    s.host.clone()
                };
                let apis: Vec<&str> = s.apis.keys().map(|x| x.as_str()).collect();
                let paths: Vec<&str> = s.paths.keys().map(|x| x.as_str()).collect();
                let how: Vec<&str> = s.how.iter().map(|x| x.as_str()).collect();
                lines.push(format!(
                    "  {}  [{}]  api={}  path={}  calls={}  ({})",
                    addr,
                    s.host_class,
                    apis.join(","),
                    paths.join(","),
                    s.calls,
                    how.join("; ")
                ));
            }
        }
        let mut missed: Vec<_> = st.missed.iter().collect();
        missed.sort_by(|a, b| b.1.cmp(a.1));
        if !missed.is_empty() {
            lines.push(
                "tokprof: requests with a prompt/input body whose response did not look like a model answer:"
                    .to_string(),
            );
            for ((h, p, path), n) in missed.into_iter().take(5) {
                let addr = if *p != 0 && *p != 80 && *p != 443 {
                    format!("{h}:{p}")
                } else {
                    h.clone()
                };
                lines.push(format!(
                    "  {addr}{path} ({n}x) -- if this IS an LLM server, re-run with --llm-host {h} or --llm-path '{path}$'"
                ));
            }
        }
        lines.join("\n")
    }
}

pub fn parse_url_parts(url: &str) -> (String, String, u16, String) {
    let raw = if url.contains("://") {
        url.to_string()
    } else {
        format!("https://{url}")
    };
    let (scheme_part, rest) = raw.split_once("://").unwrap_or(("https", &raw));
    let scheme = scheme_part.to_ascii_lowercase();
    let (authority, path_part) = match rest.find('/') {
        Some(idx) => (&rest[..idx], &rest[idx..]),
        None => (rest, "/"),
    };
    let host_port = authority.rsplit('@').next().unwrap_or(authority);
    let (host, port_opt) = if host_port.starts_with('[') {
        if let Some(end_b) = host_port.find(']') {
            let h = &host_port[1..end_b];
            let p = host_port[end_b + 1..]
                .strip_prefix(':')
                .and_then(|s| s.parse::<u16>().ok());
            (h.to_string(), p)
        } else {
            (host_port.to_string(), None)
        }
    } else if let Some((h, p_str)) = host_port.rsplit_once(':') {
        if let Ok(p) = p_str.parse::<u16>() {
            (h.to_string(), Some(p))
        } else {
            (host_port.to_string(), None)
        }
    } else {
        (host_port.to_string(), None)
    };
    let default_port = if scheme == "http" { 80 } else { 443 };
    let port = port_opt.unwrap_or(default_port);
    let clean_path = path_part.split('?').next().unwrap_or("/");
    (
        scheme,
        host,
        port,
        if clean_path.is_empty() {
            "/".to_string()
        } else {
            clean_path.to_string()
        },
    )
}

pub fn explain(
    url: &str,
    body: Option<&Value>,
    response: Option<&str>,
    sse: bool,
    detector: Option<&Detector>,
) -> String {
    let default_det = Detector::default();
    let det = detector.unwrap_or(&default_det);
    let (_, host, port, path) = parse_url_parts(url);
    let mut v = det.inspect("POST", &host, port, &path, body);
    if v.candidate && response.is_some() {
        det.confirm(&mut v, &host, port, &path, response.unwrap_or(""), sse);
    }
    let state = if v.llm {
        "LLM call"
    } else if v.candidate {
        "CANDIDATE (decided by the response)"
    } else {
        "NOT an LLM call"
    };
    let mut lines = vec![
        format!("URL:        {url}"),
        format!("Host class: {}", v.host_class),
        format!(
            "Verdict:    {state}  (score {}, accept >= {ACCEPT}, candidate >= {CANDIDATE})",
            v.score
        ),
        format!("API family: {}", v.api.as_deref().unwrap_or("-")),
    ];
    if let Some(ref mh) = v.model_hint {
        lines.push(format!("Model hint: {mh} (from URL path)"));
    }
    if v.stream_hint {
        lines.push("Stream:     yes (from URL path)".to_string());
    }
    if !v.evidence.is_empty() {
        lines.push("Evidence:".to_string());
        for e in &v.evidence {
            lines.push(format!("  + {e}"));
        }
    }
    if let Some(ref rej) = v.reject {
        lines.push(format!("Rejected:   {rej}"));
    }
    if v.candidate && response.is_none() {
        lines.push(format!(
            "Note:       pass --response FILE (or --llm-host {host}) to settle a candidate."
        ));
    }
    lines.join("\n")
}
