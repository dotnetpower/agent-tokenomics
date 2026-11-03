"""Text and JSON reports."""

from __future__ import annotations

import json
from typing import Optional

from .analyze import summarize

LABELS = {
    "system": "System prompt",
    "history_user": "History: user turns",
    "history_assistant": "History: assistant turns",
    "user_prompt": "Current user prompt",
    "tool_def": "Tools: definitions",
    "tool_call": "Tools: calls",
    "tool_result": "Tools: results",
    "attachment": "Attachments (uncounted)",
    "response_schema": "Response schema",
    "other": "Other",
    "residual": "Unattributed (template/hidden)",
}


def _bar(share: float, width: int = 24) -> str:
    n = int(round(share * width))
    return "#" * n + "." * (width - n)


def render_text(records: list, gap_s: float = 5.0) -> str:
    if not records:
        return "No LLM calls recorded."
    s = summarize(records, gap_s)
    total = s["input_total"] or 1
    lines = [
        f"Calls: {s['calls']}   Input tokens: {s['input_total']:,}   Output tokens: {s['output_total']:,}   Cached input: {s['cached_total']:,}",
        f"First call input (fixed overhead): {s['first_call_input']:,}   Median input per call: {s['median_input']:,}",
    ]
    fo = s["fan_out"]
    if fo:
        lines.append(f"LLM calls per request (gap-based grouping): median {sorted(fo)[len(fo) // 2]}, max {max(fo)}, requests {len(fo)}")
    lines += ["", "Input token breakdown", "---------------------"]
    for kind in s["kinds_order"]:
        v = s["by_kind"].get(kind, 0)
        if v:
            share = v / total
            lines.append(f"{LABELS.get(kind, kind):<32}{v:>12,}  {100 * share:5.1f}%  {_bar(max(0.0, min(1.0, share)))}")
    if len(s["steps"]) > 1 or (s["steps"] and s["steps"][0]["step"] != "(no sy"):
        lines += ["", "Per-step token map (steps identified by system prompt)", "------------------------------------------------------",
                  f"{'step':<8}{'label':<28}{'calls':>6}{'input':>12}{'fixed/call':>12}"]
        for st in s["steps"][:12]:
            label = (st["label"] or st["model"] or "-")[:26]
            lines.append(f"{st['step']:<8}{label:<28}{st['calls']:>6}{st['input']:>12,}{st['fixed_per_call']:>12,}")
    if s["findings"]:
        lines += ["", "Findings", "--------"]
        for f in s["findings"]:
            lines.append(f"[{f['severity']}] {f['text']}")
            lines.append(f"        see {f['see']}")
    lines += ["", "Note: segment counts are local estimates; 'Unattributed' is reported input minus counted segments."]
    return "\n".join(lines)


def render_markdown(records: list, gap_s: float = 5.0) -> str:
    if not records:
        return "No LLM calls recorded."
    s = summarize(records, gap_s)
    total = s["input_total"] or 1
    md = [
        "# Token profile", "",
        f"- Calls: **{s['calls']}**",
        f"- Input tokens: **{s['input_total']:,}**, output tokens: **{s['output_total']:,}**, cached input: **{s['cached_total']:,}**",
        f"- First call input (fixed overhead): **{s['first_call_input']:,}**", "",
        "## Input token breakdown", "", "| Component | Tokens | Share |", "|-----------|-------:|------:|",
    ]
    for kind in s["kinds_order"]:
        v = s["by_kind"].get(kind, 0)
        if v:
            md.append(f"| {LABELS.get(kind, kind)} | {v:,} | {100 * v / total:.1f}% |")
    if s["findings"]:
        md += ["", "## Findings", ""]
        for f in s["findings"]:
            md.append(f"- **{f['severity']}**: {f['text']} ([guide]({f['see']}))")
    return "\n".join(md)


def render_json(records: list, gap_s: float = 5.0) -> str:
    return json.dumps(summarize(records, gap_s), indent=2)
