"""Response parsing: usage, tool-call names, and streamed (SSE) responses."""

from __future__ import annotations

import json
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
    return v if isinstance(v, int) and not isinstance(v, bool) else None


def usage_openai_chat(u: dict) -> Usage:
    d = u.get("prompt_tokens_details") or {}
    c = u.get("completion_tokens_details") or {}
    return Usage(
        input_total=_int(u.get("prompt_tokens")),
        cached=_int(d.get("cached_tokens")),
        cache_write=_int(d.get("cache_write_tokens")),
        output=_int(u.get("completion_tokens")),
        reasoning=_int(c.get("reasoning_tokens")),
    )


def usage_openai_responses(u: dict) -> Usage:
    d = u.get("input_tokens_details") or {}
    c = u.get("output_tokens_details") or {}
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
    return Usage(
        input_total=_int(u.get("promptTokenCount")),
        cached=_int(u.get("cachedContentTokenCount")),
        output=_int(u.get("candidatesTokenCount")),
        reasoning=_int(u.get("thoughtsTokenCount")),
    )


def native_usage(o: dict) -> Optional[Usage]:
    """Usage in the formats of self-hosted servers that do not return an OpenAI-style `usage` object."""
    if _int(o.get("prompt_eval_count")) is not None or _int(o.get("eval_count")) is not None:  # Ollama
        return Usage(input_total=_int(o.get("prompt_eval_count")), output=_int(o.get("eval_count")))
    if _int(o.get("tokens_evaluated")) is not None:  # llama.cpp server
        return Usage(input_total=o["tokens_evaluated"], cached=_int(o.get("tokens_cached")), output=_int(o.get("tokens_predicted")))
    details = o.get("details")
    if isinstance(details, dict) and isinstance(details.get("prefill"), list):  # TGI /generate with details=true
        return Usage(input_total=len(details["prefill"]), output=_int(details.get("generated_tokens")))
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
    if provider in (COMPLETIONS, OPENAI_CHAT):
        nu = native_usage(o)  # Ollama and llama.cpp put the counts on the last chunk only
        if nu is not None:
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
        else:  # Ollama /api/chat
            msg = o.get("message")
            if isinstance(msg, dict) and "choices" not in o:
                if isinstance(msg.get("content"), str):
                    r.text += msg["content"]
                for tc in msg.get("tool_calls") or []:
                    name = ((tc or {}).get("function") or {}).get("name")
                    if name:
                        r.tool_calls.append(name)


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
    if provider in (COMPLETIONS, GEMINI) or (provider == OPENAI_CHAT and "choices" not in obj):
        _apply_extra(provider, obj, r)
    return r


class SSEParser:
    """Incremental parser for server-sent-event responses."""

    def __init__(self, provider: str):
        self.provider = provider
        self._buf = b""
        self.result = ParsedResponse()

    def feed(self, data: bytes) -> None:
        self._buf += data
        while True:
            for sep in (b"\r\n\r\n", b"\n\n"):
                idx = self._buf.find(sep)
                if idx >= 0:
                    event, self._buf = self._buf[:idx], self._buf[idx + len(sep):]
                    self._event(event)
                    break
            else:
                return

    def finish(self) -> ParsedResponse:
        if self._buf.strip():
            self._event(self._buf)
            self._buf = b""
        return self.result

    def _event(self, raw: bytes) -> None:
        payload = []
        for line in raw.replace(b"\r\n", b"\n").split(b"\n"):
            if line.startswith(b"data:"):
                payload.append(line[5:].strip())
        if not payload:
            return
        text = b"\n".join(payload)
        if text == b"[DONE]":
            return
        try:
            obj = json.loads(text)
        except ValueError:
            return
        if isinstance(obj, dict):
            self._obj(obj)

    def _obj(self, o: dict) -> None:
        r, p = self.result, self.provider
        if p == OPENAI_CHAT:
            if isinstance(o.get("usage"), dict):
                r.usage, r.has_usage = usage_openai_chat(o["usage"]), True
            for ch in o.get("choices") or []:
                delta = (ch or {}).get("delta") or {}
                if isinstance(delta.get("content"), str):
                    r.text += delta["content"]
                for tc in delta.get("tool_calls") or []:
                    name = ((tc or {}).get("function") or {}).get("name")
                    if name:
                        r.tool_calls.append(name)
        elif p == OPENAI_RESPONSES:
            t = o.get("type", "")
            if t == "response.output_text.delta" and isinstance(o.get("delta"), str):
                r.text += o["delta"]
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
                    r.text += d["text"]
            elif t == "message_delta" and isinstance(o.get("usage"), dict):
                usage_anthropic(o["usage"], r.usage)
                r.has_usage = True
        if p in (COMPLETIONS, GEMINI) or (p == OPENAI_CHAT and "choices" not in o):
            _apply_extra(p, o, r)


def parse_response_body(provider: str, text: str, sse: bool = False) -> ParsedResponse:
    """Parse a complete response body of any supported shape: JSON, a JSON array, SSE, or NDJSON."""
    raw = (text or "").strip()
    if not raw:
        return ParsedResponse()
    if sse or raw.startswith(("data:", "event:", ":")):
        parser = SSEParser(provider)
        parser.feed(raw.encode("utf-8", "replace") + b"\n\n")
        return parser.finish()
    try:
        obj = json.loads(raw)
    except ValueError:
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
                except ValueError:
                    return ParsedResponse()
    parser = SSEParser(provider)
    for c in chunks:
        if isinstance(c, dict):
            parser._obj(c)
    return parser.result
