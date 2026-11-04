"""Request parsing: provider detection and decomposition into raw segments."""

from __future__ import annotations

import json
import re
from dataclasses import dataclass, field
from typing import Any, Optional

from ..model import RawSegment

OPENAI_CHAT = "openai_chat"
OPENAI_RESPONSES = "openai_responses"
ANTHROPIC = "anthropic"
COMPLETIONS = "completions"  # prompt-in/text-out: vLLM /v1/completions, TGI, llama.cpp, Ollama /api/generate
GEMINI = "gemini"

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
    """Identify the API of one request from its path and JSON shape (see tokprof.detect for the full scoring)."""
    from ..detect import EXCLUDED_PATH, Detector, normalize_path, path_hint

    hint = path_hint(path)
    if hint in (OPENAI_CHAT, OPENAI_RESPONSES, ANTHROPIC) and not EXCLUDED_PATH.search(normalize_path(path)):
        return hint  # a well-known path decides even when the body is empty or unusual
    return Detector().inspect(method="POST", host="", port=0, path=path, body=body).api


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


def _norm_role(role: Any) -> str:
    r = str(role or "").lower().strip()
    if r in ("chatbot", "ai", "model"):
        return "assistant"
    if r == "human":
        return "user"
    return r


def _add_message(segments: list, role: str, content: Any, *, is_current: bool, turn: int) -> None:
    r = _norm_role(role)
    text, opaque = _content_text(content)
    if r in ("system", "developer"):
        _add(segments, "system", text, role=r, turn=turn)
    elif r == "user":
        _add(segments, "user_prompt" if is_current else "history_user", text, role=r, turn=turn)
    elif r == "assistant":
        _add(segments, "history_assistant", text, role=r, turn=turn)
    else:
        _add(segments, "other", text, role=r, turn=turn)
    if opaque:
        segments.append(RawSegment(kind="attachment", text="", role=r, turn=turn, opaque=True))


def _parse_openai_chat(body: dict) -> ParsedRequest:
    segs: list = []
    msgs = [m for m in (body.get("messages") or []) if isinstance(m, dict)]
    # Bedrock Converse, Ollama and Cohere v1 carry the system prompt at the top level.
    for sys_key in ("system", "preamble"):
        top_system = body.get(sys_key)
        if isinstance(top_system, (str, list)):
            _add(segs, "system", _content_text(top_system)[0], role="system")
    if not msgs and isinstance(body.get("chat_history"), list):
        for h in body["chat_history"]:
            if isinstance(h, dict):
                msgs.append({"role": h.get("role"), "content": h.get("message") if "message" in h else h.get("content") or h.get("text")})
    if isinstance(body.get("message"), str) and body["message"]:
        msgs.append({"role": "user", "content": body["message"]})
    last_user = max((i for i, m in enumerate(msgs) if _norm_role(m.get("role")) == "user"), default=-1)
    for i, m in enumerate(msgs):
        role = _norm_role(m.get("role"))
        content = m.get("content") if "content" in m else m.get("message")
        if role in ("tool", "function"):
            text, opaque = _content_text(content)
            _add(segs, "tool_result", text, role=role, name=m.get("name") or m.get("tool_call_id"), turn=i, opaque=opaque)
            continue
        if isinstance(content, list) and any(isinstance(b, dict) and ("toolUse" in b or "toolResult" in b) for b in content):
            for b in content:
                if not isinstance(b, dict):
                    continue
                if "toolUse" in b and isinstance(b["toolUse"], dict):
                    tu = b["toolUse"]
                    _add(segs, "tool_call", f"{tu.get('name', '')}({_dumps(tu.get('input'))})", role="assistant", name=tu.get("name"), turn=i)
                elif "toolResult" in b and isinstance(b["toolResult"], dict):
                    tr = b["toolResult"]
                    t_text, opaque = _content_text(tr.get("content"))
                    _add(segs, "tool_result", t_text, role="tool", name=tr.get("toolUseId"), turn=i, opaque=opaque)
                elif isinstance(b.get("text"), str):
                    _add_message(segs, role, b["text"], is_current=(i == last_user), turn=i)
        else:
            _add_message(segs, role, content, is_current=(i == last_user), turn=i)
        if role == "assistant":
            for tc in m.get("tool_calls") or []:
                fn = (tc or {}).get("function") or tc or {}
                args = fn.get("arguments") if "arguments" in fn else fn.get("parameters")
                _add(segs, "tool_call", f"{fn.get('name', '')}({args if isinstance(args, str) else _dumps(args)})", role="assistant", name=fn.get("name"), turn=i)
            fc = m.get("function_call")
            if isinstance(fc, dict):
                _add(segs, "tool_call", f"{fc.get('name', '')}({fc.get('arguments', '')})", role="assistant", name=fc.get("name"), turn=i)
    tools: list = []
    for t in body.get("tools") or []:
        fn = (t or {}).get("function") or (t or {}).get("toolSpec") or t or {}
        name = fn.get("name") or (t or {}).get("type")
        tools.append(name)
        _add(segs, "tool_def", _dumps(t), name=name)
    tc_cfg = body.get("toolConfig")
    if isinstance(tc_cfg, dict):
        for t in tc_cfg.get("tools") or []:
            spec = (t or {}).get("toolSpec") or t or {}
            name = spec.get("name")
            tools.append(name)
            _add(segs, "tool_def", _dumps(t), name=name)
    for fn in body.get("functions") or []:
        name = (fn or {}).get("name")
        tools.append(name)
        _add(segs, "tool_def", _dumps(fn), name=name)
    rf = body.get("response_format")
    if isinstance(rf, dict) and rf.get("type") in ("json_schema", "json_object") and ("json_schema" in rf or "schema" in rf):
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


_TEMPLATES = (
    # (header regex with groups role/…, end marker regex)
    (re.compile(r"<\|im_start\|>(system|user|assistant|tool)\n"), re.compile(r"<\|im_end\|>")),  # ChatML (Qwen, many fine-tunes)
    (re.compile(r"<\|start_header_id\|>(system|user|assistant|ipython)<\|end_header_id\|>\n\n"), re.compile(r"<\|eot_id\|>")),  # Llama 3
    (re.compile(r"<start_of_turn>(user|model)\n"), re.compile(r"<end_of_turn>")),  # Gemma
    (re.compile(r"<｜(System|User|Assistant)｜>\s*", re.I), re.compile(r"<｜end[▁_]of[▁_]sentence｜>", re.I)),  # DeepSeek
    (re.compile(r"<\|(system|user|assistant|tool)\|>\s*\n?", re.I), re.compile(r"<\|(?:end|eot_id|end_of_sentence)\|>", re.I)),  # Phi-3 / Phi-4
    (re.compile(r"<\|START_OF_TURN_TOKEN\|><\|(SYSTEM|USER|CHATBOT|TOOL)_TOKEN\|>", re.I), re.compile(r"<\|END_OF_TURN_TOKEN\|>", re.I)),  # Cohere Command-R
)
_ROLE_ALIASES = {"model": "assistant", "ipython": "tool", "chatbot": "assistant"}
_LLAMA2_SYS = re.compile(r"<<SYS>>(.*?)<</SYS>>", re.S)
_INST = re.compile(r"\[INST\](.*?)\[/INST\]", re.S)


def split_chat_template(prompt: str) -> Optional[list]:
    """Split a flat prompt that was already rendered with a known chat template into (role, text) turns.

    Returns None when no known template is recognized, so the caller reports one raw prompt instead of guessing.
    """
    for header, end in _TEMPLATES:
        heads = list(header.finditer(prompt))
        if not heads:
            continue
        turns = []
        for i, h in enumerate(heads):
            stop = heads[i + 1].start() if i + 1 < len(heads) else len(prompt)
            chunk = prompt[h.end():stop]
            m = end.search(chunk)
            chunk = chunk[:m.start()] if m else chunk
            raw_role = h.group(1).lower()
            role = _ROLE_ALIASES.get(raw_role, raw_role)
            if chunk.strip():
                turns.append((role, chunk.strip("\n")))
        return turns or None
    if "[INST]" in prompt:  # Llama 2 / Mistral
        turns, pos = [], 0
        sys_m = _LLAMA2_SYS.search(prompt)
        if sys_m:
            turns.append(("system", sys_m.group(1).strip()))
        for m in _INST.finditer(prompt):
            if m.start() > pos and pos:
                reply = prompt[pos:m.start()].replace("</s>", "").replace("<s>", "").strip()
                if reply:
                    turns.append(("assistant", reply))
            user = _LLAMA2_SYS.sub("", m.group(1)).strip()
            if user:
                turns.append(("user", user))
            pos = m.end()
        tail = prompt[pos:].replace("</s>", "").strip() if pos else ""
        if tail:
            turns.append(("assistant", tail))
        return turns or None
    return None


def _parse_completions(body: dict) -> ParsedRequest:
    segs: list = []
    if isinstance(body.get("system"), str):
        _add(segs, "system", body["system"], role="system")
    prompt = body.get("prompt")
    if prompt is None:
        prompt = body.get("inputs")
    if isinstance(prompt, list):
        if prompt and all(isinstance(x, str) for x in prompt):
            prompt = "\n".join(prompt)  # a batch: counted together, the provider reports the sum
        else:
            segs.append(RawSegment(kind="raw_prompt", text="", opaque=True))  # pre-tokenized ids cannot be counted as text
            prompt = None
    if isinstance(prompt, str) and prompt:
        turns = split_chat_template(prompt)
        if turns is None:
            _add(segs, "raw_prompt", prompt)
        else:
            last_user = max((i for i, (r, _) in enumerate(turns) if r == "user"), default=-1)
            for i, (role, text) in enumerate(turns):
                _add_message(segs, role if role != "tool" else "other", text, is_current=(i == last_user), turn=i)
    model = body.get("model")
    stream = body.get("stream")
    if stream is None:
        params = body.get("parameters")
        stream = bool(params.get("stream")) if isinstance(params, dict) else False
    return ParsedRequest(COMPLETIONS, model if isinstance(model, str) else None, bool(stream), segs, [])


def _parts_text(parts: Any) -> tuple:
    texts, opaque = [], False
    for p in parts if isinstance(parts, list) else []:
        if isinstance(p, dict):
            if isinstance(p.get("text"), str):
                texts.append(p["text"])
            elif "inlineData" in p or "fileData" in p or "inline_data" in p or "file_data" in p:
                opaque = True
        elif isinstance(p, str):
            texts.append(p)
    return "\n".join(texts), opaque


def _parse_gemini(body: dict) -> ParsedRequest:
    segs: list = []
    si = body.get("systemInstruction") or body.get("system_instruction")
    if isinstance(si, str):
        _add(segs, "system", si, role="system")
    elif isinstance(si, dict):
        _add(segs, "system", _parts_text(si.get("parts"))[0], role="system")
    contents = [c for c in (body.get("contents") or []) if isinstance(c, dict)]
    last_user = max((i for i, c in enumerate(contents) if (c.get("role") or "user") == "user"
                     and not any(isinstance(p, dict) and ("functionResponse" in p or "function_response" in p) for p in (c.get("parts") or []))), default=-1)
    for i, c in enumerate(contents):
        role = c.get("role") or "user"
        for part in c.get("parts") or []:
            if not isinstance(part, dict):
                continue
            fc = part.get("functionCall") or part.get("function_call")
            fr = part.get("functionResponse") or part.get("function_response")
            if isinstance(fc, dict):
                _add(segs, "tool_call", f"{fc.get('name', '')}({_dumps(fc.get('args'))})", role="assistant", name=fc.get("name"), turn=i)
            elif isinstance(fr, dict):
                _add(segs, "tool_result", _dumps(fr.get("response")), name=fr.get("name"), turn=i)
            else:
                text, opaque = _parts_text([part])
                _add_message(segs, "assistant" if role == "model" else role, text, is_current=(i == last_user), turn=i)
                if opaque:
                    segs.append(RawSegment(kind="attachment", text="", role=role, turn=i, opaque=True))
    tools: list = []
    for t in body.get("tools") or []:
        if not isinstance(t, dict):
            continue
        decls = t.get("functionDeclarations") or t.get("function_declarations")
        if isinstance(decls, list):
            for d in decls:
                name = (d or {}).get("name")
                tools.append(name)
                _add(segs, "tool_def", _dumps(d), name=name)
        else:
            name = next(iter(t), "tool")
            tools.append(name)
            _add(segs, "tool_def", _dumps(t), name=name)
    gc = body.get("generationConfig") or body.get("generation_config")
    if isinstance(gc, dict) and (gc.get("responseSchema") or gc.get("response_schema")):
        _add(segs, "response_schema", _dumps(gc.get("responseSchema") or gc.get("response_schema")))
    return ParsedRequest(GEMINI, body.get("model") if isinstance(body.get("model"), str) else None, False, segs, tools)


def parse_request(provider: str, body: dict) -> ParsedRequest:
    if provider == OPENAI_CHAT:
        return _parse_openai_chat(body)
    if provider == OPENAI_RESPONSES:
        return _parse_openai_responses(body)
    if provider == ANTHROPIC:
        return _parse_anthropic(body)
    if provider == COMPLETIONS:
        return _parse_completions(body)
    if provider == GEMINI:
        return _parse_gemini(body)
    raise ValueError(f"unknown provider: {provider}")
