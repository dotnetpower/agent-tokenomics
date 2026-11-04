"""Text, terminal, Markdown, JSON, and self-contained HTML reports."""

from __future__ import annotations

import csv
import html
import io
import json
import os
import sys
from typing import Optional

from .analyze import estimate_cost, summarize

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
    "raw_prompt": "Raw prompt (not split)",
    "other": "Other",
    "residual": "Unattributed (template/hidden)",
}

COLORS_HEX = {
    "system": "#6366f1",
    "tool_def": "#8b5cf6",
    "tool_call": "#a855f7",
    "tool_result": "#ec4899",
    "history_user": "#0ea5e9",
    "history_assistant": "#38bdf8",
    "user_prompt": "#10b981",
    "response_schema": "#f59e0b",
    "raw_prompt": "#14b8a6",
    "attachment": "#64748b",
    "other": "#94a3b8",
    "residual": "#cbd5e1",
}

ANSI_COLORS = {
    "system": "\033[38;5;63m",
    "tool_def": "\033[38;5;135m",
    "tool_call": "\033[38;5;171m",
    "tool_result": "\033[38;5;204m",
    "history_user": "\033[38;5;39m",
    "history_assistant": "\033[38;5;75m",
    "user_prompt": "\033[38;5;42m",
    "response_schema": "\033[38;5;214m",
    "raw_prompt": "\033[38;5;37m",
    "residual": "\033[38;5;245m",
    "high": "\033[1;31m",
    "medium": "\033[1;33m",
    "low": "\033[1;36m",
    "info": "\033[1;34m",
    "bold": "\033[1m",
    "dim": "\033[2m",
    "reset": "\033[0m",
}


def _bar(share: float, width: int = 24) -> str:
    n = int(round(share * width))
    return "#" * n + "." * (width - n)


def _utf8_ok() -> bool:
    enc = getattr(sys.stdout, "encoding", None) or "utf-8"
    try:
        "╭─╮█░—".encode(enc)
        return True
    except Exception:
        return False


def _ubo_bar(share: float, width: int = 24, unicode_ok: Optional[bool] = None) -> str:
    if unicode_ok is None:
        unicode_ok = _utf8_ok()
    if not unicode_ok:
        return _bar(share, width)
    blocks = " ▏▎▍▌▋▊▉█"
    total_eighths = int(round(max(0.0, min(1.0, share)) * width * 8))
    full, rem = divmod(total_eighths, 8)
    bar = "█" * full + (blocks[rem] if rem and full < width else "")
    return bar + "░" * max(0, width - len(bar))


def render_cost(records: list, prices: dict) -> str:
    from .analyze import estimate_cost

    c = estimate_cost(records, prices)
    line = f"Estimated cost: {c['total']:.4f} (currency units of your price file; prices change, treat as an estimate)"
    if c["models_without_price"]:
        line += f"\n  no price for: {', '.join(c['models_without_price'])}"
    return line


def render_compare(result: dict) -> str:
    lines = [f"Requests: before {result['requests']['before']}, after {result['requests']['after']}"]
    names = {"input_per_request": "Input tokens per request", "calls_per_request": "LLM calls per request"}
    for key, m in result["metrics"].items():
        if m["diff"] is None:
            lines.append(f"{names[key]}: not enough data")
            continue
        ci = m["ci95"]
        rel = f"{m['relative_change_pct']:+.1f}%" if m["relative_change_pct"] is not None else "n/a"
        verdict = "significant" if m["significant"] else "within noise"
        lines.append(f"{names[key]}: {m['before_mean']:,.1f} -> {m['after_mean']:,.1f}  ({rel}; diff {m['diff']:+,.1f}, 95% CI {ci[0]:+,.1f} to {ci[1]:+,.1f}; {verdict})")
    if not result["reliable"]:
        lines.append("Warning: fewer than 5 requests on a side; the interval is not reliable. Repeat runs on a fixed task set.")
    lines.append("Reminder: report task success next to cost; fewer tokens only count if quality held.")
    return "\n".join(lines)


def _preview_line(text: str, limit: int = 120) -> str:
    if not text:
        return ""
    clean = " ".join(text.strip().split())
    return (clean[:limit - 1] + "…") if len(clean) > limit else clean


def _indent_block(text: str, prefix: str = "      ", max_lines: int = 12, max_chars: int = 900) -> list:
    if not text:
        return []
    clipped = text.strip()
    truncated = False
    if len(clipped) > max_chars:
        clipped = clipped[:max_chars]
        truncated = True
    raw_lines = clipped.splitlines()
    if len(raw_lines) > max_lines:
        raw_lines = raw_lines[:max_lines]
        truncated = True
    out = [f"{prefix}{ln}" for ln in raw_lines]
    if truncated:
        out.append(f"{prefix}... ({len(text):,} chars total; see HTML dashboard or --full-prompts for full text)")
    return out


def render_text(records: list, gap_s: float = 5.0, *, full_prompts: bool = False) -> str:
    if not records:
        return "No LLM calls recorded."
    s = summarize(records, gap_s)
    total = s["input_total"] or 1
    sessions = s.get("sessions", [])
    turns = s.get("turns", [])
    calls = s.get("call_details", [])
    lines = [
        f"Sessions: {len(sessions)}   Turns: {len(turns)}   Calls: {s['calls']}   "
        f"Input tokens: {s['input_total']:,}   Output tokens: {s['output_total']:,}   Cached input: {s['cached_total']:,}",
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

    if sessions:
        lines += [
            "",
            "Per-session summary",
            "-------------------",
            f"{'session':<9}{'turns':>6}{'calls':>7}{'system':>11}{'history':>11}{'user':>11}{'tools/sch':>11}{'input':>12}{'cached':>11}{'output':>9}",
        ]
        for sess in sessions:
            lines.append(
                f"{sess['session']:<9}{sess['turns_count']:>6}{sess['calls_count']:>7}"
                f"{sess['system']:>11,}{sess['history']:>11,}{sess['user_prompt']:>11,}{sess['tools_schema']:>11,}"
                f"{sess['input_total']:>12,}{sess['cached_total']:>11,}{sess['output_total']:>9,}"
            )

    if turns:
        lines += [
            "",
            "Per-turn breakdown (System / History / User Prompt / Tools & Schema)",
            "--------------------------------------------------------------------",
            f"{'turn':<9}{'calls':>6}{'system':>11}{'history':>11}{'user':>11}{'tools/sch':>11}{'input':>12}{'cached':>11}{'output':>9}",
        ]
        for t in turns:
            tag = f"{t['session']}·T{t['turn']}"
            lines.append(
                f"{tag:<9}{t['calls_count']:>6}"
                f"{t['system']:>11,}{t['history']:>11,}{t['user_prompt']:>11,}{t['tools_schema']:>11,}"
                f"{t['input_total']:>12,}{t['cached_total']:>11,}{t['output_total']:>9,}"
            )
            if t.get("user_text"):
                lines.append(f"    User prompt: {_preview_line(t['user_text'], 100)}")
            if t.get("history_text"):
                lines.append(f"    History:     {_preview_line(t['history_text'], 100)}")
            if t.get("response_text"):
                lines.append(f"    Assistant:   {_preview_line(t['response_text'], 100)}")

    if calls:
        lines += [
            "",
            "Per-call breakdown",
            "------------------",
            f"{'#':<4}{'turn':<8}{'model':<22}{'system':>9}{'history':>9}{'user':>9}{'tools/sch':>10}{'input':>10}{'cached':>9}{'out':>7}",
        ]
        for cd in calls[:40]:
            tag = f"{cd['session']}·T{cd['turn']}"
            lines.append(
                f"#{cd['index']:<3}{tag:<8}{cd['model'][:20]:<22}"
                f"{cd['system']:>9,}{cd['history']:>9,}{cd['user_prompt']:>9,}{cd['tools_schema']:>10,}"
                f"{cd['input_total']:>10,}{cd['cached']:>9,}{cd['output']:>7,}"
            )

    if len(s["steps"]) > 1 or (s["steps"] and s["steps"][0]["step"] != "(no sy"):
        lines += ["", "Per-step token map (steps identified by system prompt)", "------------------------------------------------------",
                  f"{'step':<8}{'label':<28}{'calls':>6}{'input':>12}{'fixed/call':>12}"]
        for st in s["steps"][:12]:
            label = (st["label"] or st["model"] or "-")[:26]
            lines.append(f"{st['step']:<8}{label:<28}{st['calls']:>6}{st['input']:>12,}{st['fixed_per_call']:>12,}")
    eps = s["endpoints"]
    if len(eps) > 1 or (eps and eps[0]["class"] not in ("unknown",)):
        lines += ["", "Endpoints (by detected API and host class)", "------------------------------------------",
                  f"{'host':<26}{'class':<24}{'api':<16}{'calls':>6}{'input':>12}"]
        for e in eps[:12]:
            who = (e["host"] or (f"#{e['id']}" if e["id"] else "-"))[:24]
            lines.append(f"{who:<26}{e['class'][:22]:<24}{e['api']:<16}{e['calls']:>6}{e['input']:>12,}")

    has_prompts = any(any(cd["prompts"].values()) for cd in calls)
    if has_prompts:
        max_l = 10000 if full_prompts else 10
        max_c = 100000 if full_prompts else 700
        lines += ["", "Captured prompt & response contents", "-----------------------------------"]
        for cd in calls[:20]:
            p = cd["prompts"]
            if not any(p.values()):
                continue
            lines.append(f"  Call #{cd['index']} ({cd['session']}·T{cd['turn']} | {cd['model']} | step {cd['step']})")
            for key, title in (
                ("system", f"System Prompt ({cd['system']:,} tok)"),
                ("history", f"Conversation History ({cd['history']:,} tok)"),
                ("user_prompt", f"User Prompt ({cd['user_prompt']:,} tok)"),
                ("tools_schema", f"Tools & Schema ({cd['tools_schema']:,} tok)"),
                ("response", f"Assistant Output ({cd['output']:,} tok)"),
            ):
                if p.get(key):
                    lines.append(f"    [{title}]")
                    lines.extend(_indent_block(p[key], "      ", max_lines=max_l, max_chars=max_c))

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
    eps = s["endpoints"]
    if len(eps) > 1 or (eps and eps[0]["class"] != "unknown"):
        md += ["", "## Endpoints", "", "| Host | Class | API | Calls | Input tokens |", "|------|-------|-----|------:|-------------:|"]
        for e in eps[:12]:
            md.append(f"| {e['host'] or ('#' + e['id'] if e['id'] else '-')} | {e['class']} | {e['api']} | {e['calls']} | {e['input']:,} |")
    if s["findings"]:
        md += ["", "## Findings", ""]
        for f in s["findings"]:
            md.append(f"- **{f['severity']}**: {f['text']} ([guide]({f['see']}))")
    return "\n".join(md)


def render_json(records: list, gap_s: float = 5.0) -> str:
    return json.dumps(summarize(records, gap_s), indent=2)


CSV_COLUMNS = (
    "row_type",
    "session",
    "turn",
    "call_index",
    "call_id",
    "timestamp",
    "model",
    "provider",
    "step",
    "step_label",
    "turns_count",
    "calls_count",
    "input_total",
    "cached_tokens",
    "output_tokens",
    "system_tokens",
    "history_tokens",
    "history_user_tokens",
    "history_assistant_tokens",
    "user_prompt_tokens",
    "tools_schema_tokens",
    "tool_def_tokens",
    "tool_call_tokens",
    "tool_result_tokens",
    "response_schema_tokens",
    "other_tokens",
    "residual_tokens",
    "latency_ms",
    "status",
    "system_prompt",
    "conversation_history",
    "user_prompt",
    "tools_schema",
    "assistant_response",
)


def render_csv(records: list, gap_s: float = 5.0) -> str:
    """Render per-session, per-turn, and per-call breakdowns and prompt contents as CSV."""
    buf = io.StringIO()
    writer = csv.DictWriter(buf, fieldnames=CSV_COLUMNS, lineterminator="\n")
    writer.writeheader()
    if not records:
        return buf.getvalue().rstrip("\n")
    s = summarize(records, gap_s)
    for sess in s.get("sessions", []):
        writer.writerow({
            "row_type": "session",
            "session": sess["session"],
            "turn": "",
            "call_index": "",
            "call_id": "",
            "timestamp": "",
            "model": ",".join(sess.get("models") or []),
            "provider": "",
            "step": "",
            "step_label": "",
            "turns_count": sess["turns_count"],
            "calls_count": sess["calls_count"],
            "input_total": sess["input_total"],
            "cached_tokens": sess["cached_total"],
            "output_tokens": sess["output_total"],
            "system_tokens": sess["system"],
            "history_tokens": sess["history"],
            "history_user_tokens": "",
            "history_assistant_tokens": "",
            "user_prompt_tokens": sess["user_prompt"],
            "tools_schema_tokens": sess["tools_schema"],
            "tool_def_tokens": "",
            "tool_call_tokens": "",
            "tool_result_tokens": "",
            "response_schema_tokens": "",
            "other_tokens": sess["other"],
            "residual_tokens": sess["residual"],
            "latency_ms": "",
            "status": "",
            "system_prompt": "",
            "conversation_history": "",
            "user_prompt": "",
            "tools_schema": "",
            "assistant_response": "",
        })
    for t in s.get("turns", []):
        writer.writerow({
            "row_type": "turn",
            "session": t["session"],
            "turn": t["turn"],
            "call_index": "",
            "call_id": "",
            "timestamp": "",
            "model": ",".join(t.get("models") or []),
            "provider": "",
            "step": "",
            "step_label": "",
            "turns_count": 1,
            "calls_count": t["calls_count"],
            "input_total": t["input_total"],
            "cached_tokens": t["cached_total"],
            "output_tokens": t["output_total"],
            "system_tokens": t["system"],
            "history_tokens": t["history"],
            "history_user_tokens": t["history_user"],
            "history_assistant_tokens": t["history_assistant"],
            "user_prompt_tokens": t["user_prompt"],
            "tools_schema_tokens": t["tools_schema"],
            "tool_def_tokens": "",
            "tool_call_tokens": "",
            "tool_result_tokens": "",
            "response_schema_tokens": "",
            "other_tokens": t["other"],
            "residual_tokens": t["residual"],
            "latency_ms": t["latency_ms"],
            "status": "",
            "system_prompt": "",
            "conversation_history": t.get("history_text") or "",
            "user_prompt": t.get("user_text") or "",
            "tools_schema": "",
            "assistant_response": t.get("response_text") or "",
        })
    for cd in s.get("call_details", []):
        p = cd.get("prompts") or {}
        writer.writerow({
            "row_type": "call",
            "session": cd["session"],
            "turn": cd["turn"],
            "call_index": cd["index"],
            "call_id": cd["id"],
            "timestamp": cd["ts"],
            "model": cd["model"],
            "provider": cd["provider"],
            "step": cd["step"],
            "step_label": cd.get("step_label") or "",
            "turns_count": "",
            "calls_count": 1,
            "input_total": cd["input_total"],
            "cached_tokens": cd["cached"],
            "output_tokens": cd["output"],
            "system_tokens": cd["system"],
            "history_tokens": cd["history"],
            "history_user_tokens": cd["history_user"],
            "history_assistant_tokens": cd["history_assistant"],
            "user_prompt_tokens": cd["user_prompt"],
            "tools_schema_tokens": cd["tools_schema"],
            "tool_def_tokens": cd["tool_def"],
            "tool_call_tokens": cd["tool_call"],
            "tool_result_tokens": cd["tool_result"],
            "response_schema_tokens": cd["response_schema"],
            "other_tokens": cd["other"],
            "residual_tokens": cd["residual"],
            "latency_ms": cd["latency_ms"] if cd.get("latency_ms") is not None else "",
            "status": cd["status"] if cd.get("status") is not None else "",
            "system_prompt": p.get("system") or "",
            "conversation_history": p.get("history") or "",
            "user_prompt": p.get("user_prompt") or "",
            "tools_schema": p.get("tools_schema") or "",
            "assistant_response": p.get("response") or "",
        })
    return buf.getvalue().rstrip("\n")


def render_pretty(
    records: list,
    gap_s: float = 5.0,
    *,
    color: Optional[bool] = None,
    prices: Optional[dict] = None,
    full_prompts: bool = False,
) -> str:
    """Rich Unicode terminal report with box-drawing cards, smooth bars, per-session/per-turn breakdown, and prompt contents."""
    if not records:
        return "No LLM calls recorded."
    if color is None:
        color = hasattr(sys.stdout, "isatty") and sys.stdout.isatty() and not os.environ.get("NO_COLOR")

    def c(tag: str, text: str) -> str:
        return f"{ANSI_COLORS.get(tag, '')}{text}{ANSI_COLORS['reset']}" if color else text

    s = summarize(records, gap_s)
    total = s["input_total"] or 1
    cache_pct = 100.0 * s["cached_total"] / total if s["input_total"] else 0.0
    fo = s["fan_out"]
    med_fo = sorted(fo)[len(fo) // 2] if fo else 0
    max_fo = max(fo) if fo else 0
    sessions = s.get("sessions", [])
    turns = s.get("turns", [])
    calls = s.get("call_details", [])

    u_ok = _utf8_ok()
    rule = ("─" if u_ok else "-") * 92
    top = "╭" + ("─" * 90) + "╮" if u_ok else "+" + ("-" * 90) + "+"
    mid = "│  tokprof — LLM Token Profile (Per-Session · Per-Turn · Prompt Inspector)                 │" if u_ok else "|  tokprof - LLM Token Profile (Per-Session / Per-Turn / Prompt Inspector)                 |"
    bot = "╰" + ("─" * 90) + "╯" if u_ok else "+" + ("-" * 90) + "+"

    sess_str = c("bold", f"{len(sessions):,}")
    turns_str = c("bold", f"{len(turns):,}")
    calls_str = c("bold", f"{s['calls']:,}")
    in_str = c("bold", f"{s['input_total']:,}")
    out_str = c("bold", f"{s['output_total']:,}")
    cached_str = c("bold", f"{s['cached_total']:,}")
    lines = [
        c("bold", top),
        c("bold", mid),
        c("bold", bot),
        f"  Sessions: {sess_str}   Turns: {turns_str}   Calls: {calls_str}   "
        f"Input: {in_str}   Output: {out_str}   Cached: {cached_str} ({cache_pct:.1f}%)",
        f"  First-call overhead: {s['first_call_input']:,} tok   "
        f"Median input/call: {s['median_input']:,} tok   "
        f"Fan-out: median {med_fo}, max {max_fo} calls/turn",
    ]
    if prices:
        lines.append("  " + render_cost(records, prices))

    lines += ["", c("bold", "  1. Input token breakdown (total)"), "  " + rule]
    for kind in s["kinds_order"]:
        v = s["by_kind"].get(kind, 0)
        if v:
            share = v / total
            bar = c(kind, _ubo_bar(max(0.0, min(1.0, share)), 26, u_ok))
            lines.append(f"  {LABELS.get(kind, kind):<32}{v:>12,}  {100 * share:5.1f}%  {bar}")

    if sessions:
        lines += [
            "",
            c("bold", "  2. Per-Session breakdown (System Prompt / Conv History / User Prompt / Tools & Schema)"),
            "  " + rule,
            f"  {'session':<9}{'turns':>6}{'calls':>7}{'system':>11}{'history':>11}{'user_prompt':>13}{'tools/schema':>14}{'total_in':>12}{'cached':>10}{'output':>8}",
        ]
        for sess in sessions:
            lines.append(
                f"  {sess['session']:<9}{sess['turns_count']:>6}{sess['calls_count']:>7}"
                f"{sess['system']:>11,}{sess['history']:>11,}{sess['user_prompt']:>13,}{sess['tools_schema']:>14,}"
                f"{sess['input_total']:>12,}{sess['cached_total']:>10,}{sess['output_total']:>8,}"
            )

    if turns:
        lines += [
            "",
            c("bold", "  3. Per-Turn breakdown (across turns of each session)"),
            "  " + rule,
            f"  {'turn':<9}{'calls':>6}{'system':>11}{'history':>11}{'user_prompt':>13}{'tools/schema':>14}{'total_in':>12}{'cached':>10}{'output':>8}  {'lat':>7}",
        ]
        for t in turns:
            tag = f"{t['session']}·T{t['turn']}"
            lat = f"{t['latency_ms']:,}ms" if t.get("latency_ms") else "-"
            lines.append(
                f"  {tag:<9}{t['calls_count']:>6}"
                f"{t['system']:>11,}{t['history']:>11,}{t['user_prompt']:>13,}{t['tools_schema']:>14,}"
                f"{t['input_total']:>12,}{t['cached_total']:>10,}{t['output_total']:>8,}  {lat:>7}"
            )
            if t.get("user_text"):
                lines.append(c("user_prompt", f"      User Prompt:  {_preview_line(t['user_text'], 84)}"))
            if t.get("history_text"):
                lines.append(c("history_user", f"      Conv History: {_preview_line(t['history_text'], 84)}"))
            if t.get("response_text"):
                lines.append(c("dim", f"      Assistant:    {_preview_line(t['response_text'], 84)}"))

    if calls:
        lines += [
            "",
            c("bold", "  4. Per-Call breakdown (every individual LLM request)"),
            "  " + rule,
            f"  {'#':<4}{'turn':<8}{'model / step':<24}{'system':>9}{'history':>9}{'user':>9}{'tools/sch':>11}{'input':>10}{'cached':>9}{'out':>7} {'lat':>7}",
        ]
        for cd in calls[:30]:
            tag = f"{cd['session']}·T{cd['turn']}"
            m_step = cd["step_label"] or cd["model"] or "-"
            if cd["step_label"] and cd["model"] and cd["step_label"] != cd["model"]:
                m_step = f"{cd['model'][:12]}:{cd['step_label'][:11]}"
            lat = f"{cd['latency_ms']}ms" if cd.get("latency_ms") is not None else "-"
            lines.append(
                f"  #{cd['index']:<3}{tag:<8}{m_step[:23]:<24}"
                f"{cd['system']:>9,}{cd['history']:>9,}{cd['user_prompt']:>9,}{cd['tools_schema']:>11,}"
                f"{cd['input_total']:>10,}{cd['cached']:>9,}{cd['output']:>7,} {lat:>7}"
            )
        if len(calls) > 30:
            lines.append(f"  ... ({len(calls) - 30} more call(s))")

    if len(s["steps"]) > 1 or (s["steps"] and s["steps"][0]["step"] != "(no sy"):
        lines += [
            "",
            c("bold", "  5. Per-step token map (grouped by system prompt fingerprint)"),
            "  " + rule,
            f"  {'step':<8}{'label/model':<28}{'calls':>6}{'input':>14}{'fixed/call':>14}",
        ]
        for st in s["steps"][:12]:
            label = (st["label"] or st["model"] or "-")[:26]
            lines.append(f"  {st['step']:<8}{label:<28}{st['calls']:>6}{st['input']:>14,}{st['fixed_per_call']:>14,}")

    eps = s["endpoints"]
    if len(eps) > 1 or (eps and eps[0]["class"] not in ("unknown",)):
        lines += [
            "",
            c("bold", "  Endpoints (by detected API and host class)"),
            "  " + rule,
            f"  {'host':<26}{'class':<24}{'api':<16}{'calls':>6}{'input':>12}",
        ]
        for e in eps[:12]:
            who = (e["host"] or (f"#{e['id']}" if e["id"] else "-"))[:24]
            lines.append(f"  {who:<26}{e['class'][:22]:<24}{e['api']:<16}{e['calls']:>6}{e['input']:>12,}")

    has_prompts = any(any(cd["prompts"].values()) for cd in calls)
    if has_prompts:
        max_l = 10000 if full_prompts else 10
        max_c = 100000 if full_prompts else 700
        lines += [
            "",
            c("bold", "  6. Prompt & Response Contents (System / History / User Prompt / Tools / Output)"),
            "  " + rule,
        ]
        # Show unique system prompts first so long instructions are not repeated on every turn
        seen_sys: set = set()
        for cd in calls:
            sp_text = cd["prompts"].get("system")
            if sp_text and cd["step"] not in seen_sys:
                seen_sys.add(cd["step"])
                lines.append(c("system", f"  [System Prompt · step {cd['step']} · {cd['model']} · {cd['system']:,} tokens]"))
                lines.extend(_indent_block(sp_text, "    ", max_lines=max_l, max_chars=max_c))
                lines.append("")
        # Show each call's History, User Prompt, and Assistant Output
        for cd in calls[:20]:
            p = cd["prompts"]
            if not (p.get("history") or p.get("user_prompt") or p.get("response")):
                continue
            lines.append(c("bold", f"  Call #{cd['index']} ({cd['session']}·T{cd['turn']} · {cd['model']} · step {cd['step']})"))
            if p.get("history"):
                lines.append(c("history_user", f"    [Conversation History · {cd['history']:,} tokens]"))
                lines.extend(_indent_block(p["history"], "      ", max_lines=max_l, max_chars=max_c))
            if p.get("user_prompt"):
                lines.append(c("user_prompt", f"    [User Prompt · {cd['user_prompt']:,} tokens]"))
                lines.extend(_indent_block(p["user_prompt"], "      ", max_lines=max_l, max_chars=max_c))
            if p.get("tools_schema") and full_prompts:
                lines.append(c("tool_def", f"    [Tools & Schema · {cd['tools_schema']:,} tokens]"))
                lines.extend(_indent_block(p["tools_schema"], "      ", max_lines=max_l, max_chars=max_c))
            if p.get("response"):
                lines.append(c("dim", f"    [Assistant Output · {cd['output']:,} tokens]"))
                lines.extend(_indent_block(p["response"], "      ", max_lines=max_l, max_chars=max_c))

    if s["findings"]:
        lines += ["", c("bold", "  Findings & recommendations"), "  " + rule]
        for f in s["findings"]:
            sev = f["severity"]
            badge = c(sev, f"[{sev.upper()}]")
            lines.append(f"  {badge} {f['text']}")
            lines.append(c("dim", f"         -> {f['see']}"))

    lines += ["", c("dim", "  Note: segment counts are local estimates; 'Unattributed' is reported input minus counted segments.")]
    return "\n".join(lines)


def render_html(
    records: list,
    gap_s: float = 5.0,
    *,
    prices: Optional[dict] = None,
    title: str = "tokprof — LLM Token Profile",
) -> str:
    """Self-contained single-file HTML dashboard with interactive SVG breakdown and per-call timeline."""
    s = summarize(records, gap_s)
    total = s["input_total"] or 1
    cache_pct = (100.0 * s["cached_total"] / total) if s["input_total"] else 0.0
    fo = s["fan_out"]
    med_fo = sorted(fo)[len(fo) // 2] if fo else 0
    max_fo = max(fo) if fo else 0
    cost_info = estimate_cost(records, prices) if (prices and records) else None

    # Stacked horizontal bar segments
    stack_divs = []
    breakdown_rows = []
    for kind in s["kinds_order"]:
        v = s["by_kind"].get(kind, 0)
        if not v:
            continue
        pct = 100.0 * v / total
        col = COLORS_HEX.get(kind, "#94a3b8")
        label = html.escape(LABELS.get(kind, kind))
        if pct > 0:
            stack_divs.append(
                f'<div class="seg" style="width:{max(0.5, pct):.2f}%;background:{col}" '
                f'title="{label}: {v:,} tokens ({pct:.1f}%)"></div>'
            )
        breakdown_rows.append(
            f"<tr>"
            f'<td><span class="dot" style="background:{col}"></span>{label}</td>'
            f'<td class="num">{v:,}</td>'
            f'<td class="num">{pct:.1f}%</td>'
            f'<td><div class="mini-track"><div class="mini-fill" style="width:{max(0.0, min(100.0, pct)):.1f}%;background:{col}"></div></div></td>'
            f"</tr>"
        )

    # Per-session & per-turn rows
    sessions = s.get("sessions", [])
    turns = s.get("turns", [])
    calls = s.get("call_details", [])

    session_rows = []
    for sess in sessions:
        session_rows.append(
            f"<tr>"
            f"<td><strong>{html.escape(sess['session'])}</strong></td>"
            f'<td class="num">{sess["turns_count"]:,}</td>'
            f'<td class="num">{sess["calls_count"]:,}</td>'
            f'<td class="num">{sess["system"]:,}</td>'
            f'<td class="num">{sess["history"]:,}</td>'
            f'<td class="num">{sess["user_prompt"]:,}</td>'
            f'<td class="num">{sess["tools_schema"]:,}</td>'
            f'<td class="num"><strong>{sess["input_total"]:,}</strong></td>'
            f'<td class="num">{sess["cached_total"]:,}</td>'
            f'<td class="num">{sess["output_total"]:,}</td>'
            f"</tr>"
        )

    turn_rows = []
    max_turn_in = max((t["input_total"] or 1) for t in turns) if turns else 1
    for t in turns:
        tag = f"{t['session']} · Turn {t['turn']}"
        t_bars = []
        for k_name, kv, col in (
            ("System Prompt", t["system"], COLORS_HEX["system"]),
            ("Conversation History", t["history"], COLORS_HEX["history_user"]),
            ("User Prompt", t["user_prompt"], COLORS_HEX["user_prompt"]),
            ("Tools & Schema", t["tools_schema"], COLORS_HEX["tool_def"]),
        ):
            if kv > 0:
                w = 100.0 * kv / max_turn_in
                t_bars.append(f'<span style="width:{w:.2f}%;background:{col}" title="{k_name}: {kv:,} tok"></span>')
        previews = []
        if t.get("user_text"):
            previews.append(f'<div><strong>User Prompt:</strong> <code>{html.escape(_preview_line(t["user_text"], 140))}</code></div>')
        if t.get("history_text"):
            previews.append(f'<div><strong>Conv History:</strong> <code>{html.escape(_preview_line(t["history_text"], 140))}</code></div>')
        if t.get("response_text"):
            previews.append(f'<div><strong>Assistant:</strong> <code>{html.escape(_preview_line(t["response_text"], 140))}</code></div>')
        preview_html = f'<div class="turn-preview">{"".join(previews)}</div>' if previews else ""
        turn_rows.append(
            f"<tr>"
            f"<td><strong>{html.escape(tag)}</strong></td>"
            f'<td class="num">{t["calls_count"]:,}</td>'
            f'<td class="num">{t["system"]:,}</td>'
            f'<td class="num">{t["history"]:,}</td>'
            f'<td class="num">{t["user_prompt"]:,}</td>'
            f'<td class="num">{t["tools_schema"]:,}</td>'
            f'<td class="num"><strong>{t["input_total"]:,}</strong></td>'
            f'<td class="num">{t["cached_total"]:,}</td>'
            f'<td class="num">{t["output_total"]:,}</td>'
            f'<td><div class="call-bar">{"".join(t_bars)}</div>{preview_html}</td>'
            f"</tr>"
        )

    # Per-call timeline & expandable prompt inspector rows
    call_rows = []
    max_in = max((cd["input_total"] or 1) for cd in calls) if calls else 1
    for cd in calls[:150]:
        bars = []
        for k_name, kv, col in (
            ("System Prompt", cd["system"], COLORS_HEX["system"]),
            ("Conversation History", cd["history"], COLORS_HEX["history_user"]),
            ("User Prompt", cd["user_prompt"], COLORS_HEX["user_prompt"]),
            ("Tools & Schema", cd["tools_schema"], COLORS_HEX["tool_def"]),
        ):
            if kv > 0:
                w = 100.0 * kv / max_in
                bars.append(f'<span style="width:{w:.2f}%;background:{col}" title="{k_name}: {kv:,} tok"></span>')
        model_lbl = html.escape(cd["model"])
        if cd.get("step_label") and cd["step_label"] != cd["model"]:
            model_lbl += f' <span class="sub">({html.escape(cd["step_label"])})</span>'
        lat = f"{cd['latency_ms']:,} ms" if cd.get("latency_ms") is not None else "-"
        p = cd["prompts"]
        prompt_blocks = []
        for key, label, tok_cnt, col in (
            ("system", "System Prompt", cd["system"], COLORS_HEX["system"]),
            ("history", "Conversation History", cd["history"], COLORS_HEX["history_user"]),
            ("user_prompt", "Current User Prompt", cd["user_prompt"], COLORS_HEX["user_prompt"]),
            ("tools_schema", "Tools & Response Schema", cd["tools_schema"], COLORS_HEX["tool_def"]),
            ("other", "Other Context", cd["other"], COLORS_HEX["other"]),
            ("response", "Assistant Output", cd["output"], "#10b981"),
        ):
            if p.get(key):
                prompt_blocks.append(
                    f'<div class="p-sec">'
                    f'<div class="p-head"><span class="dot" style="background:{col}"></span>'
                    f"<strong>{label}</strong> <span class=\"sub\">({tok_cnt:,} tokens)</span></div>"
                    f'<pre class="p-pre">{html.escape(p[key])}</pre>'
                    f"</div>"
                )
        inspector = (
            f'<details class="p-details"><summary>View prompts &amp; response</summary>'
            f'<div class="p-box">{"".join(prompt_blocks)}</div></details>'
            if prompt_blocks
            else '<span class="sub">no text stored (re-capture to inspect full prompt text)</span>'
        )
        call_rows.append(
            f"<tr>"
            f'<td class="num">#{cd["index"]}</td>'
            f'<td><code>{html.escape(cd["session"])}·T{cd["turn"]}</code></td>'
            f"<td><code>{model_lbl}</code></td>"
            f'<td class="num">{cd["system"]:,}</td>'
            f'<td class="num">{cd["history"]:,}</td>'
            f'<td class="num">{cd["user_prompt"]:,}</td>'
            f'<td class="num">{cd["tools_schema"]:,}</td>'
            f'<td class="num"><strong>{cd["input_total"]:,}</strong></td>'
            f'<td class="num">{cd["cached"]:,}</td>'
            f'<td class="num">{cd["output"]:,}</td>'
            f'<td class="num">{lat}</td>'
            f'<td><div class="call-bar">{"".join(bars)}</div>{inspector}</td>'
            f"</tr>"
        )

    step_rows = []
    for st in s["steps"][:20]:
        lbl = html.escape(str(st["label"] or st["model"] or "-"))
        step_rows.append(
            f"<tr><td><code>{html.escape(st['step'])}</code></td><td>{lbl}</td>"
            f'<td class="num">{st["calls"]:,}</td><td class="num">{st["input"]:,}</td>'
            f'<td class="num">{st["fixed_per_call"]:,}</td></tr>'
        )

    ep_rows = []
    for e in s["endpoints"][:20]:
        who = html.escape(str(e["host"] or (f"#{e['id']}" if e["id"] else "-")))
        ep_rows.append(
            f"<tr><td><code>{who}</code></td><td>{html.escape(str(e['class']))}</td>"
            f"<td><code>{html.escape(str(e['api']))}</code></td>"
            f'<td class="num">{e["calls"]:,}</td><td class="num">{e["input"]:,}</td></tr>'
        )

    finding_cards = []
    for f in s["findings"]:
        sev = html.escape(f["severity"])
        finding_cards.append(
            f'<div class="finding sev-{sev}">'
            f'<span class="badge badge-{sev}">{sev.upper()}</span>'
            f'<div class="finding-body"><div>{html.escape(f["text"])}</div>'
            f'<a href="{html.escape(f["see"])}" target="_blank" rel="noopener">Troubleshooting guide &rarr;</a></div>'
            f"</div>"
        )
    if not finding_cards:
        finding_cards.append('<div class="finding sev-info"><span class="badge badge-info">OK</span><div class="finding-body">No token-waste anomalies triggered on this run.</div></div>')

    cost_card = (
        f'<div class="card"><div class="k-label">Estimated Cost</div><div class="k-val">{cost_info["total"]:.4f}</div>'
        f'<div class="k-sub">from price table</div></div>'
        if cost_info is not None
        else ""
    )

    return f"""<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>{html.escape(title)}</title>
<style>
:root {{
  --bg: #0f172a; --panel: #1e293b; --border: #334155; --text: #f8fafc; --muted: #94a3b8;
  --accent: #6366f1; --high: #ef4444; --medium: #f59e0b; --low: #06b6d4; --info: #3b82f6;
}}
@media (prefers-color-scheme: light) {{
  :root {{ --bg: #f8fafc; --panel: #ffffff; --border: #e2e8f0; --text: #0f172a; --muted: #64748b; }}
}}
* {{ box-sizing: border-box; }}
body {{ margin: 0; padding: 28px 20px 60px; font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif; background: var(--bg); color: var(--text); line-height: 1.5; }}
.wrap {{ max-width: 1120px; margin: 0 auto; }}
header {{ display: flex; justify-content: space-between; align-items: baseline; flex-wrap: wrap; gap: 12px; margin-bottom: 22px; }}
h1 {{ font-size: 1.5rem; margin: 0; letter-spacing: -0.02em; }}
h2 {{ font-size: 1.05rem; margin: 26px 0 12px; letter-spacing: -0.01em; }}
.sub {{ color: var(--muted); font-size: 0.875rem; }}
.cards {{ display: grid; grid-template-columns: repeat(auto-fit, minmax(165px, 1fr)); gap: 12px; margin-bottom: 22px; }}
.card {{ background: var(--panel); border: 1px solid var(--border); border-radius: 10px; padding: 14px 16px; }}
.k-label {{ color: var(--muted); font-size: 0.78rem; text-transform: uppercase; letter-spacing: 0.04em; }}
.k-val {{ font-size: 1.45rem; font-weight: 700; margin-top: 4px; font-variant-numeric: tabular-nums; }}
.k-sub {{ color: var(--muted); font-size: 0.78rem; margin-top: 2px; }}
.panel {{ background: var(--panel); border: 1px solid var(--border); border-radius: 10px; padding: 18px; margin-bottom: 18px; }}
.stack-bar {{ display: flex; height: 22px; border-radius: 6px; overflow: hidden; background: var(--border); margin-bottom: 16px; }}
.stack-bar .seg {{ height: 100%; transition: opacity 0.15s; }}
.stack-bar .seg:hover {{ opacity: 0.82; }}
table {{ width: 100%; border-collapse: collapse; font-size: 0.88rem; }}
th, td {{ padding: 8px 10px; text-align: left; border-bottom: 1px solid var(--border); }}
th {{ color: var(--muted); font-weight: 600; font-size: 0.78rem; text-transform: uppercase; letter-spacing: 0.04em; }}
td.num, th.num {{ text-align: right; font-variant-numeric: tabular-nums; }}
.dot {{ display: inline-block; width: 10px; height: 10px; border-radius: 3px; margin-right: 8px; vertical-align: middle; }}
.mini-track {{ width: 140px; height: 8px; background: var(--border); border-radius: 4px; overflow: hidden; }}
.mini-fill {{ height: 100%; border-radius: 4px; }}
.call-bar {{ display: flex; height: 12px; width: 200px; background: var(--border); border-radius: 4px; overflow: hidden; }}
.call-bar span {{ display: block; height: 100%; }}
.turn-preview {{ margin-top: 6px; font-size: 0.78rem; color: var(--muted); display: flex; flex-direction: column; gap: 2px; }}
.p-details {{ margin-top: 6px; font-size: 0.8rem; }}
.p-details summary {{ cursor: pointer; color: var(--accent); font-weight: 600; user-select: none; }}
.p-box {{ margin-top: 8px; display: flex; flex-direction: column; gap: 10px; max-width: 640px; }}
.p-sec {{ border: 1px solid var(--border); border-radius: 6px; padding: 8px 10px; background: var(--bg); }}
.p-head {{ font-size: 0.78rem; margin-bottom: 6px; }}
.p-pre {{ margin: 0; white-space: pre-wrap; word-break: break-word; max-height: 320px; overflow-y: auto; font-size: 0.78rem; line-height: 1.45; font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace; }}
.finding {{ display: flex; align-items: flex-start; gap: 12px; padding: 12px 14px; border-radius: 8px; border: 1px solid var(--border); margin-bottom: 10px; background: var(--panel); }}
.finding-body {{ font-size: 0.9rem; }}
.finding-body a {{ color: var(--accent); text-decoration: none; font-size: 0.82rem; font-weight: 500; }}
.badge {{ font-size: 0.7rem; font-weight: 700; padding: 2px 7px; border-radius: 4px; color: #fff; flex-shrink: 0; margin-top: 2px; }}
.badge-high {{ background: var(--high); }}
.badge-medium {{ background: var(--medium); color: #111827; }}
.badge-low {{ background: var(--low); color: #111827; }}
.badge-info {{ background: var(--info); }}
code {{ font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace; font-size: 0.82rem; }}
.grid-2 {{ display: grid; grid-template-columns: repeat(auto-fit, minmax(420px, 1fr)); gap: 18px; }}
footer {{ color: var(--muted); font-size: 0.8rem; margin-top: 24px; text-align: center; }}
</style>
</head>
<body>
<div class="wrap">
  <header>
    <div>
      <h1>{html.escape(title)}</h1>
      <div class="sub">Zero-code LLM input token decomposition &mdash; system prompt, conversation history, user prompt, and tools</div>
    </div>
  </header>

  <div class="cards">
    <div class="card"><div class="k-label">Sessions &amp; Turns</div><div class="k-val">{len(sessions):,} / {len(turns):,}</div><div class="k-sub">{s["calls"]:,} total LLM call(s)</div></div>
    <div class="card"><div class="k-label">Input Tokens</div><div class="k-val">{s["input_total"]:,}</div><div class="k-sub">median {s["median_input"]:,} / call</div></div>
    <div class="card"><div class="k-label">Cached Input</div><div class="k-val">{s["cached_total"]:,}</div><div class="k-sub">{cache_pct:.1f}% cache hit rate</div></div>
    <div class="card"><div class="k-label">Output Tokens</div><div class="k-val">{s["output_total"]:,}</div><div class="k-sub">generated</div></div>
    <div class="card"><div class="k-label">Fixed Overhead</div><div class="k-val">{s["first_call_input"]:,}</div><div class="k-sub">first call input</div></div>
    <div class="card"><div class="k-label">Fan-out</div><div class="k-val">{med_fo}</div><div class="k-sub">median calls/turn (max {max_fo})</div></div>
    {cost_card}
  </div>

  <div class="panel">
    <h2>1. Input Token Breakdown (Total)</h2>
    <div class="stack-bar">{"".join(stack_divs)}</div>
    <table>
      <thead><tr><th>Component</th><th class="num">Tokens</th><th class="num">Share</th><th>Distribution</th></tr></thead>
      <tbody>{"".join(breakdown_rows)}</tbody>
    </table>
  </div>

  <div class="panel">
    <h2>2. Per-Session Summary</h2>
    <table>
      <thead>
        <tr>
          <th>Session</th><th class="num">Turns</th><th class="num">Calls</th>
          <th class="num">System Prompt</th><th class="num">Conv History</th><th class="num">User Prompt</th><th class="num">Tools &amp; Schema</th>
          <th class="num">Total Input</th><th class="num">Cached</th><th class="num">Output</th>
        </tr>
      </thead>
      <tbody>{"".join(session_rows) or '<tr><td colspan="10" class="sub">No sessions recorded</td></tr>'}</tbody>
    </table>
  </div>

  <div class="panel">
    <h2>3. Per-Turn Breakdown (System Prompt vs. Conversation History vs. User Prompt)</h2>
    <table>
      <thead>
        <tr>
          <th>Session &middot; Turn</th><th class="num">Calls</th>
          <th class="num">System Prompt</th><th class="num">Conv History</th><th class="num">User Prompt</th><th class="num">Tools &amp; Schema</th>
          <th class="num">Total Input</th><th class="num">Cached</th><th class="num">Output</th><th>Composition &amp; Prompt Preview</th>
        </tr>
      </thead>
      <tbody>{"".join(turn_rows) or '<tr><td colspan="10" class="sub">No turns recorded</td></tr>'}</tbody>
    </table>
  </div>

  <div class="panel">
    <h2>4. Per-Call Timeline &amp; Full Prompt Inspector ({min(len(calls), 150)} of {len(calls)} call(s))</h2>
    <table>
      <thead>
        <tr>
          <th class="num">#</th><th>Turn</th><th>Model / Step</th>
          <th class="num">System</th><th class="num">History</th><th class="num">User</th><th class="num">Tools/Schema</th>
          <th class="num">Input</th><th class="num">Cached</th><th class="num">Output</th><th class="num">Latency</th><th>Prompts &amp; Response</th>
        </tr>
      </thead>
      <tbody>{"".join(call_rows) or '<tr><td colspan="12" class="sub">No calls recorded</td></tr>'}</tbody>
    </table>
  </div>

  <div class="panel">
    <h2>Findings &amp; Recommendations</h2>
    {"".join(finding_cards)}
  </div>

  <div class="grid-2">
    <div class="panel">
      <h2>Per-Step Token Map</h2>
      <table>
        <thead><tr><th>Step</th><th>Label / Model</th><th class="num">Calls</th><th class="num">Input</th><th class="num">Fixed/Call</th></tr></thead>
        <tbody>{"".join(step_rows) or '<tr><td colspan="5" class="sub">No steps recorded</td></tr>'}</tbody>
      </table>
    </div>
    <div class="panel">
      <h2>Detected Endpoints</h2>
      <table>
        <thead><tr><th>Host</th><th>Class</th><th>API</th><th class="num">Calls</th><th class="num">Input</th></tr></thead>
        <tbody>{"".join(ep_rows) or '<tr><td colspan="5" class="sub">No endpoints recorded</td></tr>'}</tbody>
      </table>
    </div>
  </div>

  <footer>Generated by <strong>tokprof</strong> &middot; Segment counts are local estimates; Unattributed is provider-reported input minus counted segments. Secrets are automatically redacted.</footer>
</div>
</body>
</html>
"""
