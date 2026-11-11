use crate::model::RawSegment;
use once_cell::sync::Lazy;
use regex::Regex;
use serde_json::{Map, Value};
use std::collections::{BTreeMap, HashMap};

pub const OPENAI_CHAT: &str = "openai_chat";
pub const OPENAI_RESPONSES: &str = "openai_responses";
pub const ANTHROPIC: &str = "anthropic";
pub const COMPLETIONS: &str = "completions";
pub const GEMINI: &str = "gemini";

const TEXT_PARTS: &[&str] = &["text", "input_text", "output_text"];
const OPAQUE_PARTS: &[&str] = &[
    "image_url",
    "input_image",
    "image",
    "input_file",
    "file",
    "document",
    "input_audio",
    "audio",
];

const WRAPPER_KEYS: &[&str] = &[
    "untrusted_input",
    "payload",
    "input",
    "request",
    "data",
    "review_input",
];
const CONV_ID_KEYS: &[&str] = &[
    "conversation_id",
    "session_id",
    "thread_id",
    "chat_id",
    "dialog_id",
];
const USER_PROMPT_KEYS: &[&str] = &[
    "utterance",
    "user_utterance",
    "question",
    "user_question",
    "query",
    "user_query",
    "user_prompt",
    "prompt",
    "message",
    "user_message",
    "current_turn",
    "current_input",
    "task",
    "problem_statement",
    "goal",
];
const HISTORY_KEYS: &[&str] = &[
    "context",
    "history",
    "conversation_history",
    "chat_history",
    "prior_turns",
    "previous_turns",
    "recent_turns",
    "turns",
    "dialogue",
    "messages",
    "memory",
    "conversation",
    "transcript",
    "previous_response",
    "recovery_context",
    "schema_repair",
];
const TOOL_DEF_KEYS: &[&str] = &[
    "capabilities",
    "descriptors",
    "tools",
    "functions",
    "available_tools",
    "tool_definitions",
    "catalog",
    "metric_concepts",
];
const TOOL_RESULT_KEYS: &[&str] = &[
    "tool_results",
    "observations",
    "evidence",
    "evidence_refs",
    "search_results",
    "retrieved_context",
    "documents",
    "chunks",
    "candidates",
    "frame",
    "semantic_judgment",
    "execution_result",
    "staged_projection",
    "draft",
    "proposal",
];
const SCHEMA_KEYS: &[&str] = &["output_schema", "response_schema", "json_schema", "schema"];
const SYSTEM_KEYS: &[&str] = &["instructions", "system_prompt"];

static ASST_PREFIX_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)^\s*(?:assistant|ai|bot|model|agent)\s*:\s*").unwrap());
static USER_PREFIX_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)^\s*(?:user|human|customer|operator)\s*:\s*").unwrap());

#[derive(Debug, Clone)]
pub struct ParsedRequest {
    pub provider: String,
    pub model: Option<String>,
    pub stream: bool,
    pub segments: Vec<RawSegment>,
    pub tools: Vec<Option<String>>,
    pub conv_hint: Option<String>,
    pub step_hint: Option<String>,
    pub chained_context: bool,
    pub cache_breakpoints: usize,
}

pub fn sorted_json(v: &Value) -> Value {
    match v {
        Value::Object(map) => {
            let mut btree = BTreeMap::new();
            for (k, val) in map {
                btree.insert(k.clone(), sorted_json(val));
            }
            let mut out = Map::new();
            for (k, val) in btree {
                out.insert(k, val);
            }
            Value::Object(out)
        }
        Value::Array(arr) => Value::Array(arr.iter().map(sorted_json).collect()),
        other => other.clone(),
    }
}

fn write_canonical_value(v: &Value, buf: &mut Vec<u8>) {
    match v {
        Value::Object(map) => {
            buf.push(b'{');
            let mut entries: Vec<(&String, &Value)> = map.iter().collect();
            entries.sort_unstable_by(|a, b| a.0.cmp(b.0));
            for (i, (k, val)) in entries.into_iter().enumerate() {
                if i > 0 {
                    buf.push(b',');
                }
                let _ = serde_json::to_writer(&mut *buf, k);
                buf.push(b':');
                write_canonical_value(val, buf);
            }
            buf.push(b'}');
        }
        Value::Array(arr) => {
            buf.push(b'[');
            for (i, item) in arr.iter().enumerate() {
                if i > 0 {
                    buf.push(b',');
                }
                write_canonical_value(item, buf);
            }
            buf.push(b']');
        }
        other => {
            let _ = serde_json::to_writer(buf, other);
        }
    }
}

pub fn dumps_canonical(v: &Value) -> String {
    let mut buf = Vec::with_capacity(128);
    write_canonical_value(v, &mut buf);
    String::from_utf8(buf).unwrap_or_else(|_| v.to_string())
}

pub fn norm_role(role: Option<&str>) -> String {
    let r = role.unwrap_or("").trim().to_ascii_lowercase();
    match r.as_str() {
        "chatbot" | "ai" | "model" => "assistant".to_string(),
        "human" => "user".to_string(),
        _ => r,
    }
}

pub fn content_text(content: Option<&Value>) -> (String, bool) {
    let Some(c) = content else {
        return (String::new(), false);
    };
    match c {
        Value::Null => (String::new(), false),
        Value::String(s) => (s.clone(), false),
        Value::Array(arr) => {
            let mut texts = Vec::new();
            let mut opaque = false;
            for part in arr {
                match part {
                    Value::String(s) => texts.push(s.clone()),
                    Value::Object(obj) => {
                        let t = obj.get("type").and_then(|v| v.as_str()).unwrap_or("");
                        if TEXT_PARTS.contains(&t) {
                            if let Some(txt) = obj.get("text").and_then(|v| v.as_str()) {
                                texts.push(txt.to_string());
                            }
                        } else if OPAQUE_PARTS.contains(&t) {
                            opaque = true;
                        } else if let Some(txt) = obj.get("text").and_then(|v| v.as_str()) {
                            texts.push(txt.to_string());
                        }
                    }
                    _ => {}
                }
            }
            (texts.join("\n"), opaque)
        }
        other => (dumps_canonical(other), false),
    }
}

fn decode_image_dims(b64_data: &str) -> Option<(u32, u32)> {
    use base64::Engine as _;
    let trimmed = b64_data.trim();
    let sample_end = trimmed.len().min(4096) & !3;
    if sample_end < 32 {
        return None;
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(&trimmed[..sample_end])
        .ok()?;
    if bytes.len() >= 24 && &bytes[0..8] == b"\x89PNG\r\n\x1a\n" {
        let w = u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);
        let h = u32::from_be_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]);
        if w > 0 && h > 0 {
            return Some((w, h));
        }
    }
    if bytes.len() >= 10 && (&bytes[0..6] == b"GIF87a" || &bytes[0..6] == b"GIF89a") {
        let w = u16::from_le_bytes([bytes[6], bytes[7]]) as u32;
        let h = u16::from_le_bytes([bytes[8], bytes[9]]) as u32;
        if w > 0 && h > 0 {
            return Some((w, h));
        }
    }
    if bytes.len() >= 4 && bytes[0] == 0xFF && bytes[1] == 0xD8 {
        let mut i = 2usize;
        while i + 9 < bytes.len() {
            if bytes[i] != 0xFF {
                i += 1;
                continue;
            }
            let marker = bytes[i + 1];
            if matches!(marker, 0xC0..=0xC3 | 0xC5..=0xC7 | 0xC9..=0xCB | 0xCD..=0xCF) {
                let h = u16::from_be_bytes([bytes[i + 5], bytes[i + 6]]) as u32;
                let w = u16::from_be_bytes([bytes[i + 7], bytes[i + 8]]) as u32;
                if w > 0 && h > 0 {
                    return Some((w, h));
                }
                break;
            }
            let seg_len = u16::from_be_bytes([bytes[i + 2], bytes[i + 3]]) as usize;
            if seg_len < 2 {
                break;
            }
            i += 2 + seg_len;
        }
    }
    None
}

fn vision_tile_tokens(w: u32, h: u32) -> i64 {
    let mut sw = w as f64;
    let mut sh = h as f64;
    if sw > 2048.0 || sh > 2048.0 {
        let scale = 2048.0 / sw.max(sh);
        sw *= scale;
        sh *= scale;
    }
    if sw.min(sh) > 768.0 {
        let scale = 768.0 / sw.min(sh);
        sw *= scale;
        sh *= scale;
    }
    let tiles_x = ((sw / 512.0).ceil() as i64).max(1);
    let tiles_y = ((sh / 512.0).ceil() as i64).max(1);
    85 + 170 * tiles_x * tiles_y
}

pub fn estimate_part_attachment_tokens(obj: &Map<String, Value>) -> i64 {
    let t = obj.get("type").and_then(|v| v.as_str()).unwrap_or("");
    let detail = obj
        .get("detail")
        .or_else(|| {
            obj.get("image_url")
                .and_then(|v| v.as_object())
                .and_then(|o| o.get("detail"))
        })
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if detail.eq_ignore_ascii_case("low") {
        return 85;
    }
    if let Some(iu) = obj.get("image_url").or_else(|| obj.get("input_image")) {
        let url_s = match iu {
            Value::String(s) => s.as_str(),
            Value::Object(o) => o
                .get("url")
                .or_else(|| o.get("image_url"))
                .and_then(|v| v.as_str())
                .unwrap_or(""),
            _ => "",
        };
        if let Some((_, b64)) = url_s.split_once(";base64,") {
            if let Some((w, h)) = decode_image_dims(b64) {
                return vision_tile_tokens(w, h);
            }
            return ((b64.len() as i64) / 512).clamp(85, 1445);
        }
        return 765;
    }
    if let Some(src) = obj
        .get("source")
        .or_else(|| obj.get("inlineData"))
        .or_else(|| obj.get("inline_data"))
        .and_then(|v| v.as_object())
    {
        let mtype = src
            .get("media_type")
            .or_else(|| src.get("mimeType"))
            .or_else(|| src.get("mime_type"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let data = src.get("data").and_then(|v| v.as_str()).unwrap_or("");
        if mtype.contains("pdf") || t == "document" {
            let approx_bytes = (data.len() * 3) / 4;
            let pages = (approx_bytes / 45_000).max(1) as i64;
            return (pages * 1500).clamp(250, 30_000);
        }
        if mtype.starts_with("audio/") || t == "input_audio" {
            let approx_bytes = (data.len() * 3) / 4;
            let secs = (approx_bytes / 16_000).max(1) as i64;
            return (secs * 25).clamp(25, 15_000);
        }
        if !data.is_empty() {
            if let Some((w, h)) = decode_image_dims(data) {
                return vision_tile_tokens(w, h);
            }
            return ((data.len() as i64) / 512).clamp(85, 1445);
        }
    }
    if t == "input_audio" || obj.contains_key("input_audio") {
        let data = obj
            .get("input_audio")
            .and_then(|v| v.as_object())
            .and_then(|o| o.get("data"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let approx_bytes = (data.len() * 3) / 4;
        return (((approx_bytes / 16_000).max(1) as i64) * 25).clamp(25, 15_000);
    }
    if t == "document" || t == "input_file" || t == "file" {
        return 1500;
    }
    765
}

fn estimate_content_attachment_tokens(content: Option<&Value>) -> i64 {
    let Some(Value::Array(arr)) = content else {
        return 765;
    };
    let mut sum = 0i64;
    for part in arr {
        if let Value::Object(obj) = part {
            let t = obj.get("type").and_then(|v| v.as_str()).unwrap_or("");
            if OPAQUE_PARTS.contains(&t)
                || obj.contains_key("inlineData")
                || obj.contains_key("inline_data")
                || obj.contains_key("fileData")
                || obj.contains_key("file_data")
                || obj.contains_key("image")
                || obj.contains_key("document")
            {
                sum += estimate_part_attachment_tokens(obj);
            }
        }
    }
    if sum > 0 { sum } else { 765 }
}

fn count_cache_breakpoints(v: &Value) -> usize {
    match v {
        Value::Object(map) => {
            let here = usize::from(map.contains_key("cache_control") || map.contains_key("cachePoint"));
            here + map.values().map(count_cache_breakpoints).sum::<usize>()
        }
        Value::Array(arr) => arr.iter().map(count_cache_breakpoints).sum(),
        _ => 0,
    }
}

fn add_seg(
    segments: &mut Vec<RawSegment>,
    kind: &str,
    text: String,
    role: Option<String>,
    name: Option<String>,
    turn: Option<i64>,
    opaque: bool,
) {
    if !text.is_empty() || opaque {
        segments.push(RawSegment {
            kind: kind.to_string(),
            text,
            role,
            name,
            turn,
            opaque,
            est_tokens: None,
        });
    }
}

fn is_empty_json(v: &Value) -> bool {
    match v {
        Value::Null => true,
        Value::String(s) => s.is_empty(),
        Value::Array(a) => a.is_empty(),
        Value::Object(o) => o.is_empty(),
        _ => false,
    }
}

fn emit_history_value(segments: &mut Vec<RawSegment>, key: &str, val: &Value, turn: Option<i64>) {
    if is_empty_json(val) {
        return;
    }
    if let Value::Array(items) = val {
        let mut u_lines = Vec::new();
        let mut a_lines = Vec::new();
        for item in items {
            match item {
                Value::Object(obj) => {
                    if let Some(role_v) = obj.get("role") {
                        let r = norm_role(role_v.as_str());
                        let src = obj
                            .get("content")
                            .or_else(|| obj.get("message"))
                            .or_else(|| obj.get("text"));
                        let (txt, _) = content_text(src);
                        if !txt.is_empty() {
                            if r == "assistant" {
                                a_lines.push(txt);
                            } else {
                                u_lines.push(txt);
                            }
                        }
                        continue;
                    }
                    if obj.contains_key("user") || obj.contains_key("assistant") {
                        if let Some(u) = obj.get("user").and_then(|v| v.as_str()) {
                            if !u.is_empty() {
                                u_lines.push(u.to_string());
                            }
                        }
                        if let Some(a) = obj.get("assistant").and_then(|v| v.as_str()) {
                            if !a.is_empty() {
                                a_lines.push(a.to_string());
                            }
                        }
                        continue;
                    }
                    u_lines.push(dumps_canonical(item));
                }
                Value::String(s) => {
                    let trimmed = s.trim();
                    if trimmed.is_empty() {
                        continue;
                    }
                    if let Some(m) = ASST_PREFIX_RE.find(trimmed) {
                        a_lines.push(trimmed[m.end()..].to_string());
                    } else if let Some(m) = USER_PREFIX_RE.find(trimmed) {
                        u_lines.push(trimmed[m.end()..].to_string());
                    } else {
                        u_lines.push(trimmed.to_string());
                    }
                }
                _ => u_lines.push(dumps_canonical(item)),
            }
        }
        if !u_lines.is_empty() {
            add_seg(
                segments,
                "history_user",
                u_lines.join("\n"),
                Some("user".to_string()),
                Some(key.to_string()),
                turn,
                false,
            );
        }
        if !a_lines.is_empty() {
            add_seg(
                segments,
                "history_assistant",
                a_lines.join("\n"),
                Some("assistant".to_string()),
                Some(key.to_string()),
                turn,
                false,
            );
        }
        return;
    }
    let txt = match val {
        Value::String(s) => s.clone(),
        other => dumps_canonical(other),
    };
    if !txt.trim().is_empty() {
        add_seg(
            segments,
            "history_user",
            txt,
            Some("user".to_string()),
            Some(key.to_string()),
            turn,
            false,
        );
    }
}

const STRUCTURED_FAST_HINTS: &[&str] = &[
    "\"utterance\"",
    "\"user_utterance\"",
    "\"question\"",
    "\"user_question\"",
    "\"query\"",
    "\"user_query\"",
    "\"user_prompt\"",
    "\"prompt\"",
    "\"message\"",
    "\"user_message\"",
    "\"current_turn\"",
    "\"current_input\"",
    "\"task\"",
    "\"problem_statement\"",
    "\"goal\"",
    "\"context\"",
    "\"history\"",
    "\"conversation_history\"",
    "\"chat_history\"",
    "\"prior_turns\"",
    "\"previous_turns\"",
    "\"recent_turns\"",
    "\"turns\"",
    "\"dialogue\"",
    "\"messages\"",
    "\"memory\"",
    "\"conversation\"",
    "\"transcript\"",
    "\"capabilities\"",
    "\"descriptors\"",
    "\"tools\"",
    "\"functions\"",
    "\"available_tools\"",
    "\"tool_definitions\"",
    "\"catalog\"",
    "\"metric_concepts\"",
    "\"tool_results\"",
    "\"observations\"",
    "\"evidence\"",
    "\"search_results\"",
    "\"retrieved_context\"",
    "\"documents\"",
    "\"chunks\"",
    "\"candidates\"",
    "\"frame\"",
    "\"output_schema\"",
    "\"response_schema\"",
    "\"json_schema\"",
    "\"schema\"",
    "\"untrusted_input\"",
    "\"payload\"",
    "\"review_input\"",
];

fn strip_markdown_json_fence(raw: &str) -> &str {
    let s = raw.trim();
    if let Some(after_open) = s.strip_prefix("```") {
        if let Some(nl) = after_open.find('\n') {
            let tag = after_open[..nl].trim();
            if tag.is_empty() || tag.eq_ignore_ascii_case("json") {
                let inner = after_open[nl + 1..].trim();
                if let Some(stripped) = inner.strip_suffix("```") {
                    return stripped.trim();
                }
            }
        }
    }
    s
}

static REMINDER_TAG_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?is)<(?P<tag>system-reminder|system_reminder|environment_context)>(?P<body>.*?)</(?:system-reminder|system_reminder|environment_context)>",
    )
    .unwrap()
});

fn extract_injected_user_reminders(
    segments: &mut Vec<RawSegment>,
    text: &str,
    turn: Option<i64>,
) -> String {
    if !text.contains("<system-reminder>")
        && !text.contains("<system_reminder>")
        && !text.contains("<environment_context>")
    {
        return text.to_string();
    }
    let mut cleaned = String::with_capacity(text.len());
    let mut last_end = 0usize;
    for caps in REMINDER_TAG_RE.captures_iter(text) {
        let Some(m) = caps.get(0) else { continue };
        cleaned.push_str(&text[last_end..m.start()]);
        let tag = caps
            .name("tag")
            .map(|x| x.as_str().to_string())
            .unwrap_or_else(|| "system-reminder".to_string());
        let body = caps
            .name("body")
            .map(|x| x.as_str().trim().to_string())
            .unwrap_or_default();
        if !body.is_empty() {
            add_seg(
                segments,
                "system",
                body,
                Some("system".to_string()),
                Some(tag),
                turn,
                false,
            );
        }
        last_end = m.end();
    }
    cleaned.push_str(&text[last_end..]);
    cleaned.trim().to_string()
}

fn try_decompose_structured_user(
    segments: &mut Vec<RawSegment>,
    text: &str,
    is_current: bool,
    turn: Option<i64>,
) -> (bool, Option<String>) {
    if text.len() < 4 || text.len() > 2_000_000 {
        return (false, None);
    }
    let s = strip_markdown_json_fence(text);
    if s.len() < 4 || !s.starts_with('{') || !s.ends_with('}') {
        return (false, None);
    }
    let sample = if s.len() > 16_384 {
        let mut end = 16_384;
        while !s.is_char_boundary(end) {
            end -= 1;
        }
        &s[..end]
    } else {
        s
    };
    let fast_hit = STRUCTURED_FAST_HINTS.iter().any(|h| sample.contains(h))
        || {
            let low = sample.to_ascii_lowercase();
            STRUCTURED_FAST_HINTS.iter().any(|h| low.contains(h))
        }
        || (s.len() > 16_384 && {
            let low = s.to_ascii_lowercase();
            STRUCTURED_FAST_HINTS.iter().any(|h| low.contains(h))
        });
    if !fast_hit {
        return (false, None);
    }
    let Ok(Value::Object(obj)) = serde_json::from_str::<Value>(s) else {
        return (false, None);
    };
    if obj.is_empty() {
        return (false, None);
    }
    let mut flat: Vec<(String, Value)> = Vec::with_capacity(obj.len() + 4);
    for (k, v) in obj {
        if WRAPPER_KEYS.contains(&k.as_str()) {
            if let Value::Object(inner) = v {
                for (ik, iv) in inner {
                    flat.push((ik, iv));
                }
                continue;
            }
        }
        flat.push((k, v));
    }
    let has_recognized = flat.iter().any(|(k, _)| {
        let kl = k.to_ascii_lowercase();
        let ks = kl.as_str();
        USER_PROMPT_KEYS.contains(&ks)
            || HISTORY_KEYS.contains(&ks)
            || TOOL_DEF_KEYS.contains(&ks)
            || TOOL_RESULT_KEYS.contains(&ks)
            || SCHEMA_KEYS.contains(&ks)
    });
    if !has_recognized {
        return (false, None);
    }
    let mut conv_hint = None;
    for (k, v) in &flat {
        if CONV_ID_KEYS.contains(&k.to_ascii_lowercase().as_str()) {
            let val_s = match v {
                Value::String(st) => st.trim().to_string(),
                Value::Number(n) => n.to_string(),
                _ => String::new(),
            };
            if !val_s.is_empty() {
                conv_hint = Some(val_s);
                break;
            }
        }
    }

    let mut added_user = false;
    let mut rest = Map::new();
    for (k, v) in flat {
        if is_empty_json(&v) {
            continue;
        }
        let kl = k.to_ascii_lowercase();
        let ks = kl.as_str();
        if HISTORY_KEYS.contains(&ks) {
            emit_history_value(segments, &k, &v, turn);
        } else if USER_PROMPT_KEYS.contains(&ks) {
            let val_text = match v {
                Value::String(st) => st,
                other => dumps_canonical(&other),
            };
            let kind = if is_current {
                "user_prompt"
            } else {
                "history_user"
            };
            add_seg(
                segments,
                kind,
                val_text,
                Some("user".to_string()),
                Some(k),
                turn,
                false,
            );
            added_user = true;
        } else if TOOL_DEF_KEYS.contains(&ks) {
            if let Value::Array(ref arr) = v {
                let mut emitted_any = false;
                for item in arr {
                    let nm = item
                        .as_object()
                        .and_then(|o| {
                            o.get("name")
                                .or_else(|| o.get("function").and_then(|f| f.as_object()).and_then(|fo| fo.get("name")))
                        })
                        .and_then(|x| x.as_str())
                        .filter(|s| !s.is_empty())
                        .map(|s| s.to_string());
                    if nm.is_some() {
                        add_seg(
                            segments,
                            "tool_def",
                            dumps_canonical(item),
                            Some("user".to_string()),
                            nm,
                            turn,
                            false,
                        );
                        emitted_any = true;
                    }
                }
                if emitted_any {
                    continue;
                }
            }
            let val_text = match v {
                Value::String(st) => st,
                other => dumps_canonical(&other),
            };
            add_seg(
                segments,
                "tool_def",
                val_text,
                Some("user".to_string()),
                Some(k),
                turn,
                false,
            );
        } else if TOOL_RESULT_KEYS.contains(&ks) {
            if let Value::Object(ref sub_obj) = v {
                if !added_user {
                    for sub_k in ["utterance", "question", "query", "user_query", "goal"] {
                        if let Some(sub_v) = sub_obj.get(sub_k).and_then(|x| x.as_str()) {
                            if !sub_v.trim().is_empty() {
                                let kind = if is_current {
                                    "user_prompt"
                                } else {
                                    "history_user"
                                };
                                add_seg(
                                    segments,
                                    kind,
                                    sub_v.to_string(),
                                    Some("user".to_string()),
                                    Some(format!("{k}.{sub_k}")),
                                    turn,
                                    false,
                                );
                                added_user = true;
                                break;
                            }
                        }
                    }
                }
            }
            let val_text = match v {
                Value::String(st) => st,
                other => dumps_canonical(&other),
            };
            add_seg(
                segments,
                "tool_result",
                val_text,
                Some("tool".to_string()),
                Some(k),
                turn,
                false,
            );
        } else if SCHEMA_KEYS.contains(&ks) {
            let val_text = match v {
                Value::String(st) => st,
                other => dumps_canonical(&other),
            };
            add_seg(
                segments,
                "response_schema",
                val_text,
                None,
                Some(k),
                turn,
                false,
            );
        } else if SYSTEM_KEYS.contains(&ks) {
            let val_text = match v {
                Value::String(st) => st,
                other => dumps_canonical(&other),
            };
            add_seg(
                segments,
                "system",
                val_text,
                Some("system".to_string()),
                Some(k),
                turn,
                false,
            );
        } else {
            rest.insert(k, v);
        }
    }
    if !rest.is_empty() {
        let dumped = dumps_canonical(&Value::Object(rest));
        if !added_user {
            let kind = if is_current {
                "user_prompt"
            } else {
                "history_user"
            };
            add_seg(
                segments,
                kind,
                dumped,
                Some("user".to_string()),
                Some("payload".to_string()),
                turn,
                false,
            );
        } else {
            add_seg(
                segments,
                "other",
                dumped,
                Some("user".to_string()),
                Some("request_metadata".to_string()),
                turn,
                false,
            );
        }
    }
    (true, conv_hint)
}

fn add_message(
    segments: &mut Vec<RawSegment>,
    role: &str,
    content: Option<&Value>,
    is_current: bool,
    turn: Option<i64>,
) -> Option<String> {
    let r = norm_role(Some(role));
    let (text, opaque) = content_text(content);
    let mut conv_hint = None;
    if r == "system" || r == "developer" {
        add_seg(segments, "system", text, Some(r.clone()), None, turn, false);
    } else if r == "user" {
        let user_body = extract_injected_user_reminders(segments, &text, turn);
        let (decomposed, ch) =
            try_decompose_structured_user(segments, &user_body, is_current, turn);
        conv_hint = ch;
        if !decomposed && !user_body.is_empty() {
            let kind = if is_current {
                "user_prompt"
            } else {
                "history_user"
            };
            add_seg(segments, kind, user_body, Some(r.clone()), None, turn, false);
        }
    } else if r == "assistant" {
        add_seg(
            segments,
            "history_assistant",
            text,
            Some(r.clone()),
            None,
            turn,
            false,
        );
    } else {
        add_seg(segments, "other", text, Some(r.clone()), None, turn, false);
    }
    if opaque {
        segments.push(RawSegment {
            kind: "attachment".to_string(),
            text: String::new(),
            role: Some(r),
            name: None,
            turn,
            opaque: true,
            est_tokens: Some(estimate_content_attachment_tokens(content)),
        });
    }
    conv_hint
}

fn parse_openai_chat(body: &Map<String, Value>) -> ParsedRequest {
    let mut segs = Vec::new();
    for sys_key in ["system", "preamble"] {
        if let Some(top_sys) = body.get(sys_key) {
            if top_sys.is_string() || top_sys.is_array() {
                let (txt, _) = content_text(Some(top_sys));
                add_seg(
                    &mut segs,
                    "system",
                    txt,
                    Some("system".to_string()),
                    None,
                    None,
                    false,
                );
            }
        }
    }
    let mut msgs: Vec<std::borrow::Cow<'_, Map<String, Value>>> = body
        .get("messages")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|m| m.as_object().map(std::borrow::Cow::Borrowed))
                .collect()
        })
        .unwrap_or_default();

    if msgs.is_empty() {
        if let Some(ch) = body.get("chat_history").and_then(|v| v.as_array()) {
            for h in ch {
                if let Some(hobj) = h.as_object() {
                    let mut m = Map::new();
                    if let Some(r) = hobj.get("role") {
                        m.insert("role".to_string(), r.clone());
                    }
                    if let Some(c) = hobj
                        .get("message")
                        .or_else(|| hobj.get("content"))
                        .or_else(|| hobj.get("text"))
                    {
                        m.insert("content".to_string(), c.clone());
                    }
                    msgs.push(std::borrow::Cow::Owned(m));
                }
            }
        }
    }
    if let Some(msg_str) = body.get("message").and_then(|v| v.as_str()) {
        if !msg_str.is_empty() {
            let mut m = Map::new();
            m.insert("role".to_string(), Value::String("user".to_string()));
            m.insert("content".to_string(), Value::String(msg_str.to_string()));
            msgs.push(std::borrow::Cow::Owned(m));
        }
    }

    let mut conv_hint = None;
    for k in CONV_ID_KEYS {
        if let Some(v) = body.get(*k) {
            let s = match v {
                Value::String(st) => st.trim().to_string(),
                Value::Number(n) => n.to_string(),
                _ => String::new(),
            };
            if !s.is_empty() {
                conv_hint = Some(s);
                break;
            }
        }
    }

    let last_user: isize = msgs
        .iter()
        .enumerate()
        .filter(|(_, m)| norm_role(m.get("role").and_then(|v| v.as_str())) == "user")
        .map(|(i, _)| i as isize)
        .max()
        .unwrap_or(-1);

    let mut call_id_to_name: HashMap<String, String> = HashMap::new();
    for m in &msgs {
        if let Some(tcs) = m.get("tool_calls").and_then(|v| v.as_array()) {
            for tc in tcs {
                if let Some(tc_obj) = tc.as_object() {
                    let cid = tc_obj.get("id").and_then(|v| v.as_str()).unwrap_or("");
                    let nm = tc_obj
                        .get("function")
                        .and_then(|v| v.as_object())
                        .unwrap_or(tc_obj)
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    if !cid.is_empty() && !nm.is_empty() {
                        call_id_to_name.insert(cid.to_string(), nm.to_string());
                    }
                }
            }
        }
        if let Some(Value::Array(arr)) = m.get("content") {
            for b in arr {
                if let Some(tu) = b
                    .as_object()
                    .and_then(|o| o.get("toolUse"))
                    .and_then(|v| v.as_object())
                {
                    let tid = tu.get("toolUseId").and_then(|v| v.as_str()).unwrap_or("");
                    let nm = tu.get("name").and_then(|v| v.as_str()).unwrap_or("");
                    if !tid.is_empty() && !nm.is_empty() {
                        call_id_to_name.insert(tid.to_string(), nm.to_string());
                    }
                }
            }
        }
    }

    for (i, m) in msgs.iter().enumerate() {
        let role = norm_role(m.get("role").and_then(|v| v.as_str()));
        let content = m.get("content").or_else(|| m.get("message"));
        let turn = Some(i as i64);
        if role == "tool" || role == "function" {
            let (text, opaque) = content_text(content);
            let nm = m
                .get("name")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
                .or_else(|| {
                    m.get("tool_call_id")
                        .and_then(|v| v.as_str())
                        .map(|cid| call_id_to_name.get(cid).cloned().unwrap_or_else(|| cid.to_string()))
                });
            add_seg(&mut segs, "tool_result", text, Some(role), nm, turn, opaque);
            continue;
        }
        if let Some(Value::Array(arr)) = content {
            let has_bedrock_tools = arr.iter().any(|b| {
                b.as_object()
                    .map(|o| o.contains_key("toolUse") || o.contains_key("toolResult"))
                    .unwrap_or(false)
            });
            if has_bedrock_tools {
                for b in arr {
                    let Some(bobj) = b.as_object() else { continue };
                    if let Some(tu) = bobj.get("toolUse").and_then(|v| v.as_object()) {
                        let nm = tu.get("name").and_then(|v| v.as_str()).unwrap_or("");
                        let inp = tu.get("input").unwrap_or(&Value::Null);
                        add_seg(
                            &mut segs,
                            "tool_call",
                            format!("{nm}({})", dumps_canonical(inp)),
                            Some("assistant".to_string()),
                            if nm.is_empty() {
                                None
                            } else {
                                Some(nm.to_string())
                            },
                            turn,
                            false,
                        );
                    } else if let Some(tr) = bobj.get("toolResult").and_then(|v| v.as_object()) {
                        let (t_text, opaque) = content_text(tr.get("content"));
                        let id = tr
                            .get("toolUseId")
                            .and_then(|v| v.as_str())
                            .map(|tid| call_id_to_name.get(tid).cloned().unwrap_or_else(|| tid.to_string()));
                        add_seg(
                            &mut segs,
                            "tool_result",
                            t_text,
                            Some("tool".to_string()),
                            id,
                            turn,
                            opaque,
                        );
                    } else if let Some(txt) = bobj.get("text").and_then(|v| v.as_str()) {
                        let v_str = Value::String(txt.to_string());
                        let ch = add_message(
                            &mut segs,
                            &role,
                            Some(&v_str),
                            (i as isize) == last_user,
                            turn,
                        );
                        if conv_hint.is_none() {
                            conv_hint = ch;
                        }
                    }
                }
            } else {
                let ch = add_message(&mut segs, &role, content, (i as isize) == last_user, turn);
                if conv_hint.is_none() {
                    conv_hint = ch;
                }
            }
        } else {
            let ch = add_message(&mut segs, &role, content, (i as isize) == last_user, turn);
            if conv_hint.is_none() {
                conv_hint = ch;
            }
        }
        if role == "assistant" {
            if let Some(tcs) = m.get("tool_calls").and_then(|v| v.as_array()) {
                for tc in tcs {
                    let Some(tc_obj) = tc.as_object() else {
                        continue;
                    };
                    let fn_obj = tc_obj
                        .get("function")
                        .and_then(|v| v.as_object())
                        .unwrap_or(tc_obj);
                    let nm = fn_obj.get("name").and_then(|v| v.as_str()).unwrap_or("");
                    let args_val = fn_obj.get("arguments").or_else(|| fn_obj.get("parameters"));
                    let args_str = match args_val {
                        Some(Value::String(s)) => s.clone(),
                        Some(other) => dumps_canonical(other),
                        None => "null".to_string(),
                    };
                    add_seg(
                        &mut segs,
                        "tool_call",
                        format!("{nm}({args_str})"),
                        Some("assistant".to_string()),
                        if nm.is_empty() {
                            None
                        } else {
                            Some(nm.to_string())
                        },
                        turn,
                        false,
                    );
                }
            }
            if let Some(fc) = m.get("function_call").and_then(|v| v.as_object()) {
                let nm = fc.get("name").and_then(|v| v.as_str()).unwrap_or("");
                let args = fc.get("arguments").and_then(|v| v.as_str()).unwrap_or("");
                add_seg(
                    &mut segs,
                    "tool_call",
                    format!("{nm}({args})"),
                    Some("assistant".to_string()),
                    if nm.is_empty() {
                        None
                    } else {
                        Some(nm.to_string())
                    },
                    turn,
                    false,
                );
            }
        }
    }

    let mut tools = Vec::new();
    if let Some(t_arr) = body.get("tools").and_then(|v| v.as_array()) {
        for t in t_arr {
            let name = t.as_object().and_then(|tobj| {
                let inner = tobj
                    .get("function")
                    .or_else(|| tobj.get("toolSpec"))
                    .and_then(|v| v.as_object())
                    .unwrap_or(tobj);
                inner
                    .get("name")
                    .or_else(|| tobj.get("type"))
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
            });
            tools.push(name.clone());
            add_seg(&mut segs, "tool_def", dumps_canonical(t), None, name, None, false);
        }
    }
    if let Some(tc_cfg) = body.get("toolConfig").and_then(|v| v.as_object()) {
        if let Some(t_arr) = tc_cfg.get("tools").and_then(|v| v.as_array()) {
            for t in t_arr {
                let name = t.as_object().and_then(|tobj| {
                    let spec = tobj
                        .get("toolSpec")
                        .and_then(|v| v.as_object())
                        .unwrap_or(tobj);
                    spec.get("name")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string())
                });
                tools.push(name.clone());
                add_seg(&mut segs, "tool_def", dumps_canonical(t), None, name, None, false);
            }
        }
    }
    if let Some(f_arr) = body.get("functions").and_then(|v| v.as_array()) {
        for fn_v in f_arr {
            let name = fn_v
                .as_object()
                .and_then(|o| o.get("name"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            tools.push(name.clone());
            add_seg(
                &mut segs,
                "tool_def",
                dumps_canonical(fn_v),
                None,
                name,
                None,
                false,
            );
        }
    }
    let mut step_hint = None;
    if let Some(rf) = body.get("response_format") {
        if let Some(rf_obj) = rf.as_object() {
            let rtype = rf_obj.get("type").and_then(|v| v.as_str()).unwrap_or("");
            if (rtype == "json_schema" || rtype == "json_object")
                && (rf_obj.contains_key("json_schema") || rf_obj.contains_key("schema"))
            {
                let schema_name = rf_obj
                    .get("json_schema")
                    .and_then(|v| v.as_object())
                    .and_then(|o| o.get("name"))
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                step_hint = schema_name.clone();
                add_seg(
                    &mut segs,
                    "response_schema",
                    dumps_canonical(rf),
                    None,
                    schema_name,
                    None,
                    false,
                );
            }
        }
    }
    let body_val = Value::Object(body.clone());
    ParsedRequest {
        provider: OPENAI_CHAT.to_string(),
        model: body
            .get("model")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        stream: body.get("stream").and_then(|v| v.as_bool()).unwrap_or(false),
        segments: segs,
        tools,
        conv_hint,
        step_hint,
        chained_context: false,
        cache_breakpoints: count_cache_breakpoints(&body_val),
    }
}

fn parse_openai_responses(body: &Map<String, Value>) -> ParsedRequest {
    let mut segs = Vec::new();
    if let Some(inst) = body.get("instructions").and_then(|v| v.as_str()) {
        add_seg(
            &mut segs,
            "system",
            inst.to_string(),
            Some("system".to_string()),
            None,
            None,
            false,
        );
    }
    if let Some(inp) = body.get("input") {
        match inp {
            Value::String(s) => {
                add_seg(
                    &mut segs,
                    "user_prompt",
                    s.clone(),
                    Some("user".to_string()),
                    None,
                    Some(0),
                    false,
                );
            }
            Value::Array(arr) => {
                let items: Vec<&Map<String, Value>> =
                    arr.iter().filter_map(|v| v.as_object()).collect();
                let mut call_id_to_name: HashMap<String, String> = HashMap::new();
                for it in &items {
                    let t = it.get("type").and_then(|v| v.as_str()).unwrap_or("");
                    if matches!(t, "function_call" | "custom_tool_call" | "mcp_call") {
                        let cid = it
                            .get("call_id")
                            .or_else(|| it.get("id"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("");
                        let nm = it.get("name").and_then(|v| v.as_str()).unwrap_or(t);
                        if !cid.is_empty() && !nm.is_empty() {
                            call_id_to_name.insert(cid.to_string(), nm.to_string());
                        }
                    } else if matches!(
                        t,
                        "computer_call" | "local_shell_call" | "web_search_call" | "file_search_call"
                    ) {
                        if let Some(cid) = it.get("call_id").or_else(|| it.get("id")).and_then(|v| v.as_str()) {
                            call_id_to_name.insert(cid.to_string(), t.to_string());
                        }
                    }
                }
                let last_user: isize = items
                    .iter()
                    .enumerate()
                    .filter(|(_, it)| it.get("role").and_then(|v| v.as_str()) == Some("user"))
                    .map(|(i, _)| i as isize)
                    .max()
                    .unwrap_or(-1);
                for (i, it) in items.iter().enumerate() {
                    let t = it.get("type").and_then(|v| v.as_str()).unwrap_or("");
                    let turn = Some(i as i64);
                    if matches!(t, "function_call" | "custom_tool_call" | "mcp_call") {
                        let nm = it.get("name").and_then(|v| v.as_str()).unwrap_or(t);
                        let args_str = match it.get("arguments").or_else(|| it.get("input")) {
                            Some(Value::String(s)) => s.clone(),
                            Some(other) => dumps_canonical(other),
                            None => String::new(),
                        };
                        add_seg(
                            &mut segs,
                            "tool_call",
                            format!("{nm}({args_str})"),
                            Some("assistant".to_string()),
                            if nm.is_empty() {
                                None
                            } else {
                                Some(nm.to_string())
                            },
                            turn,
                            false,
                        );
                    } else if matches!(
                        t,
                        "web_search_call"
                            | "file_search_call"
                            | "computer_call"
                            | "code_interpreter_call"
                            | "local_shell_call"
                    ) {
                        add_seg(
                            &mut segs,
                            "tool_call",
                            dumps_canonical(&Value::Object((*it).clone())),
                            Some("assistant".to_string()),
                            Some(t.to_string()),
                            turn,
                            false,
                        );
                    } else if matches!(
                        t,
                        "function_call_output"
                            | "custom_tool_call_output"
                            | "computer_call_output"
                            | "local_shell_call_output"
                    ) {
                        let out_v = it.get("output");
                        let txt = match out_v {
                            Some(Value::String(s)) => s.clone(),
                            Some(other) => dumps_canonical(other),
                            None => "null".to_string(),
                        };
                        let resolved_name = it
                            .get("call_id")
                            .and_then(|v| v.as_str())
                            .map(|cid| call_id_to_name.get(cid).cloned().unwrap_or_else(|| cid.to_string()))
                            .or_else(|| Some(t.to_string()));
                        add_seg(&mut segs, "tool_result", txt, None, resolved_name, turn, false);
                    } else if t == "mcp_list_tools" {
                        let srv = it
                            .get("server_label")
                            .and_then(|v| v.as_str())
                            .unwrap_or("mcp")
                            .to_string();
                        add_seg(
                            &mut segs,
                            "tool_def",
                            dumps_canonical(&Value::Object((*it).clone())),
                            None,
                            Some(srv),
                            turn,
                            false,
                        );
                    } else if t == "reasoning" {
                        let sum_v = it
                            .get("summary")
                            .cloned()
                            .unwrap_or_else(|| Value::String(String::new()));
                        add_seg(
                            &mut segs,
                            "other",
                            dumps_canonical(&sum_v),
                            Some("assistant".to_string()),
                            Some("reasoning".to_string()),
                            turn,
                            false,
                        );
                    } else if let Some(role) = it.get("role").and_then(|v| v.as_str()) {
                        add_message(
                            &mut segs,
                            role,
                            it.get("content"),
                            (i as isize) == last_user,
                            turn,
                        );
                    } else {
                        add_seg(
                            &mut segs,
                            "other",
                            dumps_canonical(&Value::Object((*it).clone())),
                            None,
                            None,
                            turn,
                            false,
                        );
                    }
                }
            }
            _ => {}
        }
    }
    let mut tools = Vec::new();
    if let Some(t_arr) = body.get("tools").and_then(|v| v.as_array()) {
        for t in t_arr {
            let name = t.as_object().and_then(|o| {
                o.get("name")
                    .or_else(|| o.get("type"))
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
            });
            tools.push(name.clone());
            add_seg(&mut segs, "tool_def", dumps_canonical(t), None, name, None, false);
        }
    }
    if let Some(fmt) = body
        .get("text")
        .and_then(|v| v.as_object())
        .and_then(|o| o.get("format"))
    {
        if fmt
            .as_object()
            .and_then(|o| o.get("type"))
            .and_then(|v| v.as_str())
            == Some("json_schema")
        {
            add_seg(
                &mut segs,
                "response_schema",
                dumps_canonical(fmt),
                None,
                None,
                None,
                false,
            );
        }
    }
    let prev_resp = body
        .get("previous_response_id")
        .and_then(|v| v.as_str())
        .filter(|s| !s.trim().is_empty())
        .map(|s| s.to_string());
    let conv_field = body
        .get("conversation")
        .and_then(|v| match v {
            Value::String(s) if !s.trim().is_empty() => Some(s.clone()),
            Value::Object(o) => o
                .get("id")
                .and_then(|iv| iv.as_str())
                .filter(|s| !s.trim().is_empty())
                .map(|s| s.to_string()),
            _ => None,
        });
    let chained = prev_resp.is_some() || conv_field.is_some();
    ParsedRequest {
        provider: OPENAI_RESPONSES.to_string(),
        model: body
            .get("model")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        stream: body.get("stream").and_then(|v| v.as_bool()).unwrap_or(false),
        segments: segs,
        tools,
        conv_hint: conv_field.or(prev_resp),
        step_hint: None,
        chained_context: chained,
        cache_breakpoints: 0,
    }
}

fn parse_anthropic(body: &Map<String, Value>) -> ParsedRequest {
    let mut segs = Vec::new();
    if let Some(sys) = body.get("system") {
        if sys.is_string() || sys.is_array() {
            let (txt, _) = content_text(Some(sys));
            add_seg(
                &mut segs,
                "system",
                txt,
                Some("system".to_string()),
                None,
                None,
                false,
            );
        }
    }
    let msgs: Vec<&Map<String, Value>> = body
        .get("messages")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|m| m.as_object()).collect())
        .unwrap_or_default();

    let has_text = |m: &Map<String, Value>| -> bool {
        match m.get("content") {
            Some(Value::String(s)) => !s.is_empty(),
            Some(Value::Array(arr)) => arr.iter().any(|b| {
                b.as_object()
                    .and_then(|o| o.get("type"))
                    .and_then(|v| v.as_str())
                    == Some("text")
            }),
            _ => false,
        }
    };
    let last_user: isize = msgs
        .iter()
        .enumerate()
        .filter(|(_, m)| m.get("role").and_then(|v| v.as_str()) == Some("user") && has_text(m))
        .map(|(i, _)| i as isize)
        .max()
        .unwrap_or(-1);

    let mut call_id_to_name: HashMap<String, String> = HashMap::new();
    for m in &msgs {
        if let Some(Value::Array(arr)) = m.get("content") {
            for b in arr {
                if let Some(bobj) = b.as_object() {
                    let t = bobj.get("type").and_then(|v| v.as_str()).unwrap_or("");
                    if matches!(t, "tool_use" | "server_tool_use" | "mcp_tool_use") {
                        let tid = bobj.get("id").and_then(|v| v.as_str()).unwrap_or("");
                        let nm = bobj.get("name").and_then(|v| v.as_str()).unwrap_or(t);
                        if !tid.is_empty() && !nm.is_empty() {
                            call_id_to_name.insert(tid.to_string(), nm.to_string());
                        }
                    }
                }
            }
        }
    }

    for (i, m) in msgs.iter().enumerate() {
        let role = m.get("role").and_then(|v| v.as_str()).unwrap_or("");
        let turn = Some(i as i64);
        match m.get("content") {
            Some(c @ Value::String(_)) => {
                add_message(&mut segs, role, Some(c), (i as isize) == last_user, turn);
            }
            Some(Value::Array(arr)) => {
                for b in arr {
                    let Some(bobj) = b.as_object() else { continue };
                    let t = bobj.get("type").and_then(|v| v.as_str()).unwrap_or("");
                    if t == "text" {
                        let txt = Value::String(
                            bobj.get("text")
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string(),
                        );
                        add_message(
                            &mut segs,
                            role,
                            Some(&txt),
                            (i as isize) == last_user && role == "user",
                            turn,
                        );
                    } else if matches!(t, "tool_use" | "server_tool_use" | "mcp_tool_use") {
                        let nm = bobj.get("name").and_then(|v| v.as_str()).unwrap_or(t);
                        let inp = bobj.get("input").unwrap_or(&Value::Null);
                        add_seg(
                            &mut segs,
                            "tool_call",
                            format!("{nm}({})", dumps_canonical(inp)),
                            Some("assistant".to_string()),
                            if nm.is_empty() {
                                None
                            } else {
                                Some(nm.to_string())
                            },
                            turn,
                            false,
                        );
                    } else if matches!(
                        t,
                        "tool_result"
                            | "web_search_tool_result"
                            | "code_execution_tool_result"
                            | "bash_code_execution_tool_result"
                            | "text_editor_code_execution_tool_result"
                            | "mcp_tool_result"
                    ) {
                        let (mut txt, opaque) = content_text(bobj.get("content"));
                        if txt.is_empty() && bobj.get("content").is_some() && !opaque {
                            txt = dumps_canonical(bobj.get("content").unwrap_or(&Value::Null));
                        }
                        let resolved_name = bobj
                            .get("tool_use_id")
                            .and_then(|v| v.as_str())
                            .map(|tid| call_id_to_name.get(tid).cloned().unwrap_or_else(|| tid.to_string()))
                            .or_else(|| Some(t.to_string()));
                        add_seg(&mut segs, "tool_result", txt, None, resolved_name, turn, opaque);
                    } else if t == "thinking" || t == "redacted_thinking" {
                        let th = bobj
                            .get("thinking")
                            .or_else(|| bobj.get("data"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        add_seg(
                            &mut segs,
                            "other",
                            th,
                            Some("assistant".to_string()),
                            Some("thinking".to_string()),
                            turn,
                            false,
                        );
                    } else if t == "document" {
                        let src = bobj.get("source").and_then(|v| v.as_object());
                        if let Some(sobj) = src {
                            if sobj.get("type").and_then(|v| v.as_str()) == Some("text") {
                                if let Some(data) = sobj.get("data").and_then(|v| v.as_str()) {
                                    let v_str = Value::String(data.to_string());
                                    add_message(
                                        &mut segs,
                                        role,
                                        Some(&v_str),
                                        (i as isize) == last_user && role == "user",
                                        turn,
                                    );
                                    continue;
                                }
                            }
                        }
                        segs.push(RawSegment {
                            kind: "attachment".to_string(),
                            text: String::new(),
                            role: Some(role.to_string()),
                            name: Some("document".to_string()),
                            turn,
                            opaque: true,
                            est_tokens: Some(estimate_part_attachment_tokens(bobj)),
                        });
                    } else if OPAQUE_PARTS.contains(&t) {
                        segs.push(RawSegment {
                            kind: "attachment".to_string(),
                            text: String::new(),
                            role: Some(role.to_string()),
                            name: Some(t.to_string()),
                            turn,
                            opaque: true,
                            est_tokens: Some(estimate_part_attachment_tokens(bobj)),
                        });
                    }
                }
            }
            _ => {}
        }
    }
    let mut tools = Vec::new();
    if let Some(t_arr) = body.get("tools").and_then(|v| v.as_array()) {
        for t in t_arr {
            let name = t.as_object().and_then(|o| {
                o.get("name")
                    .or_else(|| o.get("type"))
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
            });
            tools.push(name.clone());
            add_seg(&mut segs, "tool_def", dumps_canonical(t), None, name, None, false);
        }
    }
    let body_val = Value::Object(body.clone());
    ParsedRequest {
        provider: ANTHROPIC.to_string(),
        model: body
            .get("model")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        stream: body.get("stream").and_then(|v| v.as_bool()).unwrap_or(false),
        segments: segs,
        tools,
        conv_hint: None,
        step_hint: None,
        chained_context: false,
        cache_breakpoints: count_cache_breakpoints(&body_val),
    }
}

static TEMPLATES: Lazy<Vec<(Regex, Regex)>> = Lazy::new(|| {
    vec![
        (
            Regex::new(r"<\|im_start\|>(system|user|assistant|tool)\n").unwrap(),
            Regex::new(r"<\|im_end\|>").unwrap(),
        ),
        (
            Regex::new(r"<\|start_header_id\|>(system|user|assistant|ipython)<\|end_header_id\|>\n\n")
                .unwrap(),
            Regex::new(r"<\|eot_id\|>").unwrap(),
        ),
        (
            Regex::new(r"<start_of_turn>(user|model)\n").unwrap(),
            Regex::new(r"<end_of_turn>").unwrap(),
        ),
        (
            Regex::new(r"(?i)<｜(System|User|Assistant)｜>\s*").unwrap(),
            Regex::new(r"(?i)<｜end[▁_]of[▁_]sentence｜>").unwrap(),
        ),
        (
            Regex::new(r"(?i)<\|(system|user|assistant|tool)\|>\s*\n?").unwrap(),
            Regex::new(r"(?i)<\|(?:end|eot_id|end_of_sentence)\|>").unwrap(),
        ),
        (
            Regex::new(r"(?i)<\|START_OF_TURN_TOKEN\|><\|(SYSTEM|USER|CHATBOT|TOOL)_TOKEN\|>")
                .unwrap(),
            Regex::new(r"(?i)<\|END_OF_TURN_TOKEN\|>").unwrap(),
        ),
    ]
});

static LLAMA2_SYS_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?s)<<SYS>>(.*?)<</SYS>>").unwrap());
static INST_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?s)\[INST\](.*?)\[/INST\]").unwrap());

pub fn split_chat_template(prompt: &str) -> Option<Vec<(String, String)>> {
    if prompt.is_empty() {
        return None;
    }
    if !prompt.contains('<') && !prompt.contains("[INST]") {
        return None;
    }
    if !prompt.contains("<|")
        && !prompt.contains("<start_of_turn>")
        && !prompt.contains("<｜")
        && !prompt.contains("[INST]")
    {
        return None;
    }
    for (header_re, end_re) in TEMPLATES.iter() {
        let heads: Vec<_> = header_re.captures_iter(prompt).collect();
        if heads.is_empty() {
            continue;
        }
        let mut turns = Vec::new();
        for (i, cap) in heads.iter().enumerate() {
            let m0 = cap.get(0)?;
            let stop = if i + 1 < heads.len() {
                heads[i + 1].get(0)?.start()
            } else {
                prompt.len()
            };
            let mut chunk = &prompt[m0.end()..stop];
            if let Some(em) = end_re.find(chunk) {
                chunk = &chunk[..em.start()];
            }
            let raw_role = cap.get(1)?.as_str().to_ascii_lowercase();
            let role = match raw_role.as_str() {
                "model" | "chatbot" => "assistant".to_string(),
                "ipython" => "tool".to_string(),
                other => other.to_string(),
            };
            if !chunk.trim().is_empty() {
                turns.push((role, chunk.trim_matches('\n').to_string()));
            }
        }
        if !turns.is_empty() {
            return Some(turns);
        }
    }
    if prompt.contains("[INST]") {
        let mut turns = Vec::new();
        let mut pos = 0usize;
        if let Some(sys_m) = LLAMA2_SYS_RE.captures(prompt) {
            if let Some(g1) = sys_m.get(1) {
                turns.push(("system".to_string(), g1.as_str().trim().to_string()));
            }
        }
        for cap in INST_RE.captures_iter(prompt) {
            let Some(m0) = cap.get(0) else { continue };
            if m0.start() > pos && pos > 0 {
                let reply = prompt[pos..m0.start()]
                    .replace("</s>", "")
                    .replace("<s>", "");
                let reply_t = reply.trim();
                if !reply_t.is_empty() {
                    turns.push(("assistant".to_string(), reply_t.to_string()));
                }
            }
            if let Some(g1) = cap.get(1) {
                let user_clean = LLAMA2_SYS_RE.replace_all(g1.as_str(), "");
                let user_t = user_clean.trim();
                if !user_t.is_empty() {
                    turns.push(("user".to_string(), user_t.to_string()));
                }
            }
            pos = m0.end();
        }
        if pos > 0 && pos < prompt.len() {
            let tail = prompt[pos..].replace("</s>", "");
            let tail_t = tail.trim();
            if !tail_t.is_empty() {
                turns.push(("assistant".to_string(), tail_t.to_string()));
            }
        }
        if !turns.is_empty() {
            return Some(turns);
        }
    }
    None
}

fn parse_completions(body: &Map<String, Value>) -> ParsedRequest {
    let mut segs = Vec::new();
    if let Some(sys) = body.get("system").and_then(|v| v.as_str()) {
        add_seg(
            &mut segs,
            "system",
            sys.to_string(),
            Some("system".to_string()),
            None,
            None,
            false,
        );
    }
    let prompt_val = body
        .get("prompt")
        .or_else(|| body.get("inputs"))
        .or_else(|| body.get("inputText"));
    let mut prompt_str: Option<String> = None;
    match prompt_val {
        Some(Value::String(s)) => prompt_str = Some(s.clone()),
        Some(Value::Array(arr)) => {
            if !arr.is_empty() && arr.iter().all(|x| x.is_string()) {
                let joined: Vec<&str> = arr.iter().filter_map(|x| x.as_str()).collect();
                prompt_str = Some(joined.join("\n"));
            } else {
                segs.push(RawSegment {
                    kind: "raw_prompt".to_string(),
                    text: String::new(),
                    role: None,
                    name: None,
                    turn: None,
                    opaque: true,
                    est_tokens: Some(arr.len() as i64),
                });
            }
        }
        _ => {}
    }
    if let Some(p) = prompt_str {
        if !p.is_empty() {
            if let Some(turns) = split_chat_template(&p) {
                let last_user: isize = turns
                    .iter()
                    .enumerate()
                    .filter(|(_, (r, _))| r == "user")
                    .map(|(i, _)| i as isize)
                    .max()
                    .unwrap_or(-1);
                for (i, (role, txt)) in turns.into_iter().enumerate() {
                    let eff_role = if role == "tool" { "other" } else { &role };
                    let v_str = Value::String(txt);
                    add_message(
                        &mut segs,
                        eff_role,
                        Some(&v_str),
                        (i as isize) == last_user,
                        Some(i as i64),
                    );
                }
            } else {
                add_seg(&mut segs, "raw_prompt", p, None, None, None, false);
            }
        }
    }
    let stream = body
        .get("stream")
        .and_then(|v| v.as_bool())
        .or_else(|| {
            body.get("parameters")
                .and_then(|v| v.as_object())
                .and_then(|o| o.get("stream"))
                .and_then(|v| v.as_bool())
        })
        .unwrap_or(false);
    ParsedRequest {
        provider: COMPLETIONS.to_string(),
        model: body
            .get("model")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        stream,
        segments: segs,
        tools: Vec::new(),
        conv_hint: None,
        step_hint: None,
        chained_context: false,
        cache_breakpoints: 0,
    }
}

fn parts_text(parts: Option<&Value>) -> (String, bool) {
    let Some(Value::Array(arr)) = parts else {
        return (String::new(), false);
    };
    let mut texts = Vec::new();
    let mut opaque = false;
    for p in arr {
        match p {
            Value::String(s) => texts.push(s.clone()),
            Value::Object(o) => {
                if let Some(t) = o.get("text").and_then(|v| v.as_str()) {
                    texts.push(t.to_string());
                } else if o.contains_key("inlineData")
                    || o.contains_key("fileData")
                    || o.contains_key("inline_data")
                    || o.contains_key("file_data")
                {
                    opaque = true;
                }
            }
            _ => {}
        }
    }
    (texts.join("\n"), opaque)
}

fn parse_gemini(body: &Map<String, Value>) -> ParsedRequest {
    let mut segs = Vec::new();
    if let Some(si) = body
        .get("systemInstruction")
        .or_else(|| body.get("system_instruction"))
    {
        match si {
            Value::String(s) => add_seg(
                &mut segs,
                "system",
                s.clone(),
                Some("system".to_string()),
                None,
                None,
                false,
            ),
            Value::Object(o) => {
                let (txt, _) = parts_text(o.get("parts"));
                add_seg(
                    &mut segs,
                    "system",
                    txt,
                    Some("system".to_string()),
                    None,
                    None,
                    false,
                );
            }
            _ => {}
        }
    }
    let contents: Vec<&Map<String, Value>> = body
        .get("contents")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|c| c.as_object()).collect())
        .unwrap_or_default();

    let last_user: isize = contents
        .iter()
        .enumerate()
        .filter(|(_, c)| {
            let role = c.get("role").and_then(|v| v.as_str()).unwrap_or("user");
            if role != "user" {
                return false;
            }
            let has_fr = c
                .get("parts")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter().any(|p| {
                        p.as_object()
                            .map(|o| {
                                o.contains_key("functionResponse")
                                    || o.contains_key("function_response")
                            })
                            .unwrap_or(false)
                    })
                })
                .unwrap_or(false);
            !has_fr
        })
        .map(|(i, _)| i as isize)
        .max()
        .unwrap_or(-1);

    for (i, c) in contents.iter().enumerate() {
        let role = c.get("role").and_then(|v| v.as_str()).unwrap_or("user");
        let turn = Some(i as i64);
        if let Some(parts) = c.get("parts").and_then(|v| v.as_array()) {
            for part in parts {
                let Some(pobj) = part.as_object() else {
                    continue;
                };
                if let Some(fc) = pobj
                    .get("functionCall")
                    .or_else(|| pobj.get("function_call"))
                    .and_then(|v| v.as_object())
                {
                    let nm = fc.get("name").and_then(|v| v.as_str()).unwrap_or("");
                    let args = fc.get("args").unwrap_or(&Value::Null);
                    add_seg(
                        &mut segs,
                        "tool_call",
                        format!("{nm}({})", dumps_canonical(args)),
                        Some("assistant".to_string()),
                        if nm.is_empty() {
                            None
                        } else {
                            Some(nm.to_string())
                        },
                        turn,
                        false,
                    );
                } else if let Some(fr) = pobj
                    .get("functionResponse")
                    .or_else(|| pobj.get("function_response"))
                    .and_then(|v| v.as_object())
                {
                    let nm = fr
                        .get("name")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                    let resp = fr.get("response").unwrap_or(&Value::Null);
                    add_seg(
                        &mut segs,
                        "tool_result",
                        dumps_canonical(resp),
                        None,
                        nm,
                        turn,
                        false,
                    );
                } else if let Some(ec) = pobj
                    .get("executableCode")
                    .or_else(|| pobj.get("executable_code"))
                    .and_then(|v| v.as_object())
                {
                    let code = ec.get("code").and_then(|v| v.as_str()).unwrap_or("");
                    add_seg(
                        &mut segs,
                        "tool_call",
                        code.to_string(),
                        Some("assistant".to_string()),
                        Some("code_execution".to_string()),
                        turn,
                        false,
                    );
                } else if let Some(cer) = pobj
                    .get("codeExecutionResult")
                    .or_else(|| pobj.get("code_execution_result"))
                    .and_then(|v| v.as_object())
                {
                    let out_s = cer.get("output").and_then(|v| v.as_str()).unwrap_or("");
                    add_seg(
                        &mut segs,
                        "tool_result",
                        out_s.to_string(),
                        None,
                        Some("code_execution".to_string()),
                        turn,
                        false,
                    );
                } else {
                    let single_arr = Value::Array(vec![part.clone()]);
                    let (txt, opaque) = parts_text(Some(&single_arr));
                    let eff_role = if role == "model" { "assistant" } else { role };
                    let v_str = Value::String(txt);
                    add_message(
                        &mut segs,
                        eff_role,
                        Some(&v_str),
                        (i as isize) == last_user,
                        turn,
                    );
                    if opaque {
                        segs.push(RawSegment {
                            kind: "attachment".to_string(),
                            text: String::new(),
                            role: Some(role.to_string()),
                            name: None,
                            turn,
                            opaque: true,
                            est_tokens: Some(estimate_part_attachment_tokens(pobj)),
                        });
                    }
                }
            }
        }
    }
    let mut tools = Vec::new();
    if let Some(t_arr) = body.get("tools").and_then(|v| v.as_array()) {
        for t in t_arr {
            let Some(tobj) = t.as_object() else { continue };
            if let Some(decls) = tobj
                .get("functionDeclarations")
                .or_else(|| tobj.get("function_declarations"))
                .and_then(|v| v.as_array())
            {
                for d in decls {
                    let nm = d
                        .as_object()
                        .and_then(|o| o.get("name"))
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                    tools.push(nm.clone());
                    add_seg(&mut segs, "tool_def", dumps_canonical(d), None, nm, None, false);
                }
            } else {
                let nm = tobj.keys().next().cloned().or_else(|| Some("tool".to_string()));
                tools.push(nm.clone());
                add_seg(&mut segs, "tool_def", dumps_canonical(t), None, nm, None, false);
            }
        }
    }
    if let Some(gc) = body
        .get("generationConfig")
        .or_else(|| body.get("generation_config"))
        .and_then(|v| v.as_object())
    {
        if let Some(rs) = gc
            .get("responseSchema")
            .or_else(|| gc.get("response_schema"))
        {
            add_seg(
                &mut segs,
                "response_schema",
                dumps_canonical(rs),
                None,
                None,
                None,
                false,
            );
        }
    }
    let has_cached_content = body
        .get("cachedContent")
        .or_else(|| body.get("cached_content"))
        .and_then(|v| v.as_str())
        .map(|s| !s.is_empty())
        .unwrap_or(false);
    ParsedRequest {
        provider: GEMINI.to_string(),
        model: body
            .get("model")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        stream: body.get("stream").and_then(|v| v.as_bool()).unwrap_or(false),
        segments: segs,
        tools,
        conv_hint: None,
        step_hint: None,
        chained_context: has_cached_content,
        cache_breakpoints: usize::from(has_cached_content),
    }
}

fn parse_generic(body: &Map<String, Value>) -> ParsedRequest {
    if body.contains_key("anthropic_version") {
        return parse_anthropic(body);
    }
    if body.contains_key("contents") {
        return parse_gemini(body);
    }
    if body.contains_key("messages") || body.contains_key("chat_history") || body.contains_key("message") {
        return parse_openai_chat(body);
    }
    if body.contains_key("input") && !body.contains_key("prompt") {
        return parse_openai_responses(body);
    }
    parse_completions(body)
}

pub fn detect_provider(path: &str, body: Option<&Value>) -> Option<String> {
    use crate::detect::{normalize_path, path_hint, Detector, EXCLUDED_PATH_RE};

    let npath = normalize_path(path);
    if let Some(hint) = path_hint(&npath) {
        if matches!(hint, OPENAI_CHAT | OPENAI_RESPONSES | ANTHROPIC)
            && !EXCLUDED_PATH_RE.is_match(&npath)
        {
            return Some(hint.to_string());
        }
    }
    Detector::default().inspect("POST", "", 0, &npath, body).api
}

pub fn parse_request(provider: &str, body: &Value) -> Result<ParsedRequest, String> {
    let empty = Map::new();
    let obj = body.as_object().unwrap_or(&empty);
    match provider {
        OPENAI_CHAT => Ok(parse_openai_chat(obj)),
        OPENAI_RESPONSES => Ok(parse_openai_responses(obj)),
        ANTHROPIC => Ok(parse_anthropic(obj)),
        COMPLETIONS => Ok(parse_completions(obj)),
        GEMINI => Ok(parse_gemini(obj)),
        "generic" | "bedrock" => Ok(parse_generic(obj)),
        other => Err(format!("unknown provider: {other}")),
    }
}
