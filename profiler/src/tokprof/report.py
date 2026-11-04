"""Text, terminal, Markdown, JSON, and self-contained HTML reports."""

from __future__ import annotations

import html
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


def _ubo_bar(share: float, width: int = 24) -> str:
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
    eps = s["endpoints"]
    if len(eps) > 1 or (eps and eps[0]["class"] not in ("unknown",)):
        lines += ["", "Endpoints (by detected API and host class)", "------------------------------------------",
                  f"{'host':<30}{'class':<18}{'api':<18}{'calls':>6}{'input':>12}"]
        for e in eps[:12]:
            who = e["host"] or (f"#{e['id']}" if e["id"] else "-")
            lines.append(f"{who:<30}{e['class']:<18}{e['api']:<18}{e['calls']:>6}{e['input']:>12,}")
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


def render_pretty(records: list, gap_s: float = 5.0, *, color: Optional[bool] = None, prices: Optional[dict] = None) -> str:
    """Rich Unicode terminal report with box-drawing cards, smooth bars, and optional ANSI colors."""
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

    calls_str = c("bold", f"{s['calls']:,}")
    in_str = c("bold", f"{s['input_total']:,}")
    out_str = c("bold", f"{s['output_total']:,}")
    cached_str = c("bold", f"{s['cached_total']:,}")
    lines = [
        c("bold", "╭──────────────────────────────────────────────────────────────────────────────╮"),
        c("bold", "│  tokprof — LLM Token Profile                                                 │"),
        c("bold", "╰──────────────────────────────────────────────────────────────────────────────╯"),
        f"  Calls: {calls_str}   Input: {in_str}   Output: {out_str}   Cached: {cached_str} ({cache_pct:.1f}%)",
        f"  First-call overhead: {s['first_call_input']:,} tok   "
        f"Median input/call: {s['median_input']:,} tok   "
        f"Fan-out: median {med_fo}, max {max_fo} ({len(fo)} req)",
    ]
    if prices:
        lines.append("  " + render_cost(records, prices))

    lines += ["", c("bold", "  Input token breakdown"), "  " + "─" * 76]
    for kind in s["kinds_order"]:
        v = s["by_kind"].get(kind, 0)
        if v:
            share = v / total
            bar = c(kind, _ubo_bar(max(0.0, min(1.0, share)), 26))
            lines.append(f"  {LABELS.get(kind, kind):<32}{v:>12,}  {100 * share:5.1f}%  {bar}")

    if len(records) > 1:
        lines += ["", c("bold", "  Call timeline (input tokens per call)"), "  " + "─" * 76]
        max_call_in = max((r.get("usage") or {}).get("input_total") or r.get("est_input", 0) or 1 for r in records[:20])
        for idx, r in enumerate(records[:20], 1):
            u = r.get("usage") or {}
            inp = u.get("input_total") if u.get("input_total") is not None else r.get("est_input", 0)
            cached = u.get("cached") or 0
            model = (r.get("model") or r.get("provider") or "-")[:18]
            lat = f"{r['latency_ms']}ms" if r.get("latency_ms") is not None else "-"
            bar = _ubo_bar(inp / max_call_in, 20)
            cache_tag = f" (cached {cached:,})" if cached else ""
            lines.append(f"  #{idx:<3} {model:<18} {inp:>10,} tok  {bar}  {lat:>7}{cache_tag}")
        if len(records) > 20:
            lines.append(f"  ... ({len(records) - 20} more call(s))")

    if len(s["steps"]) > 1 or (s["steps"] and s["steps"][0]["step"] != "(no sy"):
        lines += [
            "",
            c("bold", "  Per-step token map (grouped by system prompt)"),
            "  " + "─" * 76,
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
            "  " + "─" * 76,
            f"  {'host':<28}{'class':<18}{'api':<16}{'calls':>6}{'input':>12}",
        ]
        for e in eps[:12]:
            who = (e["host"] or (f"#{e['id']}" if e["id"] else "-"))[:26]
            lines.append(f"  {who:<28}{e['class']:<18}{e['api']:<16}{e['calls']:>6}{e['input']:>12,}")

    if s["findings"]:
        lines += ["", c("bold", "  Findings & recommendations"), "  " + "─" * 76]
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

    # Per-call timeline rows
    call_rows = []
    max_in = max(((r.get("usage") or {}).get("input_total") or r.get("est_input", 0) or 1) for r in records) if records else 1
    for idx, r in enumerate(records[:100], 1):
        u = r.get("usage") or {}
        inp = u.get("input_total") if u.get("input_total") is not None else r.get("est_input", 0)
        out_tok = u.get("output") or r.get("output_est") or 0
        cached = u.get("cached") or 0
        by_k: dict = {}
        for seg in r.get("segments", []):
            by_k[seg["kind"]] = by_k.get(seg["kind"], 0) + seg["tokens"]
        if (r.get("residual") or 0) > 0:
            by_k["residual"] = r["residual"]
        bars = []
        for k in s["kinds_order"]:
            kv = by_k.get(k, 0)
            if kv > 0:
                w = 100.0 * kv / max_in
                bars.append(
                    f'<span style="width:{w:.2f}%;background:{COLORS_HEX.get(k, "#94a3b8")}" '
                    f'title="{html.escape(LABELS.get(k, k))}: {kv:,} tok"></span>'
                )
        model = html.escape(str(r.get("model") or r.get("provider") or "-"))
        lat = f"{r['latency_ms']:,} ms" if r.get("latency_ms") is not None else "-"
        call_rows.append(
            f"<tr>"
            f'<td class="num">#{idx}</td>'
            f"<td><code>{model}</code></td>"
            f'<td class="num">{inp:,}</td>'
            f'<td class="num">{cached:,}</td>'
            f'<td class="num">{out_tok:,}</td>'
            f'<td class="num">{lat}</td>'
            f'<td><div class="call-bar">{"".join(bars)}</div></td>'
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
.call-bar {{ display: flex; height: 12px; width: 220px; background: var(--border); border-radius: 4px; overflow: hidden; }}
.call-bar span {{ display: block; height: 100%; }}
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
    <div class="card"><div class="k-label">LLM Calls</div><div class="k-val">{s["calls"]:,}</div><div class="k-sub">{len(fo)} request group(s)</div></div>
    <div class="card"><div class="k-label">Input Tokens</div><div class="k-val">{s["input_total"]:,}</div><div class="k-sub">median {s["median_input"]:,} / call</div></div>
    <div class="card"><div class="k-label">Cached Input</div><div class="k-val">{s["cached_total"]:,}</div><div class="k-sub">{cache_pct:.1f}% cache hit rate</div></div>
    <div class="card"><div class="k-label">Output Tokens</div><div class="k-val">{s["output_total"]:,}</div><div class="k-sub">generated</div></div>
    <div class="card"><div class="k-label">Fixed Overhead</div><div class="k-val">{s["first_call_input"]:,}</div><div class="k-sub">first call input</div></div>
    <div class="card"><div class="k-label">Fan-out</div><div class="k-val">{med_fo}</div><div class="k-sub">median calls/req (max {max_fo})</div></div>
    {cost_card}
  </div>

  <div class="panel">
    <h2>Input Token Breakdown</h2>
    <div class="stack-bar">{"".join(stack_divs)}</div>
    <table>
      <thead><tr><th>Component</th><th class="num">Tokens</th><th class="num">Share</th><th>Distribution</th></tr></thead>
      <tbody>{"".join(breakdown_rows)}</tbody>
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

  <div class="panel">
    <h2>Per-Call Timeline ({min(len(records), 100)} of {len(records)} call(s))</h2>
    <table>
      <thead><tr><th class="num">#</th><th>Model</th><th class="num">Input</th><th class="num">Cached</th><th class="num">Output</th><th class="num">Latency</th><th>Composition</th></tr></thead>
      <tbody>{"".join(call_rows) or '<tr><td colspan="7" class="sub">No calls recorded</td></tr>'}</tbody>
    </table>
  </div>

  <footer>Generated by <strong>tokprof</strong> &middot; Segment counts are local estimates; Unattributed is provider-reported input minus counted segments. Providers/prompts/keys are never stored.</footer>
</div>
</body>
</html>
"""
