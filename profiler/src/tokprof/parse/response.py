"""Response parsing: usage, tool-call names, and streamed (SSE) responses."""

from __future__ import annotations

import base64
import json
import struct
from dataclasses import dataclass, field
from typing import Any, Optional

from ..model import Usage
from .request import ANTHROPIC, COMPLETIONS, GEMINI, OPENAI_CHAT, OPENAI_RESPONSES


@dataclass
class ParsedResponse:
    usage: Usage = field(default_factory=Usage)
    tool_calls: list = field(default_factory=list)
    text: str = ""
    has_usage: bool = False
    error: Optional[str] = None


def _int(v: Any) -> Optional[int]:
    return v if isinstance(v, int) and not isinstance(v, bool) and v >= 0 else None


def usage_openai_chat(u: dict) -> Usage:
    if not isinstance(u, dict):
        return Usage()
    raw_d = u.get("prompt_tokens_details") or u.get("input_tokens_details")
    d = raw_d if isinstance(raw_d, dict) else {}
    raw_c = u.get("completion_tokens_details") or u.get("output_tokens_details")
    c = raw_c if isinstance(raw_c, dict) else {}
    nested = u.get("tokens") if isinstance(u.get("tokens"), dict) else (u.get("billed_units") if isinstance(u.get("billed_units"), dict) else {})
    in_tok = _int(u.get("prompt_tokens"))
    if in_tok is None:
        in_tok = _int(u.get("input_tokens"))
    if in_tok is None:
        in_tok = _int(u.get("inputTokens"))
    if in_tok is None:
        in_tok = _int(nested.get("input_tokens"))
    out_tok = _int(u.get("completion_tokens"))
    if out_tok is None:
        out_tok = _int(u.get("output_tokens"))
    if out_tok is None:
        out_tok = _int(u.get("outputTokens"))
    if out_tok is None:
        out_tok = _int(nested.get("output_tokens"))
    cached = _int(d.get("cached_tokens"))
    if cached is None:
        cached = _int(u.get("cacheReadInputTokens") if "cacheReadInputTokens" in u else u.get("cache_read_input_tokens"))
    cache_write = _int(d.get("cache_write_tokens"))
    if cache_write is None:
        cache_write = _int(u.get("cacheWriteInputTokens") if "cacheWriteInputTokens" in u else u.get("cache_creation_input_tokens"))
    return Usage(
        input_total=in_tok,
        cached=cached,
        cache_write=cache_write,
        output=out_tok,
        reasoning=_int(c.get("reasoning_tokens")),
    )


def usage_openai_responses(u: dict) -> Usage:
    if not isinstance(u, dict):
        return Usage()
    raw_d = u.get("input_tokens_details")
    d = raw_d if isinstance(raw_d, dict) else {}
    raw_c = u.get("output_tokens_details")
    c = raw_c if isinstance(raw_c, dict) else {}
    return Usage(
        input_total=_int(u.get("input_tokens")),
        cached=_int(d.get("cached_tokens")),
        cache_write=_int(d.get("cache_write_tokens")),
        output=_int(u.get("output_tokens")),
        reasoning=_int(c.get("reasoning_tokens")),
    )


def usage_anthropic(u: dict, into: Optional[Usage] = None) -> Usage:
    """Anthropic reports input_tokens excluding cached tokens; normalize to a total."""
    out = into or Usage()
    if not isinstance(u, dict):
        return out
    base = _int(u.get("input_tokens"))
    read = _int(u.get("cache_read_input_tokens"))
    write = _int(u.get("cache_creation_input_tokens"))
    if base is not None:
        out.input_total = base + (read or 0) + (write or 0)
        out.cached = read if read is not None else out.cached
        out.cache_write = write if write is not None else out.cache_write
    if _int(u.get("output_tokens")) is not None:
        out.output = u["output_tokens"]
    return out


def usage_gemini(u: dict) -> Usage:
    if not isinstance(u, dict):
        return Usage()
    return Usage(
        input_total=_int(u.get("promptTokenCount")),
        cached=_int(u.get("cachedContentTokenCount")),
        output=_int(u.get("candidatesTokenCount")),
        reasoning=_int(u.get("thoughtsTokenCount")),
    )


def native_usage(o: dict) -> Optional[Usage]:
    """Usage in the formats of self-hosted servers, Bedrock, and Cohere."""
    if _int(o.get("prompt_eval_count")) is not None or _int(o.get("eval_count")) is not None:  # Ollama
        return Usage(input_total=_int(o.get("prompt_eval_count")), output=_int(o.get("eval_count")))
    if _int(o.get("tokens_evaluated")) is not None:  # llama.cpp server
        return Usage(input_total=o["tokens_evaluated"], cached=_int(o.get("tokens_cached")), output=_int(o.get("tokens_predicted")))
    details = o.get("details")
    if isinstance(details, dict) and isinstance(details.get("prefill"), list):  # TGI /generate with details=true
        return Usage(input_total=len(details["prefill"]), output=_int(details.get("generated_tokens")))
    meta = o.get("meta") or o.get("metadata")
    if isinstance(meta, dict):
        if isinstance(meta.get("usage"), dict):  # Bedrock Converse stream metadata
            return usage_openai_chat(meta["usage"])
        t = meta.get("tokens") if isinstance(meta.get("tokens"), dict) else meta.get("billed_units")
        if isinstance(t, dict) and (_int(t.get("input_tokens")) is not None or _int(t.get("output_tokens")) is not None):  # Cohere v1
            return Usage(input_total=_int(t.get("input_tokens")), output=_int(t.get("output_tokens")))
    bm = o.get("amazon-bedrock-invocationMetrics")
    if isinstance(bm, dict) and (_int(bm.get("inputTokenCount")) is not None or _int(bm.get("outputTokenCount")) is not None):
        return Usage(input_total=_int(bm.get("inputTokenCount")), output=_int(bm.get("outputTokenCount")))
    delta = o.get("delta")
    if isinstance(delta, dict) and isinstance((delta.get("message") or {}).get("usage"), dict):  # Cohere v2 SSE message-end
        return usage_openai_chat(delta["message"]["usage"])
    return None


def _apply_extra(provider: str, o: dict, r: ParsedResponse) -> None:
    """Fields of the non-OpenAI-chat shapes; the same code serves whole responses and stream chunks."""
    if provider == GEMINI:
        u = o.get("usageMetadata") or o.get("usage_metadata")
        if isinstance(u, dict):
            r.usage, r.has_usage = usage_gemini(u), True
        for cand in o.get("candidates") or []:
            for part in ((cand or {}).get("content") or {}).get("parts") or []:
                if not isinstance(part, dict):
                    continue
                if isinstance(part.get("text"), str) and not part.get("thought"):
                    r.text += part["text"]
                fc = part.get("functionCall") or part.get("function_call")
                if isinstance(fc, dict) and fc.get("name"):
                    r.tool_calls.append(fc["name"])
        return
    if provider in (COMPLETIONS, OPENAI_CHAT, ANTHROPIC):
        nu = native_usage(o)  # Ollama, llama.cpp, Bedrock, Cohere
        if nu is not None and (nu.input_total is not None or nu.output is not None):
            r.usage, r.has_usage = nu, True
        if provider == COMPLETIONS:
            if isinstance(o.get("usage"), dict):
                r.usage, r.has_usage = usage_openai_chat(o["usage"]), True
            for ch in o.get("choices") or []:
                if isinstance((ch or {}).get("text"), str):
                    r.text += ch["text"]
            for key in ("response", "content", "generated_text"):  # Ollama, llama.cpp, TGI
                if isinstance(o.get(key), str):
                    r.text += o[key]
                    break
            tok = o.get("token")  # TGI /generate_stream
            if isinstance(tok, dict) and isinstance(tok.get("text"), str):
                r.text += tok["text"]
        elif provider == OPENAI_CHAT:  # Ollama /api/chat, Cohere v1/v2, Bedrock Converse
            msg = o.get("message")
            if isinstance(msg, dict) and "choices" not in o:
                c = msg.get("content")
                if isinstance(c, str):
                    r.text += c
                elif isinstance(c, list):
                    for part in c:
                        if isinstance(part, dict) and isinstance(part.get("text"), str):
                            r.text += part["text"]
                for tc in msg.get("tool_calls") or []:
                    name = ((tc or {}).get("function") or tc or {}).get("name")
                    if name:
                        r.tool_calls.append(name)
            if isinstance(o.get("text"), str) and "choices" not in o:  # Cohere v1
                r.text += o["text"]
            for tc in o.get("tool_calls") or []:
                if isinstance(tc, dict) and tc.get("name"):
                    r.tool_calls.append(tc["name"])
            out_msg = (o.get("output") or {}).get("message") if isinstance(o.get("output"), dict) else None
            if isinstance(out_msg, dict):  # Bedrock Converse
                for b in out_msg.get("content") or []:
                    if isinstance(b, dict):
                        if isinstance(b.get("text"), str):
                            r.text += b["text"]
                        tu = b.get("toolUse")
                        if isinstance(tu, dict) and tu.get("name"):
                            r.tool_calls.append(tu["name"])
            cbd = o.get("contentBlockDelta")
            if isinstance(cbd, dict) and isinstance((cbd.get("delta") or {}).get("text"), str):  # Bedrock Converse stream
                r.text += cbd["delta"]["text"]
            cbs = o.get("contentBlockStart")
            if isinstance(cbs, dict):
                tu = ((cbs.get("start") or {}).get("toolUse") or {})
                if isinstance(tu, dict) and tu.get("name"):
                    r.tool_calls.append(tu["name"])
            delta = o.get("delta")
            if isinstance(delta, dict) and isinstance(((delta.get("message") or {}).get("content") or {}).get("text"), str):  # Cohere v2 SSE
                r.text += delta["message"]["content"]["text"]


def parse_response_json(provider: str, obj: Any) -> ParsedResponse:
    r = ParsedResponse()
    if not isinstance(obj, dict):
        return r
    if isinstance(obj.get("error"), (dict, str)):
        e = obj["error"]
        r.error = (e.get("type") or e.get("message") or "error") if isinstance(e, dict) else str(e)
    if provider == OPENAI_CHAT:
        if isinstance(obj.get("usage"), dict):
            r.usage, r.has_usage = usage_openai_chat(obj["usage"]), True
        for ch in obj.get("choices") or []:
            msg = (ch or {}).get("message") or {}
            if isinstance(msg.get("content"), str):
                r.text += msg["content"]
            for tc in msg.get("tool_calls") or []:
                name = ((tc or {}).get("function") or {}).get("name")
                if name:
                    r.tool_calls.append(name)
    elif provider == OPENAI_RESPONSES:
        if isinstance(obj.get("usage"), dict):
            r.usage, r.has_usage = usage_openai_responses(obj["usage"]), True
        for item in obj.get("output") or []:
            if (item or {}).get("type") == "function_call" and item.get("name"):
                r.tool_calls.append(item["name"])
            for part in (item or {}).get("content") or []:
                if isinstance(part, dict) and isinstance(part.get("text"), str):
                    r.text += part["text"]
    elif provider == ANTHROPIC:
        if isinstance(obj.get("usage"), dict):
            r.usage, r.has_usage = usage_anthropic(obj["usage"]), True
        for b in obj.get("content") or []:
            if (b or {}).get("type") == "tool_use" and b.get("name"):
                r.tool_calls.append(b["name"])
            elif (b or {}).get("type") == "text" and isinstance(b.get("text"), str):
                r.text += b["text"]
    if provider in (COMPLETIONS, GEMINI, ANTHROPIC) or (provider == OPENAI_CHAT and "choices" not in obj):
        _apply_extra(provider, obj, r)
    return r


MAX_SSE_BUFFER = 16 * 1024 * 1024
MAX_EVENTSTREAM_FRAMES = 10_000


class SSEParser:
    """Incremental O(N) parser for server-sent-event responses."""

    def __init__(self, provider: str):
        self.provider = provider
        self._buf = bytearray()
        self._pos = 0
        self._text_parts: list = []
        self.result = ParsedResponse()

    def _append_text(self, s: str) -> None:
        if s:
            self._text_parts.append(s)

    def feed(self, data: bytes) -> None:
        if not data:
            return
        if len(self._buf) - self._pos + len(data) > MAX_SSE_BUFFER:
            self._buf.clear()
            self._pos = 0
            return
        self._buf.extend(data)
        buf = self._buf
        pos = self._pos
        n = len(buf)
        has_crlf = buf.find(b"\r\n\r\n", pos) >= 0
        if not has_crlf:
            while pos < n:
                idx = buf.find(b"\n\n", pos)
                if idx < 0:
                    break
                self._event_bytes(bytes(buf[pos:idx]))
                pos = idx + 2
        else:
            while pos < n:
                idx_lf = buf.find(b"\n\n", pos)
                idx_crlf = buf.find(b"\r\n\r\n", pos)
                if idx_lf < 0 and idx_crlf < 0:
                    break
                if idx_crlf >= 0 and (idx_lf < 0 or idx_crlf < idx_lf):
                    idx, sep_len = idx_crlf, 4
                else:
                    idx, sep_len = idx_lf, 2
                self._event_bytes(bytes(buf[pos:idx]))
                pos = idx + sep_len
        if pos > 65536 or pos == len(buf):
            del buf[:pos]
            pos = 0
        self._pos = pos

    def feed_text(self, raw: str) -> None:
        """Fast direct-string path when the full SSE transcript is already decoded as a str."""
        for line in raw.splitlines():
            if line.startswith("data:"):
                payload = line[5:].strip()
                if payload and payload != "[DONE]":
                    try:
                        obj = json.loads(payload)
                    except ValueError:
                        continue
                    if isinstance(obj, dict):
                        self._obj(obj)

    def finish(self) -> ParsedResponse:
        if self._pos < len(self._buf):
            rem = bytes(self._buf[self._pos:]).strip()
            self._buf.clear()
            self._pos = 0
            if rem:
                self._event_bytes(rem)
        if self._text_parts:
            self.result.text = (self.result.text + "".join(self._text_parts)) if self.result.text else "".join(self._text_parts)
            self._text_parts.clear()
        return self.result

    def _event_bytes(self, raw: bytes) -> None:
        if b"data:" not in raw:
            return
        payload = []
        for line in raw.splitlines():
            if line.startswith(b"data:"):
                payload.append(line[5:].strip())
        if not payload:
            return
        text = b"\n".join(payload) if len(payload) > 1 else payload[0]
        if text == b"[DONE]" or not text:
            return
        try:
            obj = json.loads(text)
        except ValueError:
            return
        if isinstance(obj, dict):
            self._obj(obj)

    def _obj(self, o: dict) -> None:
        r, p = self.result, self.provider
        prev_text = r.text
        r.text = ""
        if p == OPENAI_CHAT:
            if isinstance(o.get("usage"), dict):
                r.usage, r.has_usage = usage_openai_chat(o["usage"]), True
            for ch in o.get("choices") or []:
                delta = (ch or {}).get("delta") or {}
                if isinstance(delta.get("content"), str):
                    self._append_text(delta["content"])
                for tc in delta.get("tool_calls") or []:
                    name = ((tc or {}).get("function") or {}).get("name")
                    if name:
                        r.tool_calls.append(name)
        elif p == OPENAI_RESPONSES:
            t = o.get("type", "")
            if t == "response.output_text.delta" and isinstance(o.get("delta"), str):
                self._append_text(o["delta"])
            elif t in ("response.output_item.added", "response.output_item.done"):
                item = o.get("item") or {}
                if item.get("type") == "function_call" and item.get("name") and item["name"] not in r.tool_calls[-1:]:
                    r.tool_calls.append(item["name"])
            elif t in ("response.completed", "response.incomplete", "response.failed"):
                u = (o.get("response") or {}).get("usage")
                if isinstance(u, dict):
                    r.usage, r.has_usage = usage_openai_responses(u), True
        elif p == ANTHROPIC:
            t = o.get("type", "")
            if t == "message_start":
                u = (o.get("message") or {}).get("usage")
                if isinstance(u, dict):
                    usage_anthropic(u, r.usage)
                    r.has_usage = True
            elif t == "content_block_start":
                b = o.get("content_block") or {}
                if b.get("type") == "tool_use" and b.get("name"):
                    r.tool_calls.append(b["name"])
            elif t == "content_block_delta":
                d = o.get("delta") or {}
                if d.get("type") == "text_delta" and isinstance(d.get("text"), str):
                    self._append_text(d["text"])
            elif t == "message_delta" and isinstance(o.get("usage"), dict):
                usage_anthropic(o["usage"], r.usage)
                r.has_usage = True
        if p in (COMPLETIONS, GEMINI, ANTHROPIC) or (p == OPENAI_CHAT and "choices" not in o):
            _apply_extra(p, o, r)
            if r.text:
                self._append_text(r.text)
                r.text = ""
        if prev_text:
            r.text = prev_text


def _unwrap_eventstream_obj(obj: dict) -> list:
    if isinstance(obj.get("bytes"), str):  # Bedrock invoke-with-response-stream wraps chunks in base64
        try:
            decoded = base64.b64decode(obj["bytes"]).decode("utf-8", "replace")
            inner = json.loads(decoded)
            if isinstance(inner, dict):
                return [inner]
        except Exception:
            pass
    return [obj]


_EVENTSTREAM_HINTS = (":event-type", ":content-type", ":message-type", '"bytes"', "contentBlockDelta", "contentBlockStart", "amazon-bedrock-invocationMetrics", "\x00\x00")


def extract_eventstream_objects(raw: Any) -> list:
    """Extract JSON payload objects from an AWS EventStream (`application/vnd.amazon.eventstream`) body.

    Works both on raw binary frames (12-byte prelude + headers + JSON payload + 4-byte CRC) and on text
    buffers where binary framing bytes were decoded as UTF-8 replacement characters.
    """
    out: list = []
    if isinstance(raw, (bytes, bytearray)):
        buf = bytes(raw)
        pos = 0
        while pos + 16 <= len(buf) and len(out) < MAX_EVENTSTREAM_FRAMES:
            total_len, headers_len = struct.unpack(">II", buf[pos:pos + 8])
            if total_len < 16 or pos + total_len > len(buf) or 12 + headers_len > total_len - 4:
                break
            payload = buf[pos + 12 + headers_len:pos + total_len - 4]
            pos += total_len
            if payload:
                try:
                    obj = json.loads(payload.decode("utf-8", "replace"))
                    if isinstance(obj, dict):
                        out.extend(_unwrap_eventstream_obj(obj))
                except ValueError:
                    pass
        if out:
            return out
        text = buf.decode("utf-8", "replace")
    else:
        text = str(raw or "")
    if not text or not any(h in text for h in _EVENTSTREAM_HINTS):
        return out
    decoder = json.JSONDecoder()
    idx = 0
    while idx < len(text) and len(out) < MAX_EVENTSTREAM_FRAMES:
        brace = text.find("{", idx)
        if brace < 0:
            break
        try:
            obj, end = decoder.raw_decode(text, brace)
            idx = max(brace + 1, end)
            if isinstance(obj, dict):
                out.extend(_unwrap_eventstream_obj(obj))
        except ValueError:
            idx = brace + 1
    return out


def parse_response_body(provider: str, text: Any, sse: bool = False) -> ParsedResponse:
    """Parse a complete response body of any supported shape: JSON, a JSON array, SSE, NDJSON, or AWS EventStream."""
    if isinstance(text, (bytes, bytearray)):
        frames = extract_eventstream_objects(text)
        if frames:
            parser = SSEParser(provider)
            for c in frames:
                parser._obj(c)
            return parser.finish()
        text = bytes(text).decode("utf-8", "replace")
    raw = (text or "").strip()
    if not raw:
        return ParsedResponse()
    if sse or raw.startswith(("data:", "event:", ":")):
        parser = SSEParser(provider)
        parser.feed_text(raw)
        return parser.finish()
    try:
        obj = json.loads(raw)
    except (ValueError, RecursionError):
        obj = None
    if isinstance(obj, dict):
        return parse_response_json(provider, obj)
    chunks = obj if isinstance(obj, list) else None
    if chunks is None:  # NDJSON: one JSON object per line (Ollama, llama.cpp)
        chunks = []
        for line in raw.splitlines():
            line = line.strip()
            if line:
                try:
                    chunks.append(json.loads(line))
                except (ValueError, RecursionError):
                    chunks = None
                    break
    if chunks is None:  # AWS EventStream binary framing with embedded JSON objects
        chunks = extract_eventstream_objects(raw)
        if not chunks:
            return ParsedResponse()
    parser = SSEParser(provider)
    for c in chunks:
        if isinstance(c, dict):
            parser._obj(c)
    return parser.finish()
