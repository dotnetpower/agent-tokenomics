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

    let mut in_tok = non_neg_int(u.get("prompt_tokens"))
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

    // AWS Bedrock Converse reports non-cached tokens in `inputTokens` separately from cache read/write.
    if u.contains_key("inputTokens") && !u.contains_key("prompt_tokens") {
        let cr = cached.unwrap_or(0);
        let cw = cache_write.unwrap_or(0);
        if cr > 0 || cw > 0 {
            if let Some(base_in) = in_tok {
                let total_tok = non_neg_int(u.get("totalTokens"));
                let out_v = out_tok.unwrap_or(0);
                if total_tok == Some(base_in + cr + cw + out_v)
                    || total_tok.is_none()
                    || base_in < cr + cw
                {
                    in_tok = Some(base_in + cr + cw);
                }
            }
        }
    }

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
    let prompt = non_neg_int(u.get("promptTokenCount"));
    let tool_prompt = non_neg_int(u.get("toolUsePromptTokenCount")).unwrap_or(0);
    Usage {
        input_total: prompt.map(|p| p + tool_prompt),
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
                let itype = io.get("type").and_then(|v| v.as_str()).unwrap_or("");
                if matches!(itype, "function_call" | "custom_tool_call" | "mcp_call") {
                    if let Some(nm) = io.get("name").and_then(|v| v.as_str()) {
                        r.tool_calls.push(nm.to_string());
                    }
                } else if matches!(
                    itype,
                    "web_search_call"
                        | "file_search_call"
                        | "computer_call"
                        | "code_interpreter_call"
                        | "local_shell_call"
                ) {
                    r.tool_calls.push(itype.to_string());
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
                if matches!(btype, "tool_use" | "server_tool_use" | "mcp_tool_use") {
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

fn find_next_sse_boundary(slice: &[u8]) -> Option<(usize, usize)> {
    let mut offset = 0usize;
    while offset < slice.len() {
        let rel = slice[offset..].iter().position(|&b| b == b'\n')?;
        let i = offset + rel;
        if i + 1 < slice.len() && slice[i + 1] == b'\n' {
            if i > 0 && slice[i - 1] == b'\r' {
                return Some((i - 1, 3));
            }
            return Some((i, 2));
        }
        if i + 2 < slice.len() && slice[i + 1] == b'\r' && slice[i + 2] == b'\n' {
            if i > 0 && slice[i - 1] == b'\r' {
                return Some((i - 1, 4));
            }
            return Some((i, 3));
        }
        offset = i + 1;
    }
    None
}

#[inline]
fn trim_ascii(mut s: &[u8]) -> &[u8] {
    while let Some(&first) = s.first() {
        if first.is_ascii_whitespace() {
            s = &s[1..];
        } else {
            break;
        }
    }
    while let Some(&last) = s.last() {
        if last.is_ascii_whitespace() {
            s = &s[..s.len() - 1];
        } else {
            break;
        }
    }
    s
}

#[derive(serde::Deserialize)]
struct FastChatChunk<'a> {
    #[serde(default, borrow)]
    choices: Vec<FastChatChoice<'a>>,
}

#[derive(serde::Deserialize)]
struct FastChatChoice<'a> {
    #[serde(default, borrow)]
    delta: Option<FastChatDelta<'a>>,
}

#[derive(serde::Deserialize)]
struct FastChatDelta<'a> {
    #[serde(default, borrow)]
    content: Option<std::borrow::Cow<'a, str>>,
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
        // Fast path when internal buffer is empty: scan `data` directly without copying into `self.buf`
        if self.pos == self.buf.len() {
            self.buf.clear();
            self.pos = 0;
            let mut cur = 0usize;
            while cur < data.len() {
                let Some((idx, sep_len)) = find_next_sse_boundary(&data[cur..]) else {
                    break;
                };
                let ev = &data[cur..cur + idx];
                cur += idx + sep_len;
                self.process_event_slice(ev);
            }
            if cur < data.len() {
                let rem = &data[cur..];
                if rem.len() <= MAX_SSE_BUFFER {
                    self.buf.extend_from_slice(rem);
                }
            }
            return;
        }

        if self.buf.len().saturating_sub(self.pos) + data.len() > MAX_SSE_BUFFER {
            self.buf.clear();
            self.pos = 0;
            return;
        }
        self.buf.extend_from_slice(data);
        let mut local_buf = std::mem::take(&mut self.buf);
        while self.pos < local_buf.len() {
            let Some((idx, sep_len)) = find_next_sse_boundary(&local_buf[self.pos..]) else {
                break;
            };
            let ev_start = self.pos;
            let ev_end = self.pos + idx;
            self.pos += idx + sep_len;
            self.process_event_slice(&local_buf[ev_start..ev_end]);
        }
        if self.pos > 65_536 || self.pos == local_buf.len() {
            local_buf.drain(..self.pos);
            self.pos = 0;
        }
        self.buf = local_buf;
    }

    pub fn feed_text(&mut self, raw: &str) {
        if raw.contains("\n\n") || raw.contains("\r\n\r\n") {
            self.feed(raw.as_bytes());
            return;
        }
        for line in raw.lines() {
            if let Some(rest) = line.strip_prefix("data:") {
                let payload = rest.trim();
                if !payload.is_empty() && payload != "[DONE]" {
                    self.process_json_bytes(payload.as_bytes());
                }
            }
        }
    }

    pub fn finish(&mut self) -> ParsedResponse {
        if self.pos < self.buf.len() {
            let local_buf = std::mem::take(&mut self.buf);
            let pos = self.pos;
            self.pos = 0;
            self.process_event_slice(&local_buf[pos..]);
        }
        self.result.clone()
    }

    fn process_json_bytes(&mut self, slice: &[u8]) {
        if slice.is_empty() || slice == b"[DONE]" {
            return;
        }
        if self.provider == OPENAI_CHAT
            && find_subslice(slice, b"\"choices\"").is_some()
            && find_subslice(slice, b"\"usage\"").is_none()
            && find_subslice(slice, b"\"tool_calls\"").is_none()
            && find_subslice(slice, b"\"error\"").is_none()
        {
            if let Ok(chunk) = serde_json::from_slice::<FastChatChunk<'_>>(slice) {
                for ch in chunk.choices {
                    if let Some(d) = ch.delta {
                        if let Some(c) = d.content {
                            self.result.text.push_str(&c);
                        }
                    }
                }
                return;
            }
        }
        if let Ok(Value::Object(o)) = serde_json::from_slice::<Value>(slice) {
            self.process_obj(&o);
        }
    }

    fn process_event_slice(&mut self, raw: &[u8]) {
        if find_subslice(raw, b"data:").is_none() {
            return;
        }
        let mut first_payload: Option<&[u8]> = None;
        let mut multi_payload: Option<Vec<u8>> = None;
        let mut line_start = 0usize;
        while line_start <= raw.len() {
            let rel_nl = raw[line_start..]
                .iter()
                .position(|&b| b == b'\n')
                .unwrap_or(raw.len() - line_start);
            let mut line_end = line_start + rel_nl;
            if line_end > line_start && raw[line_end - 1] == b'\r' {
                line_end -= 1;
            }
            let line = &raw[line_start..line_end];
            if line.starts_with(b"data:") {
                let p = trim_ascii(&line[5..]);
                if let Some(ref mut joined) = multi_payload {
                    joined.push(b'\n');
                    joined.extend_from_slice(p);
                } else if let Some(fp) = first_payload {
                    let mut v = Vec::with_capacity(fp.len() + 1 + p.len());
                    v.extend_from_slice(fp);
                    v.push(b'\n');
                    v.extend_from_slice(p);
                    multi_payload = Some(v);
                } else {
                    first_payload = Some(p);
                }
            }
            if line_start + rel_nl >= raw.len() {
                break;
            }
            line_start += rel_nl + 1;
        }

        if let Some(joined) = multi_payload {
            self.process_json_bytes(&joined);
        } else if let Some(slice) = first_payload {
            self.process_json_bytes(slice);
        }
    }

    pub fn process_obj(&mut self, o: &Map<String, Value>) {
        if let Some(err_v) = o.get("error") {
            match err_v {
                Value::String(s) if !s.is_empty() => self.result.error = Some(s.clone()),
                Value::Object(eo) => {
                    let msg = eo
                        .get("message")
                        .or_else(|| eo.get("type"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("stream_error");
                    self.result.error = Some(msg.to_string());
                }
                _ => {}
            }
        }
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
                                    .filter(|s| !s.is_empty())
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
                    let itype = item.get("type").and_then(|v| v.as_str()).unwrap_or("");
                    if matches!(itype, "function_call" | "custom_tool_call" | "mcp_call") {
                        if let Some(nm) = item.get("name").and_then(|v| v.as_str()) {
                            if self.result.tool_calls.last().map(|s| s.as_str()) != Some(nm) {
                                self.result.tool_calls.push(nm.to_string());
                            }
                        }
                    } else if matches!(
                        itype,
                        "web_search_call"
                            | "file_search_call"
                            | "computer_call"
                            | "code_interpreter_call"
                            | "local_shell_call"
                    ) && self.result.tool_calls.last().map(|s| s.as_str()) != Some(itype)
                    {
                        self.result.tool_calls.push(itype.to_string());
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
                    if matches!(
                        cb.get("type").and_then(|v| v.as_str()),
                        Some("tool_use" | "server_tool_use" | "mcp_tool_use")
                    ) {
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
    let nlen = needle.len();
    if nlen == 0 || haystack.len() < nlen {
        return None;
    }
    let first = needle[0];
    if nlen == 1 {
        return haystack.iter().position(|&b| b == first);
    }
    let last_start = haystack.len() - nlen;
    let tail = &needle[1..];
    let mut offset = 0usize;
    while offset <= last_start {
        let rel = haystack[offset..=last_start]
            .iter()
            .position(|&b| b == first)?;
        let pos = offset + rel;
        if &haystack[pos + 1..pos + nlen] == tail {
            return Some(pos);
        }
        offset = pos + 1;
    }
    None
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
