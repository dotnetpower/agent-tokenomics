use crate::counting::TokenCounter;
use crate::model::{CallRecord, RawSegment, Segment, SCHEMA_VERSION};
use crate::parse_request::ParsedRequest;
use crate::parse_response::ParsedResponse;
use hmac::{Hmac, Mac};
use once_cell::sync::Lazy;
use rand::RngCore;
use regex::Regex;
use sha2::Sha256;
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

type HmacSha256 = Hmac<Sha256>;

pub const MIN_SECTION_CHARS: usize = 200;
pub const MAX_SEGMENT_TEXT_CHARS: usize = 32_768;
pub const MAX_SECTION_SPLIT_CHARS: usize = 524_288;

static BOUNDARY_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?m)^(?:#{1,3}[ \t]+(?P<h>.+?)|<(?P<t>[A-Za-z_][\w:-]*)>[ \t]*)$").unwrap()
});

static SECRET_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?i)(?:Bearer\s+[A-Za-z0-9._\-]{16,}|sk-(?:proj-|ant-|svcacct-)?[A-Za-z0-9_\-]{20,}|gh[pousr]_[A-Za-z0-9]{20,}|github_pat_[A-Za-z0-9_]{22,}|AKIA[0-9A-Z]{16}|AIza[0-9A-Za-z_\-]{35}|xox[baprs]-[0-9A-Za-z\-]{10,}|eyJ[A-Za-z0-9_\-]{16,}\.eyJ[A-Za-z0-9_\-]{16,}\.[A-Za-z0-9_\-]{16,})",
    )
    .unwrap()
});

const SECRET_MARKERS: &[&str] = &[
    "Bearer",
    "bearer",
    "BEARER",
    "sk-",
    "ghp_",
    "gho_",
    "ghu_",
    "ghs_",
    "ghr_",
    "github_pat_",
    "AKIA",
    "AIza",
    "xox",
    "eyJ",
];

pub fn safe_char_prefix(s: &str, max_chars: usize) -> (&str, usize) {
    let mut total_chars = 0usize;
    let mut byte_idx = s.len();
    for (i, (b_idx, _)) in s.char_indices().enumerate() {
        total_chars = i + 1;
        if i == max_chars {
            byte_idx = b_idx;
        }
    }
    if total_chars <= max_chars {
        (s, total_chars)
    } else {
        (&s[..byte_idx], total_chars)
    }
}

pub fn redact_secrets(text: &str) -> String {
    if text.is_empty() {
        return String::new();
    }
    let (work_slice, orig_chars) = safe_char_prefix(text, MAX_SEGMENT_TEXT_CHARS + 256);
    let redacted = if SECRET_MARKERS.iter().any(|m| work_slice.contains(m)) {
        SECRET_RE
            .replace_all(work_slice, "***REDACTED***")
            .into_owned()
    } else {
        work_slice.to_string()
    };
    let (final_slice, redacted_chars) = safe_char_prefix(&redacted, MAX_SEGMENT_TEXT_CHARS);
    if orig_chars > MAX_SEGMENT_TEXT_CHARS || redacted_chars > MAX_SEGMENT_TEXT_CHARS {
        format!(
            "{}\n... [truncated, {} chars total]",
            final_slice,
            format_with_commas(orig_chars)
        )
    } else {
        redacted
    }
}

fn format_with_commas(n: usize) -> String {
    let s = n.to_string();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (i, ch) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    out.chars().rev().collect()
}

pub fn hash_text(text: &str, salt: &[u8]) -> String {
    if text.is_empty() {
        return String::new();
    }
    let key = if salt.is_empty() { b"tokprof" } else { salt };
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC accepts any key size");
    mac.update(text.as_bytes());
    let result = mac.finalize().into_bytes();
    hex::encode(&result[..8])
}

pub fn split_sections(text: &str, min_chars: usize) -> Vec<(Option<String>, String)> {
    if text.is_empty()
        || text.len() < min_chars
        || text.len() > MAX_SECTION_SPLIT_CHARS
        || (!text.contains('#') && !text.contains('<'))
    {
        return vec![(None, text.to_string())];
    }
    let mut starts = Vec::new();
    let mut names = Vec::new();
    for caps in BOUNDARY_RE.captures_iter(text) {
        if let Some(m) = caps.get(0) {
            starts.push(m.start());
            let nm = caps
                .name("h")
                .or_else(|| caps.name("t"))
                .map(|x| x.as_str().trim().to_string())
                .unwrap_or_default();
            names.push(nm);
        }
    }
    if starts.len() < 2 && !(starts.len() == 1 && starts[0] > 0) {
        return vec![(None, text.to_string())];
    }
    let mut pieces: Vec<(String, String)> = Vec::new();
    if !starts.is_empty() && starts[0] > 0 {
        pieces.push(("(preamble)".to_string(), text[..starts[0]].to_string()));
    }
    for (i, &s) in starts.iter().enumerate() {
        let end = if i + 1 < starts.len() {
            starts[i + 1]
        } else {
            text.len()
        };
        pieces.push((names[i].clone(), text[s..end].to_string()));
    }
    let mut merged: Vec<(String, String)> = Vec::new();
    for (name, body) in pieces {
        let body_chars = body.chars().count();
        if !merged.is_empty() && body_chars < min_chars {
            if let Some(last) = merged.last_mut() {
                last.1.push_str(&body);
            }
        } else {
            merged.push((name, body));
        }
    }
    if merged.len() > 1 && merged[0].1.chars().count() < min_chars {
        let first = merged.remove(0);
        merged[0].1 = format!("{}{}", first.1, merged[0].1);
    }
    if merged.len() < 2 {
        return vec![(None, text.to_string())];
    }
    merged.into_iter().map(|(n, b)| (Some(n), b)).collect()
}

fn split_components(seg: &RawSegment, components: &HashMap<String, String>) -> Vec<RawSegment> {
    if components.is_empty()
        || seg.opaque
        || (seg.kind != "system" && seg.kind != "other")
        || seg.text.is_empty()
    {
        return vec![seg.clone()];
    }
    let mut sorted_comp: Vec<(&String, &String)> = components.iter().collect();
    sorted_comp.sort_by_key(|b| std::cmp::Reverse(b.1.len()));

    let mut parts = vec![seg.clone()];
    for (name, ctext_raw) in sorted_comp {
        let ctext = ctext_raw.trim();
        if ctext.is_empty() {
            continue;
        }
        let mut nxt = Vec::new();
        for p in parts {
            if (p.kind == "system" || p.kind == "other")
                && !p.opaque
                && p.name.as_deref() != Some(name.as_str())
                && p.text.contains(ctext)
                && nxt.len() < 64
            {
                if let Some(pos) = p.text.find(ctext) {
                    let before = &p.text[..pos];
                    let after = &p.text[pos + ctext.len()..];
                    for (piece, nm) in [
                        (before, p.name.clone()),
                        (ctext, Some(name.clone())),
                        (after, p.name.clone()),
                    ] {
                        if !piece.trim().is_empty() {
                            nxt.push(RawSegment {
                                kind: p.kind.clone(),
                                text: piece.to_string(),
                                role: p.role.clone(),
                                name: nm,
                                turn: p.turn,
                                opaque: false,
                            });
                        }
                    }
                } else {
                    nxt.push(p);
                }
            } else {
                nxt.push(p);
            }
        }
        parts = nxt;
    }
    parts
}

pub fn build_segments(
    raw: &[RawSegment],
    counter: &TokenCounter,
    salt: &[u8],
    components: Option<&HashMap<String, String>>,
    split_system: bool,
    include_text: Option<bool>,
) -> Vec<Segment> {
    let inc_text = include_text
        .unwrap_or_else(|| std::env::var("TOKPROF_NO_CONTENT").ok().as_deref() != Some("1"));
    let empty_map = HashMap::new();
    let comp_map = components.unwrap_or(&empty_map);
    let mut out = Vec::with_capacity(raw.len());

    for r in raw {
        let pieces = if comp_map.is_empty() {
            vec![r.clone()]
        } else {
            split_components(r, comp_map)
        };
        for p in pieces {
            let sub_items: Vec<(Option<String>, String)> = if split_system
                && p.kind == "system"
                && !p.opaque
                && p.name.is_none()
                && p.text.len() >= MIN_SECTION_CHARS
            {
                split_sections(&p.text, MIN_SECTION_CHARS)
            } else {
                vec![(p.name.clone(), p.text.clone())]
            };
            for (name, body) in sub_items {
                let tokens = if p.opaque && body.is_empty() {
                    0
                } else {
                    counter.count(&body)
                };
                let chars = body.chars().count();
                let hash = if body.is_empty() {
                    String::new()
                } else {
                    hash_text(&body, salt)
                };
                let text = if inc_text && !body.is_empty() {
                    Some(redact_secrets(&body))
                } else {
                    None
                };
                out.push(Segment {
                    kind: p.kind.clone(),
                    tokens,
                    chars,
                    hash,
                    name,
                    role: p.role.clone(),
                    turn: p.turn,
                    method: counter.method.clone(),
                    opaque: p.opaque,
                    text,
                });
            }
        }
    }
    out
}

pub fn quality_label(residual: Option<i64>, reported: Option<i64>) -> Option<String> {
    let res = residual?;
    let rep = reported?;
    if rep <= 0 {
        return None;
    }
    let ratio = (res.unsigned_abs() as f64) / (rep as f64);
    if ratio <= 0.05 {
        Some("good".to_string())
    } else if ratio <= 0.15 {
        Some("fair".to_string())
    } else {
        Some("poor".to_string())
    }
}

pub struct BuildRecordArgs<'a> {
    pub parsed: &'a ParsedRequest,
    pub response: Option<&'a ParsedResponse>,
    pub counter: &'a TokenCounter,
    pub salt: &'a [u8],
    pub components: Option<&'a HashMap<String, String>>,
    pub endpoint: &'a str,
    pub status: Option<i64>,
    pub latency_ms: Option<i64>,
    pub ttfb_ms: Option<i64>,
    pub run: Option<String>,
    pub error: Option<String>,
    pub ts: Option<f64>,
    pub include_text: Option<bool>,
}

pub fn build_record(args: BuildRecordArgs<'_>) -> CallRecord {
    let inc_text = args
        .include_text
        .unwrap_or_else(|| std::env::var("TOKPROF_NO_CONTENT").ok().as_deref() != Some("1"));
    let segments = build_segments(
        &args.parsed.segments,
        args.counter,
        args.salt,
        args.components,
        true,
        Some(inc_text),
    );
    let est: i64 = segments.iter().map(|s| s.tokens).sum();
    let usage = args
        .response
        .filter(|r| r.has_usage)
        .map(|r| r.usage.sanitized())
        .unwrap_or_default();
    let reported = usage.input_total;
    let residual = reported.map(|rep| rep - est);

    let sys_raw: Vec<&str> = args
        .parsed
        .segments
        .iter()
        .filter(|s| s.kind == "system" && !s.text.is_empty())
        .map(|s| s.text.as_str())
        .collect();
    let sysfp = if sys_raw.is_empty() {
        None
    } else if sys_raw.len() == 1 && segments.len() == 1 && segments[0].kind == "system" {
        Some(segments[0].hash.clone())
    } else {
        Some(hash_text(&sys_raw.join("\n"), args.salt))
    };

    let first_non_system = args
        .parsed
        .segments
        .iter()
        .find(|s| s.kind == "history_user" || s.kind == "user_prompt");
    let sys_label = segments
        .iter()
        .find(|s| s.kind == "system" && s.name.is_some())
        .and_then(|s| s.name.clone())
        .or_else(|| args.parsed.step_hint.clone());

    let conv_id = if let Some(ref hint) = args.parsed.conv_hint {
        Some(hash_text(hint, args.salt))
    } else if let Some(seg) = first_non_system {
        if !seg.text.is_empty() {
            let first_line = seg.text.split('\n').next().unwrap_or(&seg.text);
            let key = if first_line.is_empty() {
                &seg.text
            } else {
                first_line
            };
            Some(hash_text(key, args.salt))
        } else {
            None
        }
    } else {
        None
    };

    let (output_est, resp_text) = match args.response {
        Some(r) if !r.text.is_empty() => {
            let cnt = args.counter.count(&r.text);
            let txt = if inc_text {
                Some(redact_secrets(&r.text))
            } else {
                None
            };
            (Some(cnt), txt)
        }
        _ => (None, None),
    };

    let mut id_bytes = [0u8; 6];
    rand::thread_rng().fill_bytes(&mut id_bytes);

    let now_ts = args.ts.unwrap_or_else(|| {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs_f64())
            .unwrap_or(0.0)
    });

    CallRecord {
        v: SCHEMA_VERSION,
        id: hex::encode(id_bytes),
        ts: now_ts,
        run: args.run,
        provider: args.parsed.provider.clone(),
        model: args.parsed.model.clone(),
        endpoint: args.endpoint.to_string(),
        host_class: None,
        host_id: None,
        host: None,
        detect: None,
        stream: args.parsed.stream,
        status: args.status,
        latency_ms: args.latency_ms,
        ttfb_ms: args.ttfb_ms,
        conv: conv_id,
        sysfp,
        sys_label,
        usage: usage.to_map(),
        segments,
        est_input: est,
        residual,
        quality: quality_label(residual, reported),
        tools_defined: args
            .parsed
            .tools
            .iter()
            .filter_map(|t| t.clone())
            .filter(|t| !t.is_empty())
            .collect(),
        tool_calls: args
            .response
            .map(|r| r.tool_calls.clone())
            .unwrap_or_default(),
        output_est,
        response_text: resp_text,
        error: args
            .error
            .or_else(|| args.response.and_then(|r| r.error.clone())),
    }
}
