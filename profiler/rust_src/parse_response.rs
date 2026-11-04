use crate::model::Usage;
use crate::parse_request::{ANTHROPIC, COMPLETIONS, GEMINI, OPENAI_CHAT, OPENAI_RESPONSES};
use base64::Engine;
use serde_json::{Map, Value};

const MAX_SSE_BUFFER: usize = 16 * 1024 * 1024;
const MAX_EVENTSTREAM_FRAMES: usize = 10_000;

#[derive(Debug, Clone, Default)]
pub struct ParsedResponse {
    pub usage: Usage,
    pub tool_calls: Vec<String>,
    pub text: String,
    pub has_usage: bool,
    pub error: Option<String>,
}

fn non_neg_int(v: Option<&Value>) -> Option<i64> {
    let val = v?;
    if val.is_boolean() {
        return None;
    }
    if let Some(n) = val.as_i64() {
        return if n >= 0 { Some(n) } else { None };
    }
    if let Some(f) = val.as_f64() {
        if f.is_finite() && f >= 0.0 && (f - f.round()).abs() < 1e-6 && f <= (i64::MAX as f64) {
            return Some(f.round() as i64);
        }
    }
    None
}

pub fn usage_openai_chat(u: &Map<String, Value>) -> Usage {
    let empty = Map::new();
    let d = u
        .get("prompt_tokens_details")
        .or_else(|| u.get("input_tokens_details"))
        .and_then(|v| v.as_object())
        .unwrap_or(&empty);
    let c = u
        .get("completion_tokens_details")
        .or_else(|| u.get("output_tokens_details"))
        .and_then(|v| v.as_object())
        .unwrap_or(&empty);
    let nested = u
        .get("tokens")
        .or_else(|| u.get("billed_units"))
        .and_then(|v| v.as_object())
        .unwrap_or(&empty);

    let in_tok = non_neg_int(u.get("prompt_tokens"))
        .or_else(|| non_neg_int(u.get("input_tokens")))
        .or_else(|| non_neg_int(u.get("inputTokens")))
        .or_else(|| non_neg_int(nested.get("input_tokens")));
    let out_tok = non_neg_int(u.get("completion_tokens"))
        .or_else(|| non_neg_int(u.get("output_tokens")))
        .or_else(|| non_neg_int(u.get("outputTokens")))
        .or_else(|| non_neg_int(nested.get("output_tokens")));
    let cached = non_neg_int(d.get("cached_tokens"))
        .or_else(|| non_neg_int(u.get("cacheReadInputTokens")))
        .or_else(|| non_neg_int(u.get("cache_read_input_tokens")));
    let cache_write = non_neg_int(d.get("cache_write_tokens"))
        .or_else(|| non_neg_int(u.get("cacheWriteInputTokens")))
        .or_else(|| non_neg_int(u.get("cache_creation_input_tokens")));

    Usage {
        input_total: in_tok,
        cached,
        cache_write,
        output: out_tok,
        reasoning: non_neg_int(c.get("reasoning_tokens")),
    }
}

pub fn usage_openai_responses(u: &Map<String, Value>) -> Usage {
    let empty = Map::new();
    let d = u
        .get("input_tokens_details")
        .and_then(|v| v.as_object())
        .unwrap_or(&empty);
    let c = u
        .get("output_tokens_details")
        .and_then(|v| v.as_object())
        .unwrap_or(&empty);
    Usage {
        input_total: non_neg_int(u.get("input_tokens")),
        cached: non_neg_int(d.get("cached_tokens")),
        cache_write: non_neg_int(d.get("cache_write_tokens")),
        output: non_neg_int(u.get("output_tokens")),
        reasoning: non_neg_int(c.get("reasoning_tokens")),
    }
}

pub fn usage_anthropic(u: &Map<String, Value>, into: &mut Usage) {
    let base = non_neg_int(u.get("input_tokens"));
    let read = non_neg_int(u.get("cache_read_input_tokens"));
    let write = non_neg_int(u.get("cache_creation_input_tokens"));
    if let Some(b) = base {
        into.input_total = Some(b + read.unwrap_or(0) + write.unwrap_or(0));
        if read.is_some() {
            into.cached = read;
        }
        if write.is_some() {
            into.cache_write = write;
        }
    }
    if let Some(out) = non_neg_int(u.get("output_tokens")) {
        into.output = Some(out);
    }
}

pub fn usage_gemini(u: &Map<String, Value>) -> Usage {
    Usage {
        input_total: non_neg_int(u.get("promptTokenCount")),
        cached: non_neg_int(u.get("cachedContentTokenCount")),
        cache_write: None,
        output: non_neg_int(u.get("candidatesTokenCount")),
        reasoning: non_neg_int(u.get("thoughtsTokenCount")),
    }
}

pub fn native_usage(o: &Map<String, Value>) -> Option<Usage> {
    if non_neg_int(o.get("prompt_eval_count")).is_some() || non_neg_int(o.get("eval_count")).is_some()
    {
        return Some(Usage {
            input_total: non_neg_int(o.get("prompt_eval_count")),
            output: non_neg_int(o.get("eval_count")),
            ..Default::default()
        });
    }
    if let Some(ev) = non_neg_int(o.get("tokens_evaluated")) {
        return Some(Usage {
            input_total: Some(ev),
            cached: non_neg_int(o.get("tokens_cached")),
            output: non_neg_int(o.get("tokens_predicted")),
            ..Default::default()
        });
    }
    if let Some(details) = o.get("details").and_then(|v| v.as_object()) {
        if let Some(prefill) = details.get("prefill").and_then(|v| v.as_array()) {
            return Some(Usage {
                input_total: Some(prefill.len() as i64),
                output: non_neg_int(details.get("generated_tokens")),
                ..Default::default()
            });
        }
    }
    if let Some(meta) = o
        .get("meta")
        .or_else(|| o.get("metadata"))
        .and_then(|v| v.as_object())
    {
        if let Some(u) = meta.get("usage").and_then(|v| v.as_object()) {
            return Some(usage_openai_chat(u));
        }
        if let Some(t) = meta
            .get("tokens")
            .or_else(|| meta.get("billed_units"))
            .and_then(|v| v.as_object())
        {
            if non_neg_int(t.get("input_tokens")).is_some()
                || non_neg_int(t.get("output_tokens")).is_some()
            {
                return Some(Usage {
                    input_total: non_neg_int(t.get("input_tokens")),
                    output: non_neg_int(t.get("output_tokens")),
                    ..Default::default()
                });
            }
        }
    }
    if let Some(bm) = o
        .get("amazon-bedrock-invocationMetrics")
        .and_then(|v| v.as_object())
    {
        if non_neg_int(bm.get("inputTokenCount")).is_some()
            || non_neg_int(bm.get("outputTokenCount")).is_some()
        {
            return Some(Usage {
                input_total: non_neg_int(bm.get("inputTokenCount")),
                output: non_neg_int(bm.get("outputTokenCount")),
                ..Default::default()
            });
        }
    }
    if let Some(u) = o
        .get("delta")
        .and_then(|v| v.as_object())
        .and_then(|d| d.get("message"))
        .and_then(|v| v.as_object())
        .and_then(|m| m.get("usage"))
        .and_then(|v| v.as_object())
    {
        return Some(usage_openai_chat(u));
    }
    None
}

fn apply_extra(provider: &str, o: &Map<String, Value>, r: &mut ParsedResponse) {
    if provider == GEMINI {
        if let Some(u) = o
            .get("usageMetadata")
            .or_else(|| o.get("usage_metadata"))
            .and_then(|v| v.as_object())
        {
            r.usage = usage_gemini(u);
            r.has_usage = true;
        }
        if let Some(cands) = o.get("candidates").and_then(|v| v.as_array()) {
            for cand in cands {
                if let Some(parts) = cand
                    .as_object()
                    .and_then(|c| c.get("content"))
                    .and_then(|v| v.as_object())
                    .and_then(|c| c.get("parts"))
                    .and_then(|v| v.as_array())
                {
                    for part in parts {
                        let Some(pobj) = part.as_object() else {
                            continue;
                        };
                        let is_thought = pobj
                            .get("thought")
                            .and_then(|v| v.as_bool())
                            .unwrap_or(false);
                        if !is_thought {
                            if let Some(txt) = pobj.get("text").and_then(|v| v.as_str()) {
                                r.text.push_str(txt);
                            }
                        }
                        if let Some(fc) = pobj
                            .get("functionCall")
                            .or_else(|| pobj.get("function_call"))
                            .and_then(|v| v.as_object())
                        {
                            if let Some(nm) = fc.get("name").and_then(|v| v.as_str()) {
                                r.tool_calls.push(nm.to_string());
                            }
                        }
                    }
                }
            }
        }
        return;
    }
    if matches!(provider, COMPLETIONS | OPENAI_CHAT | ANTHROPIC) {
        if let Some(nu) = native_usage(o) {
            if nu.input_total.is_some() || nu.output.is_some() {
                r.usage = nu;
                r.has_usage = true;
            }
        }
        if provider == COMPLETIONS {
            if let Some(u) = o.get("usage").and_then(|v| v.as_object()) {
                r.usage = usage_openai_chat(u);
                r.has_usage = true;
            }
            if let Some(choices) = o.get("choices").and_then(|v| v.as_array()) {
                for ch in choices {
                    if let Some(txt) = ch
                        .as_object()
                        .and_then(|c| c.get("text"))
                        .and_then(|v| v.as_str())
                    {
                        r.text.push_str(txt);
                    }
                }
            }
            for key in ["response", "content", "generated_text"] {
                if let Some(txt) = o.get(key).and_then(|v| v.as_str()) {
                    r.text.push_str(txt);
                    break;
                }
            }
            if let Some(txt) = o
                .get("token")
                .and_then(|v| v.as_object())
                .and_then(|t| t.get("text"))
                .and_then(|v| v.as_str())
            {
                r.text.push_str(txt);
            }
        } else if provider == OPENAI_CHAT {
            if !o.contains_key("choices") {
                if let Some(msg) = o.get("message").and_then(|v| v.as_object()) {
                    match msg.get("content") {
                        Some(Value::String(s)) => r.text.push_str(s),
                        Some(Value::Array(arr)) => {
                            for p in arr {
                                if let Some(txt) = p
                                    .as_object()
                                    .and_then(|po| po.get("text"))
                                    .and_then(|v| v.as_str())
                                {
                                    r.text.push_str(txt);
                                }
                            }
                        }
                        _ => {}
                    }
                    if let Some(tcs) = msg.get("tool_calls").and_then(|v| v.as_array()) {
                        for tc in tcs {
                            if let Some(tco) = tc.as_object() {
                                let inner = tco
                                    .get("function")
                                    .and_then(|v| v.as_object())
                                    .unwrap_or(tco);
                                if let Some(nm) = inner.get("name").and_then(|v| v.as_str()) {
                                    r.tool_calls.push(nm.to_string());
                                }
                            }
                        }
                    }
                }
                if let Some(txt) = o.get("text").and_then(|v| v.as_str()) {
                    r.text.push_str(txt);
                }
            }
            if let Some(tcs) = o.get("tool_calls").and_then(|v| v.as_array()) {
                for tc in tcs {
                    if let Some(nm) = tc
                        .as_object()
                        .and_then(|tco| tco.get("name"))
                        .and_then(|v| v.as_str())
                    {
                        r.tool_calls.push(nm.to_string());
                    }
                }
            }
            if let Some(out_msg) = o
                .get("output")
                .and_then(|v| v.as_object())
                .and_then(|oo| oo.get("message"))
                .and_then(|v| v.as_object())
            {
                if let Some(carr) = out_msg.get("content").and_then(|v| v.as_array()) {
                    for b in carr {
                        let Some(bo) = b.as_object() else { continue };
                        if let Some(txt) = bo.get("text").and_then(|v| v.as_str()) {
                            r.text.push_str(txt);
                        }
                        if let Some(nm) = bo
                            .get("toolUse")
                            .and_then(|v| v.as_object())
                            .and_then(|tu| tu.get("name"))
                            .and_then(|v| v.as_str())
                        {
                            r.tool_calls.push(nm.to_string());
                        }
                    }
                }
            }
            if let Some(txt) = o
                .get("contentBlockDelta")
                .and_then(|v| v.as_object())
                .and_then(|cbd| cbd.get("delta"))
                .and_then(|v| v.as_object())
                .and_then(|d| d.get("text"))
                .and_then(|v| v.as_str())
            {
                r.text.push_str(txt);
            }
            if let Some(nm) = o
                .get("contentBlockStart")
                .and_then(|v| v.as_object())
                .and_then(|cbs| cbs.get("start"))
                .and_then(|v| v.as_object())
                .and_then(|st| st.get("toolUse"))
                .and_then(|v| v.as_object())
                .and_then(|tu| tu.get("name"))
                .and_then(|v| v.as_str())
            {
                r.tool_calls.push(nm.to_string());
            }
            if let Some(txt) = o
                .get("delta")
                .and_then(|v| v.as_object())
                .and_then(|d| d.get("message"))
                .and_then(|v| v.as_object())
                .and_then(|m| m.get("content"))
                .and_then(|v| v.as_object())
                .and_then(|c| c.get("text"))
                .and_then(|v| v.as_str())
            {
                r.text.push_str(txt);
            }
        }
    }
}

pub fn parse_response_json(provider: &str, obj: &Value) -> ParsedResponse {
    let mut r = ParsedResponse::default();
    let Some(map) = obj.as_object() else {
        return r;
    };
    if let Some(err_v) = map.get("error") {
        match err_v {
            Value::String(s) => r.error = Some(s.clone()),
            Value::Object(eo) => {
                let msg = eo
                    .get("type")
                    .or_else(|| eo.get("message"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("error");
                r.error = Some(msg.to_string());
            }
            _ => {}
        }
    }
    if provider == OPENAI_CHAT {
        if let Some(u) = map.get("usage").and_then(|v| v.as_object()) {
            r.usage = usage_openai_chat(u);
            r.has_usage = true;
        }
        if let Some(choices) = map.get("choices").and_then(|v| v.as_array()) {
            for ch in choices {
                if let Some(msg) = ch
                    .as_object()
                    .and_then(|co| co.get("message"))
                    .and_then(|v| v.as_object())
                {
                    if let Some(txt) = msg.get("content").and_then(|v| v.as_str()) {
                        r.text.push_str(txt);
                    }
                    if let Some(tcs) = msg.get("tool_calls").and_then(|v| v.as_array()) {
                        for tc in tcs {
                            if let Some(nm) = tc
                                .as_object()
                                .and_then(|tco| tco.get("function"))
                                .and_then(|v| v.as_object())
                                .and_then(|f| f.get("name"))
                                .and_then(|v| v.as_str())
                            {
                                r.tool_calls.push(nm.to_string());
                            }
                        }
                    }
                }
            }
        }
    } else if provider == OPENAI_RESPONSES {
        if let Some(u) = map.get("usage").and_then(|v| v.as_object()) {
            r.usage = usage_openai_responses(u);
            r.has_usage = true;
        }
        if let Some(out_arr) = map.get("output").and_then(|v| v.as_array()) {
            for item in out_arr {
                let Some(io) = item.as_object() else { continue };
                if io.get("type").and_then(|v| v.as_str()) == Some("function_call") {
                    if let Some(nm) = io.get("name").and_then(|v| v.as_str()) {
                        r.tool_calls.push(nm.to_string());
                    }
                }
                if let Some(carr) = io.get("content").and_then(|v| v.as_array()) {
                    for part in carr {
                        if let Some(txt) = part
                            .as_object()
                            .and_then(|po| po.get("text"))
                            .and_then(|v| v.as_str())
                        {
                            r.text.push_str(txt);
                        }
                    }
                }
            }
        }
    } else if provider == ANTHROPIC {
        if let Some(u) = map.get("usage").and_then(|v| v.as_object()) {
            usage_anthropic(u, &mut r.usage);
            r.has_usage = true;
        }
        if let Some(carr) = map.get("content").and_then(|v| v.as_array()) {
            for b in carr {
                let Some(bo) = b.as_object() else { continue };
                let btype = bo.get("type").and_then(|v| v.as_str()).unwrap_or("");
                if btype == "tool_use" {
                    if let Some(nm) = bo.get("name").and_then(|v| v.as_str()) {
                        r.tool_calls.push(nm.to_string());
                    }
                } else if btype == "text" {
                    if let Some(txt) = bo.get("text").and_then(|v| v.as_str()) {
                        r.text.push_str(txt);
                    }
                }
            }
        }
    }
    if matches!(provider, COMPLETIONS | GEMINI | ANTHROPIC)
        || (provider == OPENAI_CHAT && !map.contains_key("choices"))
    {
        apply_extra(provider, map, &mut r);
    }
    r
}

pub struct SSEParser {
    pub provider: String,
    buf: Vec<u8>,
    pos: usize,
    pub result: ParsedResponse,
}

impl SSEParser {
    pub fn new(provider: impl Into<String>) -> Self {
        Self {
            provider: provider.into(),
            buf: Vec::new(),
            pos: 0,
            result: ParsedResponse::default(),
        }
    }

    pub fn feed(&mut self, data: &[u8]) {
        if data.is_empty() {
            return;
        }
        if self.buf.len().saturating_sub(self.pos) + data.len() > MAX_SSE_BUFFER {
            self.buf.clear();
            self.pos = 0;
            return;
        }
        self.buf.extend_from_slice(data);
        while self.pos < self.buf.len() {
            let slice = &self.buf[self.pos..];
            let idx_lf = find_subslice(slice, b"\n\n");
            let idx_crlf = find_subslice(slice, b"\r\n\r\n");
            let (idx, sep_len) = match (idx_lf, idx_crlf) {
                (None, None) => break,
                (Some(l), None) => (l, 2),
                (None, Some(c)) => (c, 4),
                (Some(l), Some(c)) => {
                    if c < l {
                        (c, 4)
                    } else {
                        (l, 2)
                    }
                }
            };
            let event = self.buf[self.pos..self.pos + idx].to_vec();
            self.pos += idx + sep_len;
            self.process_event(&event);
        }
        if self.pos > 65_536 || self.pos == self.buf.len() {
            self.buf.drain(..self.pos);
            self.pos = 0;
        }
    }

    pub fn feed_text(&mut self, raw: &str) {
        for line in raw.lines() {
            if let Some(rest) = line.strip_prefix("data:") {
                let payload = rest.trim();
                if !payload.is_empty() && payload != "[DONE]" {
                    if let Ok(Value::Object(o)) = serde_json::from_str::<Value>(payload) {
                        self.process_obj(&o);
                    }
                }
            }
        }
    }

    pub fn finish(&mut self) -> ParsedResponse {
        if self.pos < self.buf.len() {
            let rem = self.buf[self.pos..].to_vec();
            self.buf.clear();
            self.pos = 0;
            self.process_event(&rem);
        }
        self.result.clone()
    }

    fn process_event(&mut self, raw: &[u8]) {
        if find_subslice(raw, b"data:").is_none() {
            return;
        }
        let s = String::from_utf8_lossy(raw);
        let mut payloads = Vec::new();
        for line in s.lines() {
            if let Some(rest) = line.strip_prefix("data:") {
                payloads.push(rest.trim());
            }
        }
        if payloads.is_empty() {
            return;
        }
        let joined = payloads.join("\n");
        if joined == "[DONE]" || joined.is_empty() {
            return;
        }
        if let Ok(Value::Object(o)) = serde_json::from_str::<Value>(&joined) {
            self.process_obj(&o);
        }
    }

    pub fn process_obj(&mut self, o: &Map<String, Value>) {
        let p = self.provider.as_str();
        if p == OPENAI_CHAT {
            if let Some(u) = o.get("usage").and_then(|v| v.as_object()) {
                self.result.usage = usage_openai_chat(u);
                self.result.has_usage = true;
            }
            if let Some(choices) = o.get("choices").and_then(|v| v.as_array()) {
                for ch in choices {
                    if let Some(delta) = ch
                        .as_object()
                        .and_then(|co| co.get("delta"))
                        .and_then(|v| v.as_object())
                    {
                        if let Some(txt) = delta.get("content").and_then(|v| v.as_str()) {
                            self.result.text.push_str(txt);
                        }
                        if let Some(tcs) = delta.get("tool_calls").and_then(|v| v.as_array()) {
                            for tc in tcs {
                                if let Some(nm) = tc
                                    .as_object()
                                    .and_then(|tco| tco.get("function"))
                                    .and_then(|v| v.as_object())
                                    .and_then(|f| f.get("name"))
                                    .and_then(|v| v.as_str())
                                {
                                    self.result.tool_calls.push(nm.to_string());
                                }
                            }
                        }
                    }
                }
            }
        } else if p == OPENAI_RESPONSES {
            let t = o.get("type").and_then(|v| v.as_str()).unwrap_or("");
            if t == "response.output_text.delta" {
                if let Some(d) = o.get("delta").and_then(|v| v.as_str()) {
                    self.result.text.push_str(d);
                }
            } else if t == "response.output_item.added" || t == "response.output_item.done" {
                if let Some(item) = o.get("item").and_then(|v| v.as_object()) {
                    if item.get("type").and_then(|v| v.as_str()) == Some("function_call") {
                        if let Some(nm) = item.get("name").and_then(|v| v.as_str()) {
                            if self.result.tool_calls.last().map(|s| s.as_str()) != Some(nm) {
                                self.result.tool_calls.push(nm.to_string());
                            }
                        }
                    }
                }
            } else if matches!(
                t,
                "response.completed" | "response.incomplete" | "response.failed"
            ) {
                if let Some(u) = o
                    .get("response")
                    .and_then(|v| v.as_object())
                    .and_then(|ro| ro.get("usage"))
                    .and_then(|v| v.as_object())
                {
                    self.result.usage = usage_openai_responses(u);
                    self.result.has_usage = true;
                }
            }
        } else if p == ANTHROPIC {
            let t = o.get("type").and_then(|v| v.as_str()).unwrap_or("");
            if t == "message_start" {
                if let Some(u) = o
                    .get("message")
                    .and_then(|v| v.as_object())
                    .and_then(|mo| mo.get("usage"))
                    .and_then(|v| v.as_object())
                {
                    usage_anthropic(u, &mut self.result.usage);
                    self.result.has_usage = true;
                }
            } else if t == "content_block_start" {
                if let Some(cb) = o.get("content_block").and_then(|v| v.as_object()) {
                    if cb.get("type").and_then(|v| v.as_str()) == Some("tool_use") {
                        if let Some(nm) = cb.get("name").and_then(|v| v.as_str()) {
                            self.result.tool_calls.push(nm.to_string());
                        }
                    }
                }
            } else if t == "content_block_delta" {
                if let Some(d) = o.get("delta").and_then(|v| v.as_object()) {
                    if d.get("type").and_then(|v| v.as_str()) == Some("text_delta") {
                        if let Some(txt) = d.get("text").and_then(|v| v.as_str()) {
                            self.result.text.push_str(txt);
                        }
                    }
                }
            } else if t == "message_delta" {
                if let Some(u) = o.get("usage").and_then(|v| v.as_object()) {
                    usage_anthropic(u, &mut self.result.usage);
                    self.result.has_usage = true;
                }
            }
        }
        if matches!(p, COMPLETIONS | GEMINI | ANTHROPIC)
            || (p == OPENAI_CHAT && !o.contains_key("choices"))
        {
            apply_extra(p, o, &mut self.result);
        }
    }
}

pub fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    haystack.windows(needle.len()).position(|w| w == needle)
}

fn unwrap_eventstream_obj(obj: Map<String, Value>) -> Vec<Map<String, Value>> {
    if let Some(b64) = obj.get("bytes").and_then(|v| v.as_str()) {
        if let Ok(decoded) = base64::engine::general_purpose::STANDARD.decode(b64) {
            if let Ok(Value::Object(inner)) = serde_json::from_slice::<Value>(&decoded) {
                return vec![inner];
            }
        }
    }
    vec![obj]
}

const EVENTSTREAM_HINTS: &[&str] = &[
    ":event-type",
    ":content-type",
    ":message-type",
    "\"bytes\"",
    "contentBlockDelta",
    "contentBlockStart",
    "amazon-bedrock-invocationMetrics",
    "\x00\x00",
];

pub fn extract_eventstream_objects_bytes(buf: &[u8]) -> Vec<Map<String, Value>> {
    let mut out = Vec::new();
    let mut pos = 0usize;
    while pos.checked_add(16).map(| end| end <= buf.len()).unwrap_or(false)
        && out.len() < MAX_EVENTSTREAM_FRAMES
    {
        let total_len =
            u32::from_be_bytes([buf[pos], buf[pos + 1], buf[pos + 2], buf[pos + 3]]) as usize;
        let headers_len =
            u32::from_be_bytes([buf[pos + 4], buf[pos + 5], buf[pos + 6], buf[pos + 7]]) as usize;
        let Some(frame_end) = pos.checked_add(total_len) else {
            break;
        };
        let Some(payload_rel_start) = 12usize.checked_add(headers_len) else {
            break;
        };
        if total_len < 16 || frame_end > buf.len() || payload_rel_start > total_len - 4 {
            break;
        }
        let payload = &buf[pos + payload_rel_start..frame_end - 4];
        pos = frame_end;
        if !payload.is_empty() {
            if let Ok(Value::Object(obj)) = serde_json::from_slice::<Value>(payload) {
                out.extend(unwrap_eventstream_obj(obj));
            }
        }
    }
    if !out.is_empty() {
        return out;
    }
    let text = String::from_utf8_lossy(buf);
    extract_eventstream_objects_str(&text)
}

pub fn extract_eventstream_objects_str(text: &str) -> Vec<Map<String, Value>> {
    let mut out = Vec::new();
    if text.is_empty() || !EVENTSTREAM_HINTS.iter().any(|h| text.contains(h)) {
        return out;
    }
    let bytes = text.as_bytes();
    let mut idx = 0usize;
    while idx < bytes.len() && out.len() < MAX_EVENTSTREAM_FRAMES {
        let Some(rel_brace) = bytes[idx..].iter().position(|&b| b == b'{') else {
            break;
        };
        let brace = idx + rel_brace;
        let sub = &text[brace..];
        let mut stream = serde_json::Deserializer::from_str(sub).into_iter::<Value>();
        if let Some(Ok(val)) = stream.next() {
            let offset = stream.byte_offset();
            idx = brace + offset.max(1);
            if let Value::Object(obj) = val {
                out.extend(unwrap_eventstream_obj(obj));
            }
        } else {
            idx = brace + 1;
        }
    }
    out
}

pub fn parse_response_body_bytes(provider: &str, data: &[u8], sse: bool) -> ParsedResponse {
    let frames = extract_eventstream_objects_bytes(data);
    if !frames.is_empty() {
        let mut parser = SSEParser::new(provider);
        for c in frames {
            parser.process_obj(&c);
        }
        return parser.finish();
    }
    let text = String::from_utf8_lossy(data);
    parse_response_body(provider, &text, sse)
}

pub fn parse_response_body(provider: &str, text: &str, sse: bool) -> ParsedResponse {
    let raw = text.trim();
    if raw.is_empty() {
        return ParsedResponse::default();
    }
    if sse
        || raw.starts_with("data:")
        || raw.starts_with("event:")
        || raw.starts_with(':')
    {
        let mut parser = SSEParser::new(provider);
        parser.feed_text(raw);
        return parser.finish();
    }
    if let Ok(val) = serde_json::from_str::<Value>(raw) {
        if val.is_object() {
            return parse_response_json(provider, &val);
        }
        if let Value::Array(arr) = val {
            let mut parser = SSEParser::new(provider);
            for c in arr {
                if let Value::Object(o) = c {
                    parser.process_obj(&o);
                }
            }
            return parser.finish();
        }
    }
    let mut ndjson_chunks = Vec::new();
    let mut ndjson_ok = true;
    for line in raw.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        match serde_json::from_str::<Value>(line) {
            Ok(Value::Object(o)) => ndjson_chunks.push(o),
            _ => {
                ndjson_ok = false;
                break;
            }
        }
    }
    let chunks = if ndjson_ok && !ndjson_chunks.is_empty() {
        ndjson_chunks
    } else {
        extract_eventstream_objects_str(raw)
    };
    if chunks.is_empty() {
        return ParsedResponse::default();
    }
    let mut parser = SSEParser::new(provider);
    for c in chunks {
        parser.process_obj(&c);
    }
    parser.finish()
}
