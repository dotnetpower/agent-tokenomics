"""Request parsing: provider detection and decomposition into raw segments."""

from __future__ import annotations

import json
from dataclasses import dataclass, field
from typing import Any, Optional

from ..model import RawSegment

OPENAI_CHAT = "openai_chat"
OPENAI_RESPONSES = "openai_responses"
ANTHROPIC = "anthropic"

_TEXT_PARTS = {"text", "input_text", "output_text"}
_OPAQUE_PARTS = {"image_url", "input_image", "image", "input_file", "file", "document", "input_audio", "audio"}


@dataclass
class ParsedRequest:
    provider: str
    model: Optional[str]
    stream: bool
    segments: list = field(default_factory=list)
    tools: list = field(default_factory=list)


def detect_provider(path: str, body: Any) -> Optional[str]:
    """Identify the API from the URL path, falling back to the JSON shape."""
    p = (path or "").split("?", 1)[0].rstrip("/")
    if p.endswith("/chat/completions"):
        return OPENAI_CHAT
    if p.endswith("/responses"):
        return OPENAI_RESPONSES
    if p.endswith("/messages") and not p.endswith("/count_tokens"):
        return ANTHROPIC
    if isinstance(body, dict) and p.endswith(("/completions", "/embeddings")) is False:
        # Shape-based fallback only for endpoints we did not recognize by path.
        if "messages" in body and "model" in body:
            return ANTHROPIC if ("max_tokens" in body and "system" in body or _looks_anthropic(body)) else OPENAI_CHAT
    return None


def _looks_anthropic(body: dict) -> bool:
    msgs = body.get("messages")
    if not isinstance(msgs, list):
        return False
    for m in msgs:
        c = m.get("content") if isinstance(m, dict) else None
        if isinstance(c, list) and any(isinstance(b, dict) and b.get("type") in ("tool_use", "tool_result") for b in c):
            return True
    return False


def _dumps(obj: Any) -> str:
    return json.dumps(obj, ensure_ascii=False, separators=(",", ":"), sort_keys=True)


def _content_text(content: Any) -> tuple[str, bool]:
    """Return (text, has_opaque) for a string or list-of-parts content."""
    if content is None:
        return "", False
    if isinstance(content, str):
        return content, False
    if isinstance(content, list):
        texts, opaque = [], False
        for part in content:
            if isinstance(part, str):
                texts.append(part)
            elif isinstance(part, dict):
                t = part.get("type")
                if t in _TEXT_PARTS and isinstance(part.get("text"), str):
                    texts.append(part["text"])
                elif t in _OPAQUE_PARTS:
                    opaque = True
                elif isinstance(part.get("text"), str):
                    texts.append(part["text"])
        return "\n".join(texts), opaque
    return _dumps(content), False


def _add(segments: list, kind: str, text: str, *, role=None, name=None, turn=None, opaque=False) -> None:
    if text or opaque:
        segments.append(RawSegment(kind=kind, text=text, role=role, name=name, turn=turn, opaque=opaque))


def _add_message(segments: list, role: str, content: Any, *, is_current: bool, turn: int) -> None:
    text, opaque = _content_text(content)
    if role in ("system", "developer"):
        _add(segments, "system", text, role=role, turn=turn)
    elif role == "user":
        _add(segments, "user_prompt" if is_current else "history_user", text, role=role, turn=turn)
    elif role == "assistant":
        _add(segments, "history_assistant", text, role=role, turn=turn)
    else:
        _add(segments, "other", text, role=role, turn=turn)
    if opaque:
        segments.append(RawSegment(kind="attachment", text="", role=role, turn=turn, opaque=True))


def _parse_openai_chat(body: dict) -> ParsedRequest:
    segs: list = []
    msgs = [m for m in (body.get("messages") or []) if isinstance(m, dict)]
    last_user = max((i for i, m in enumerate(msgs) if m.get("role") == "user"), default=-1)
    for i, m in enumerate(msgs):
        role = m.get("role") or ""
        if role in ("tool", "function"):
            text, opaque = _content_text(m.get("content"))
            _add(segs, "tool_result", text, role=role, name=m.get("name") or m.get("tool_call_id"), turn=i, opaque=opaque)
            continue
        _add_message(segs, role, m.get("content"), is_current=(i == last_user), turn=i)
        if role == "assistant":
            for tc in m.get("tool_calls") or []:
                fn = (tc or {}).get("function") or {}
                _add(segs, "tool_call", f"{fn.get('name', '')}({fn.get('arguments', '')})", role="assistant", name=fn.get("name"), turn=i)
            fc = m.get("function_call")
            if isinstance(fc, dict):
                _add(segs, "tool_call", f"{fc.get('name', '')}({fc.get('arguments', '')})", role="assistant", name=fc.get("name"), turn=i)
    tools: list = []
    for t in body.get("tools") or []:
        fn = (t or {}).get("function") or t or {}
        name = fn.get("name") or (t or {}).get("type")
        tools.append(name)
        _add(segs, "tool_def", _dumps(t), name=name)
    for fn in body.get("functions") or []:
        name = (fn or {}).get("name")
        tools.append(name)
        _add(segs, "tool_def", _dumps(fn), name=name)
    rf = body.get("response_format")
    if isinstance(rf, dict) and rf.get("type") == "json_schema":
        _add(segs, "response_schema", _dumps(rf))
    return ParsedRequest(OPENAI_CHAT, body.get("model"), bool(body.get("stream")), segs, tools)


def _parse_openai_responses(body: dict) -> ParsedRequest:
    segs: list = []
    if isinstance(body.get("instructions"), str):
        _add(segs, "system", body["instructions"], role="system")
    inp = body.get("input")
    if isinstance(inp, str):
        _add(segs, "user_prompt", inp, role="user", turn=0)
    elif isinstance(inp, list):
        items = [it for it in inp if isinstance(it, dict)]
        last_user = max((i for i, it in enumerate(items) if it.get("role") == "user"), default=-1)
        for i, it in enumerate(items):
            t = it.get("type")
            if t == "function_call":
                _add(segs, "tool_call", f"{it.get('name', '')}({it.get('arguments', '')})", role="assistant", name=it.get("name"), turn=i)
            elif t == "function_call_output":
                out = it.get("output")
                _add(segs, "tool_result", out if isinstance(out, str) else _dumps(out), name=it.get("call_id"), turn=i)
            elif t == "reasoning":
                _add(segs, "other", _dumps(it.get("summary") or ""), role="assistant", name="reasoning", turn=i)
            elif it.get("role"):
                _add_message(segs, it["role"], it.get("content"), is_current=(i == last_user), turn=i)
            else:
                _add(segs, "other", _dumps(it), turn=i)
    tools: list = []
    for t in body.get("tools") or []:
        name = (t or {}).get("name") or (t or {}).get("type")
        tools.append(name)
        _add(segs, "tool_def", _dumps(t), name=name)
    fmt = (body.get("text") or {}).get("format") if isinstance(body.get("text"), dict) else None
    if isinstance(fmt, dict) and fmt.get("type") == "json_schema":
        _add(segs, "response_schema", _dumps(fmt))
    return ParsedRequest(OPENAI_RESPONSES, body.get("model"), bool(body.get("stream")), segs, tools)


def _parse_anthropic(body: dict) -> ParsedRequest:
    segs: list = []
    system = body.get("system")
    if isinstance(system, str):
        _add(segs, "system", system, role="system")
    elif isinstance(system, list):
        text, _ = _content_text(system)
        _add(segs, "system", text, role="system")
    msgs = [m for m in (body.get("messages") or []) if isinstance(m, dict)]

    def has_text(m: dict) -> bool:
        c = m.get("content")
        if isinstance(c, str):
            return bool(c)
        return isinstance(c, list) and any(isinstance(b, dict) and b.get("type") == "text" for b in c)

    last_user = max((i for i, m in enumerate(msgs) if m.get("role") == "user" and has_text(m)), default=-1)
    for i, m in enumerate(msgs):
        role, content = m.get("role") or "", m.get("content")
        if isinstance(content, str):
            _add_message(segs, role, content, is_current=(i == last_user), turn=i)
            continue
        for b in content or []:
            if not isinstance(b, dict):
                continue
            t = b.get("type")
            if t == "text":
                _add_message(segs, role, b.get("text", ""), is_current=(i == last_user and role == "user"), turn=i)
            elif t == "tool_use":
                _add(segs, "tool_call", f"{b.get('name', '')}({_dumps(b.get('input'))})", role="assistant", name=b.get("name"), turn=i)
            elif t == "tool_result":
                text, opaque = _content_text(b.get("content"))
                _add(segs, "tool_result", text, name=b.get("tool_use_id"), turn=i, opaque=opaque)
            elif t in ("thinking", "redacted_thinking"):
                _add(segs, "other", b.get("thinking") or "", role="assistant", name="thinking", turn=i)
            elif t in _OPAQUE_PARTS:
                segs.append(RawSegment(kind="attachment", text="", role=role, turn=i, opaque=True))
    tools: list = []
    for t in body.get("tools") or []:
        name = (t or {}).get("name") or (t or {}).get("type")
        tools.append(name)
        _add(segs, "tool_def", _dumps(t), name=name)
    return ParsedRequest(ANTHROPIC, body.get("model"), bool(body.get("stream")), segs, tools)


def parse_request(provider: str, body: dict) -> ParsedRequest:
    if provider == OPENAI_CHAT:
        return _parse_openai_chat(body)
    if provider == OPENAI_RESPONSES:
        return _parse_openai_responses(body)
    if provider == ANTHROPIC:
        return _parse_anthropic(body)
    raise ValueError(f"unknown provider: {provider}")
