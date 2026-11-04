"""Data model shared by capture, analysis, and reports."""

from __future__ import annotations

import sys
from dataclasses import dataclass, field
from typing import Optional

SCHEMA_VERSION = 1
_SLOTS = {"slots": True} if sys.version_info >= (3, 10) else {}

# Segment kinds. "residual" is not a segment: it is reported usage minus the sum of counted segments
# (template overhead, opaque content such as images, hidden provider tokens).
KINDS = (
    "system",
    "history_user",
    "history_assistant",
    "user_prompt",
    "tool_def",
    "tool_call",
    "tool_result",
    "attachment",
    "response_schema",
    "raw_prompt",
    "other",
)

TOOL_KINDS = ("tool_def", "tool_call", "tool_result")


@dataclass(**_SLOTS)
class RawSegment:
    """A piece of a request before token counting."""

    kind: str
    text: str
    role: Optional[str] = None
    name: Optional[str] = None
    turn: Optional[int] = None
    opaque: bool = False  # content we cannot count locally (images, files, audio)


@dataclass(**_SLOTS)
class Segment:
    kind: str
    tokens: int
    chars: int
    hash: str
    name: Optional[str] = None
    role: Optional[str] = None
    turn: Optional[int] = None
    method: str = "heuristic"
    opaque: bool = False
    text: Optional[str] = None

    def to_dict(self) -> dict:
        return {
            "kind": self.kind,
            "tokens": self.tokens,
            "chars": self.chars,
            "hash": self.hash,
            "name": self.name,
            "role": self.role,
            "turn": self.turn,
            "method": self.method,
            "opaque": self.opaque,
            "text": self.text,
        }


@dataclass(**_SLOTS)
class Usage:
    """Provider-reported usage, normalized. input_total includes cached tokens."""

    input_total: Optional[int] = None
    cached: Optional[int] = None
    cache_write: Optional[int] = None
    output: Optional[int] = None
    reasoning: Optional[int] = None

    def to_clean_dict(self) -> dict:
        out: dict = {}
        for k in ("input_total", "cached", "cache_write", "output", "reasoning"):
            v = getattr(self, k)
            if isinstance(v, int) and not isinstance(v, bool) and v >= 0:
                out[k] = v
        return out


@dataclass
class CallRecord:
    v: int = SCHEMA_VERSION
    id: str = ""
    ts: float = 0.0
    run: Optional[str] = None
    provider: str = ""
    model: Optional[str] = None
    endpoint: str = ""
    host_class: Optional[str] = None  # provider:<name> | loopback | private-ip | private-dns | local-name | public
    host_id: Optional[str] = None  # salted hash of host:port, so endpoints can be told apart without storing the name
    host: Optional[str] = None  # only with --record-hosts
    detect: Optional[dict] = None  # why this call was classified as an LLM call
    stream: bool = False
    status: Optional[int] = None
    latency_ms: Optional[int] = None
    ttfb_ms: Optional[int] = None
    conv: Optional[str] = None  # hash of the first non-system message: groups one conversation
    sysfp: Optional[str] = None  # hash of the system prompt: identifies a pipeline step
    sys_label: Optional[str] = None
    usage: dict = field(default_factory=dict)
    segments: list = field(default_factory=list)
    est_input: int = 0
    residual: Optional[int] = None
    quality: Optional[str] = None
    tools_defined: list = field(default_factory=list)
    tool_calls: list = field(default_factory=list)
    output_est: Optional[int] = None
    response_text: Optional[str] = None
    error: Optional[str] = None

    def to_dict(self) -> dict:
        return {
            "v": self.v,
            "id": self.id,
            "ts": self.ts,
            "run": self.run,
            "provider": self.provider,
            "model": self.model,
            "endpoint": self.endpoint,
            "host_class": self.host_class,
            "host_id": self.host_id,
            "host": self.host,
            "detect": self.detect,
            "stream": self.stream,
            "status": self.status,
            "latency_ms": self.latency_ms,
            "ttfb_ms": self.ttfb_ms,
            "conv": self.conv,
            "sysfp": self.sysfp,
            "sys_label": self.sys_label,
            "usage": self.usage,
            "segments": [s.to_dict() if hasattr(s, "to_dict") else s for s in self.segments],
            "est_input": self.est_input,
            "residual": self.residual,
            "quality": self.quality,
            "tools_defined": self.tools_defined,
            "tool_calls": self.tool_calls,
            "output_est": self.output_est,
            "response_text": self.response_text,
            "error": self.error,
        }
