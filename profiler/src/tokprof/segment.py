"""Turn raw request segments into counted segments and a call record."""

from __future__ import annotations

import hashlib
import hmac
import os
import re
import time
from functools import lru_cache
from typing import Optional

from .model import CallRecord, RawSegment, Segment, Usage
from .parse.request import ParsedRequest
from .parse.response import ParsedResponse

_BOUNDARY = re.compile(r"^(?:#{1,3}[ \t]+(?P<h>.+?)|<(?P<t>[A-Za-z_][\w:-]*)>[ \t]*)$", re.MULTILINE)
_SECRET_RE = re.compile(
    r"(?:Bearer\s+[A-Za-z0-9._\-]{16,}"
    r"|sk-(?:proj-|ant-|svcacct-)?[A-Za-z0-9_\-]{20,}"
    r"|gh[pousr]_[A-Za-z0-9]{20,}"
    r"|github_pat_[A-Za-z0-9_]{22,}"
    r"|AKIA[0-9A-Z]{16}"
    r"|AIza[0-9A-Za-z_\-]{35}"
    r"|xox[baprs]-[0-9A-Za-z\-]{10,}"
    r"|eyJ[A-Za-z0-9_\-]{16,}\.eyJ[A-Za-z0-9_\-]{16,}\.[A-Za-z0-9_\-]{16,})",
    re.I,
)
_SECRET_MARKERS = ("Bearer", "bearer", "BEARER", "sk-", "ghp_", "gho_", "ghu_", "ghs_", "ghr_", "github_pat_", "AKIA", "AIza", "xox", "eyJ")

MIN_SECTION_CHARS = 200
MAX_SEGMENT_TEXT_CHARS = 32_768
MAX_SECTION_SPLIT_CHARS = 524_288


def redact_secrets(text: str) -> str:
    if not text:
        return ""
    orig_len = len(text)
    work = text[: MAX_SEGMENT_TEXT_CHARS + 256] if orig_len > MAX_SEGMENT_TEXT_CHARS else text
    if any(m in work for m in _SECRET_MARKERS):
        work = _SECRET_RE.sub("***REDACTED***", work)
    if orig_len > MAX_SEGMENT_TEXT_CHARS or len(work) > MAX_SEGMENT_TEXT_CHARS:
        return work[:MAX_SEGMENT_TEXT_CHARS] + f"\n... [truncated, {orig_len:,} chars total]"
    return work


@lru_cache(maxsize=4096)
def _hash_cached(text: str, key: bytes) -> str:
    return hmac.digest(key, text.encode("utf-8", "replace"), "sha256")[:8].hex()


def hash_text(text: str, salt: bytes) -> str:
    """Salted hash: equal texts match within one run; the text cannot be recovered from it."""
    if not text:
        return ""
    key = bytes(salt) if salt else b"tokprof"
    if len(text) <= 65_536:
        return _hash_cached(text, key)
    return hmac.digest(key, text.encode("utf-8", "replace"), "sha256")[:8].hex()


def split_sections(text: str, min_chars: int = MIN_SECTION_CHARS) -> list:
    """Split a long system prompt at markdown headings (levels 1-3) or opening XML-style tags."""
    if not text or len(text) < min_chars or len(text) > MAX_SECTION_SPLIT_CHARS:
        return [(None, text)]
    if "#" not in text and "<" not in text:
        return [(None, text)]
    marks = list(_BOUNDARY.finditer(text))
    if len(marks) < 2 and not (marks and marks[0].start() > 0):
        return [(None, text)]
    starts = [m.start() for m in marks]
    names = [(m.group("h") or m.group("t") or "").strip() for m in marks]
    pieces: list = []
    if starts and starts[0] > 0:
        pieces.append(("(preamble)", text[: starts[0]]))
    n_starts = len(starts)
    for i, s in enumerate(starts):
        end = starts[i + 1] if i + 1 < n_starts else len(text)
        pieces.append((names[i], text[s:end]))
    merged: list = []  # [[name, [chunks], total_len], ...]
    for name, body in pieces:
        blen = len(body)
        if merged and blen < min_chars:
            merged[-1][1].append(body)
            merged[-1][2] += blen
        else:
            merged.append([name, [body], blen])
    if len(merged) > 1 and merged[0][2] < min_chars:
        merged[1][1] = merged[0][1] + merged[1][1]
        merged[1][2] += merged[0][2]
        merged.pop(0)
    if len(merged) < 2:
        return [(None, text)]
    return [(n, "".join(chunks) if len(chunks) > 1 else chunks[0]) for n, chunks, _ in merged]


def _split_components(seg: RawSegment, components: dict) -> list:
    """Carve registered component texts out of a segment so they are attributed by name."""
    if not components or seg.opaque or seg.kind not in ("system", "other") or not seg.text:
        return [seg]
    parts = [seg]
    for name, ctext in sorted(components.items(), key=lambda kv: -len(kv[1] or "")):
        ctext = (ctext or "").strip()
        if not ctext:
            continue
        nxt: list = []
        for p in parts:
            if p.kind in ("system", "other") and not p.opaque and p.name != name and ctext in p.text and len(nxt) < 64:
                before, _, after = p.text.partition(ctext)
                for piece, nm in ((before, p.name), (ctext, name), (after, p.name)):
                    if piece.strip():
                        nxt.append(RawSegment(kind=p.kind, text=piece, role=p.role, name=nm, turn=p.turn))
            else:
                nxt.append(p)
        parts = nxt
    return parts


def build_segments(
    raw: list,
    counter,
    salt: bytes,
    components: Optional[dict] = None,
    split_system: bool = True,
    include_text: Optional[bool] = None,
) -> list:
    if include_text is None:
        include_text = os.environ.get("TOKPROF_NO_CONTENT") != "1"
    out: list = []
    has_comp = bool(components)
    method = counter.method
    for r in raw:
        pieces = _split_components(r, components) if has_comp else (r,)
        for p in pieces:
            if split_system and p.kind == "system" and not p.opaque and p.name is None and len(p.text) >= MIN_SECTION_CHARS:
                sub_items = [(name, body) for name, body in split_sections(p.text)]
            else:
                sub_items = [(p.name, p.text)]
            for name, body in sub_items:
                tokens = 0 if p.opaque and not body else counter.count(body)
                out.append(
                    Segment(
                        kind=p.kind,
                        tokens=tokens,
                        chars=len(body),
                        hash=hash_text(body, salt) if body else "",
                        name=name,
                        role=p.role,
                        turn=p.turn,
                        method=method,
                        opaque=p.opaque,
                        text=redact_secrets(body) if (include_text and body) else None,
                    )
                )
    return out


def quality_label(residual: Optional[int], reported: Optional[int]) -> Optional[str]:
    """How well the counted segments explain the reported input. Thresholds are provisional."""
    if residual is None or reported is None or isinstance(reported, bool) or reported <= 0:
        return None
    ratio = abs(residual) / reported
    return "good" if ratio <= 0.05 else ("fair" if ratio <= 0.15 else "poor")


def build_record(
    *,
    parsed: ParsedRequest,
    response: Optional[ParsedResponse],
    counter,
    salt: bytes,
    components: Optional[dict] = None,
    endpoint: str = "",
    status: Optional[int] = None,
    latency_ms: Optional[int] = None,
    ttfb_ms: Optional[int] = None,
    run: Optional[str] = None,
    error: Optional[str] = None,
    ts: Optional[float] = None,
    include_text: Optional[bool] = None,
) -> CallRecord:
    if include_text is None:
        include_text = os.environ.get("TOKPROF_NO_CONTENT") != "1"
    segments = build_segments(parsed.segments, counter, salt, components, include_text=include_text)
    est = sum(s.tokens for s in segments)
    usage = response.usage if response and response.has_usage else Usage()
    reported = usage.input_total
    residual = (reported - est) if (reported is not None and not isinstance(reported, bool)) else None

    sys_raw = [s.text for s in parsed.segments if s.kind == "system" and s.text]
    if not sys_raw:
        sysfp = None
    elif len(sys_raw) == 1 and len(segments) == 1 and segments[0].kind == "system":
        sysfp = segments[0].hash or hash_text(sys_raw[0], salt)
    else:
        sysfp = hash_text("\n".join(sys_raw) if len(sys_raw) > 1 else sys_raw[0], salt)

    first_non_system = next((s for s in parsed.segments if s.kind in ("history_user", "user_prompt")), None)
    sys_label = next((s.name for s in segments if s.kind == "system" and s.name), None) or parsed.step_hint
    if parsed.conv_hint:
        conv_id = hash_text(parsed.conv_hint, salt)
    elif first_non_system and first_non_system.text:
        first_line = first_non_system.text.partition("\n")[0]
        conv_id = hash_text(first_line or first_non_system.text, salt)
    else:
        conv_id = None

    output_est = None
    resp_text = None
    if response is not None and response.text:
        output_est = counter.count(response.text)
        if include_text:
            resp_text = redact_secrets(response.text)

    return CallRecord(
        id=os.urandom(6).hex(),
        ts=float(ts) if ts is not None else time.time(),
        run=run,
        provider=parsed.provider,
        model=parsed.model,
        endpoint=endpoint,
        stream=parsed.stream,
        status=status,
        latency_ms=latency_ms,
        ttfb_ms=ttfb_ms,
        conv=conv_id,
        sysfp=sysfp,
        sys_label=sys_label,
        usage=usage.to_clean_dict(),
        segments=[s.to_dict() for s in segments],
        est_input=est,
        residual=residual,
        quality=quality_label(residual, reported),
        tools_defined=[t for t in parsed.tools if t],
        tool_calls=list(response.tool_calls) if response else [],
        output_est=output_est,
        response_text=resp_text,
        error=error or (response.error if response else None),
    )
