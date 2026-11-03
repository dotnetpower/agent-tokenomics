"""Analysis over recorded calls: breakdowns, per-step token map, and findings."""

from __future__ import annotations

import statistics
from collections import Counter, defaultdict
from typing import Optional

from .model import KINDS

DOCS = "https://dotnetpower.github.io/agent-tokenomics"
LINKS = {
    "concepts": f"{DOCS}/agent-tokenomics/00-concepts/",
    "diagnose": f"{DOCS}/agent-tokenomics/01-diagnose/",
    "isolate": f"{DOCS}/agent-tokenomics/02-isolate/",
    "improve": f"{DOCS}/agent-tokenomics/04-improve/",
}


def _in(rec: dict) -> int:
    u = rec.get("usage") or {}
    return u.get("input_total") if u.get("input_total") is not None else rec.get("est_input", 0)


def group_requests(records: list, gap_s: float = 5.0) -> list:
    """Group calls into user requests: a new group starts after a quiet gap (fan-out estimate)."""
    groups: list = []
    last_end: Optional[float] = None
    for r in sorted(records, key=lambda r: r.get("ts", 0)):
        start = r.get("ts", 0)
        end = start + (r.get("latency_ms") or 0) / 1000.0
        if last_end is None or start - last_end > gap_s:
            groups.append([])
        groups[-1].append(r)
        last_end = max(last_end or 0, end)
    return groups


def kind_breakdown(records: list) -> dict:
    """Sum input tokens by segment kind plus the unattributed residual."""
    totals: Counter = Counter()
    reported_total = 0
    for r in records:
        for s in r.get("segments", []):
            totals[s["kind"]] += s["tokens"]
        if r.get("residual") is not None:
            totals["residual"] += r["residual"]
        reported_total += _in(r)
    return {"totals": dict(totals), "input_total": reported_total}


def step_map(records: list) -> list:
    steps: dict = defaultdict(lambda: {"calls": 0, "input": 0, "fixed": [], "label": None, "model": None})
    for r in records:
        key = r.get("sysfp") or "(no system prompt)"
        s = steps[key]
        s["calls"] += 1
        s["input"] += _in(r)
        s["label"] = s["label"] or r.get("sys_label")
        s["model"] = s["model"] or r.get("model")
        fixed = sum(x["tokens"] for x in r.get("segments", []) if x["kind"] in ("system", "tool_def", "response_schema"))
        s["fixed"].append(fixed)
    rows = []
    for key, s in steps.items():
        rows.append({
            "step": key[:6], "label": s["label"], "model": s["model"], "calls": s["calls"],
            "input": s["input"], "fixed_per_call": int(statistics.median(s["fixed"])) if s["fixed"] else 0,
        })
    rows.sort(key=lambda x: -x["input"])
    return rows


def _called_tools(records: list) -> set:
    names: set = set()
    for r in records:
        names.update(r.get("tool_calls", []))
        for s in r.get("segments", []):
            if s["kind"] == "tool_call" and s.get("name"):
                names.add(s["name"])
    return names


def findings(records: list, min_cacheable: int = 1024, gap_s: float = 5.0) -> list:
    out: list = []
    if not records:
        return out
    total_in = sum(_in(r) for r in records) or 1

    # Tool definitions that are never called.
    called = _called_tools(records)
    unused: Counter = Counter()
    for r in records:
        for s in r.get("segments", []):
            if s["kind"] == "tool_def" and s.get("name") and s["name"] not in called:
                unused[s["name"]] += s["tokens"]
    if unused:
        paid = sum(unused.values())
        names = ", ".join(n for n, _ in unused.most_common(5))
        out.append({
            "severity": "high" if paid / total_in > 0.1 else "medium",
            "text": f"{len(unused)} tool definition(s) were sent but never called ({paid:,} tokens, {100 * paid / total_in:.1f}% of input): {names}",
            "see": LINKS["isolate"],
        })

    # Repeated static blocks (same hash, large).
    seen: dict = {}
    for r in records:
        for s in r.get("segments", []):
            if s["kind"] in ("system", "tool_def", "response_schema", "other") and s["tokens"] >= 200 and s.get("hash"):
                e = seen.setdefault(s["hash"], {"tokens": s["tokens"], "name": s.get("name"), "kind": s["kind"], "calls": 0})
                e["calls"] += 1
    rep = [e for e in seen.values() if e["calls"] >= 2]
    rep.sort(key=lambda e: -e["tokens"] * e["calls"])
    for e in rep[:3]:
        paid = e["tokens"] * e["calls"]
        if paid / total_in >= 0.05:
            out.append({
                "severity": "high" if paid / total_in >= 0.25 else "medium",
                "text": f"A {e['kind']} block{(' ' + repr(e['name'])) if e['name'] else ''} of {e['tokens']:,} tokens was sent in {e['calls']} calls ({paid:,} tokens, {100 * paid / total_in:.1f}% of input)",
                "see": LINKS["isolate"],
            })

    # Tool results dominating input.
    bd = kind_breakdown(records)["totals"]
    tr = bd.get("tool_result", 0)
    if tr / total_in > 0.4:
        out.append({"severity": "high", "text": f"Tool results are {100 * tr / total_in:.1f}% of input tokens: truncate, paginate, or search before reading", "see": LINKS["improve"]})

    # Cache hit rate.
    cached = sum((r.get("usage") or {}).get("cached", 0) for r in records)
    have_cache = any("cached" in (r.get("usage") or {}) for r in records)
    if have_cache and len(records) > 3:
        pct = 100 * cached / total_in
        if pct < 30:
            out.append({"severity": "medium", "text": f"Cache hit rate is {pct:.1f}%. Check that stable content comes first and the history is append-only", "see": LINKS["concepts"]})

    # Short prompts that cannot be cached.
    short = sum(1 for r in records if _in(r) < min_cacheable)
    if short and short == len(records):
        out.append({"severity": "low", "text": f"All calls are under {min_cacheable} tokens; provider prompt caching typically needs a longer prefix (check your provider's minimum)", "see": LINKS["concepts"]})

    # Fan-out.
    groups = group_requests(records, gap_s)
    sizes = [len(g) for g in groups]
    if sizes and statistics.median(sizes) >= 4:
        out.append({"severity": "medium", "text": f"Each user request triggers a median of {statistics.median(sizes):g} LLM calls (max {max(sizes)}): look for steps to skip, merge, or run on a smaller model", "see": LINKS["improve"]})

    # Attribution quality.
    q = Counter(r.get("quality") for r in records if r.get("quality"))
    poor = q.get("poor", 0)
    if poor and poor / max(1, sum(q.values())) > 0.3:
        out.append({"severity": "info", "text": f"{poor} of {sum(q.values())} calls have a large unexplained residual (opaque content, hidden tokens, or an approximate tokenizer); treat the breakdown as approximate", "see": LINKS["diagnose"]})
    return out


def summarize(records: list, gap_s: float = 5.0) -> dict:
    bd = kind_breakdown(records)
    inputs = [_in(r) for r in records]
    first = records[0] if records else {}
    return {
        "calls": len(records),
        "input_total": bd["input_total"],
        "output_total": sum((r.get("usage") or {}).get("output", 0) or 0 for r in records),
        "cached_total": sum((r.get("usage") or {}).get("cached", 0) or 0 for r in records),
        "by_kind": bd["totals"],
        "steps": step_map(records),
        "fan_out": [len(g) for g in group_requests(records, gap_s)],
        "first_call_input": _in(first) if first else 0,
        "median_input": int(statistics.median(inputs)) if inputs else 0,
        "findings": findings(records, gap_s=gap_s),
        "kinds_order": list(KINDS) + ["residual"],
    }
