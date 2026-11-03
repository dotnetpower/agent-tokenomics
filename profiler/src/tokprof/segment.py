"""Turn raw request segments into counted segments and a call record."""

from __future__ import annotations

import hashlib
import hmac
import re
import time
import uuid
from typing import Optional

from .model import CallRecord, RawSegment, Segment, Usage
from .parse.request import ParsedRequest
from .parse.response import ParsedResponse

_BOUNDARY = re.compile(r"^(?:#{1,3}[ \t]+(?P<h>.+?)|<(?P<t>[A-Za-z_][\w:-]*)>[ \t]*)$", re.MULTILINE)

MIN_SECTION_CHARS = 200


def hash_text(text: str, salt: bytes) -> str:
    """Salted hash: equal texts match within one run; the text cannot be recovered from it."""
    return hmac.new(salt, text.encode("utf-8", "replace"), hashlib.sha256).hexdigest()[:16]


def split_sections(text: str, min_chars: int = MIN_SECTION_CHARS) -> list:
    """Split a long system prompt at markdown headings (levels 1-3) or opening XML-style tags."""
    marks = list(_BOUNDARY.finditer(text))
    if len(marks) < 2 and not (marks and marks[0].start() > 0):
        return [(None, text)]
    starts = [m.start() for m in marks]
    names = [(m.group("h") or m.group("t") or "").strip() for m in marks]
    pieces: list = []
    if starts and starts[0] > 0:
        pieces.append(["(preamble)", text[: starts[0]]])
    for i, s in enumerate(starts):
        end = starts[i + 1] if i + 1 < len(starts) else len(text)
        pieces.append([names[i], text[s:end]])
    merged: list = []
    for name, body in pieces:
        if merged and len(body) < min_chars:
            merged[-1][1] += body
        else:
            merged.append([name, body])
    if len(merged) > 1 and len(merged[0][1]) < min_chars:
        merged[1][1] = merged[0][1] + merged[1][1]
        merged.pop(0)
    if len(merged) < 2:
        return [(None, text)]
    return [(n, b) for n, b in merged]


def _split_components(seg: RawSegment, components: dict) -> list:
    """Carve registered component texts out of a segment so they are attributed by name."""
    parts = [seg]
    for name, ctext in components.items():
        ctext = (ctext or "").strip()
        if not ctext:
            continue
        nxt: list = []
        for p in parts:
            if p.kind in ("system", "other") and not p.opaque and ctext in p.text:
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
) -> list:
    out: list = []
    for r in raw:
        pieces = _split_components(r, components or {})
        expanded: list = []
        for p in pieces:
            if split_system and p.kind == "system" and not p.opaque and p.name is None:
                for name, body in split_sections(p.text):
                    expanded.append(RawSegment(kind="system", text=body, role=p.role, name=name, turn=p.turn))
            else:
                expanded.append(p)
        for p in expanded:
            tokens = 0 if p.opaque and not p.text else counter.count(p.text)
            out.append(
                Segment(
                    kind=p.kind,
                    tokens=tokens,
                    chars=len(p.text),
                    hash=hash_text(p.text, salt) if p.text else "",
                    name=p.name,
                    role=p.role,
                    turn=p.turn,
                    method=counter.method,
                    opaque=p.opaque,
                )
            )
    return out


def quality_label(residual: Optional[int], reported: Optional[int]) -> Optional[str]:
    """How well the counted segments explain the reported input. Thresholds are provisional."""
    if residual is None or not reported:
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
) -> CallRecord:
    segments = build_segments(parsed.segments, counter, salt, components)
    est = sum(s.tokens for s in segments)
    usage = response.usage if response and response.has_usage else Usage()
    reported = usage.input_total
    residual = (reported - est) if reported is not None else None

    system_text = "\n".join(s.text for s in parsed.segments if s.kind == "system")
    first_non_system = next((s for s in parsed.segments if s.kind in ("user_prompt", "history_user")), None)
    sys_label = next((s.name for s in segments if s.kind == "system" and s.name), None)

    output_est = None
    if response is not None and response.text:
        output_est = counter.count(response.text)

    return CallRecord(
        id=uuid.uuid4().hex[:12],
        ts=ts if ts is not None else time.time(),
        run=run,
        provider=parsed.provider,
        model=parsed.model,
        endpoint=endpoint,
        stream=parsed.stream,
        status=status,
        latency_ms=latency_ms,
        ttfb_ms=ttfb_ms,
        conv=hash_text(first_non_system.text, salt) if first_non_system else None,
        sysfp=hash_text(system_text, salt) if system_text else None,
        sys_label=sys_label,
        usage={k: v for k, v in vars(usage).items() if v is not None},
        segments=[vars(s) for s in segments],
        est_input=est,
        residual=residual,
        quality=quality_label(residual, reported),
        tools_defined=[t for t in parsed.tools if t],
        tool_calls=list(response.tool_calls) if response else [],
        output_est=output_est,
        error=error or (response.error if response else None),
    )
