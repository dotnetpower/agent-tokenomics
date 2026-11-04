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

    # Failure loops: repeated identical tool calls in one conversation or repeated HTTP/provider errors.
    failed = [r for r in records if r.get("error") or ((r.get("status") or 200) >= 400)]
    if len(failed) >= 2:
        f_tokens = sum(_in(r) for r in failed)
        out.append({
            "severity": "high" if len(failed) >= 3 or f_tokens / total_in >= 0.15 else "medium",
            "text": f"{len(failed)} call(s) failed with an error or HTTP >= 400 ({f_tokens:,} input tokens): check retry/backoff or failure-loop guards",
            "see": LINKS["improve"],
        })
    by_conv: dict = defaultdict(list)
    for r in sorted(records, key=lambda x: x.get("ts", 0)):
        by_conv[r.get("conv") or "_default"].append(r)
    for conv_recs in by_conv.values():
        streak_name, streak_len, streak_tokens = None, 0, 0
        for r in conv_recs:
            tcs = r.get("tool_calls") or [s.get("name") for s in r.get("segments", []) if s["kind"] == "tool_call" and s.get("name")]
            sig = ",".join(str(x) for x in tcs if x) if tcs else None
            if sig and sig == streak_name:
                streak_len += 1
                streak_tokens += _in(r)
            else:
                if streak_name and streak_len >= 3:
                    out.append({
                        "severity": "high",
                        "text": f"Tool call '{streak_name}' was repeated {streak_len} times in a row ({streak_tokens:,} input tokens): add a loop guard that redirects after identical calls",
                        "see": LINKS["improve"],
                    })
                streak_name, streak_len, streak_tokens = sig, (1 if sig else 0), (_in(r) if sig else 0)
        if streak_name and streak_len >= 3:
            out.append({
                "severity": "high",
                "text": f"Tool call '{streak_name}' was repeated {streak_len} times in a row ({streak_tokens:,} input tokens): add a loop guard that redirects after identical calls",
                "see": LINKS["improve"],
            })

    # Prompt cache-busting: system prompt token count stays nearly constant while its hash changes on almost every call.
    sys_per_call = []
    for r in records:
        sys_segs = [s for s in r.get("segments", []) if s["kind"] == "system"]
        if sys_segs:
            sys_per_call.append((sum(s["tokens"] for s in sys_segs), "|".join(s.get("hash", "") for s in sys_segs)))
    if len(sys_per_call) >= 3:
        toks = [t for t, _ in sys_per_call]
        hashes = {h for _, h in sys_per_call if h}
        med = int(statistics.median(toks))
        if med >= 200 and len(hashes) >= max(3, int(0.8 * len(sys_per_call))) and (max(toks) - min(toks)) <= max(32, int(0.08 * med)):
            out.append({
                "severity": "high",
                "text": f"System prompt hash changed on {len(hashes)} of {len(sys_per_call)} calls while its size stayed ~{med:,} tokens: a dynamic value (timestamp, UUID, or unsorted keys) is busting the prefix cache",
                "see": LINKS["improve"],
            })

    # Context growth curve: history + tool_result tokens growing rapidly across turns of a conversation.
    for conv_key, conv_recs in by_conv.items():
        if len(conv_recs) < 4:
            continue
        hist_series = [
            sum(s["tokens"] for s in r.get("segments", []) if s["kind"] in ("history_user", "history_assistant", "tool_result"))
            for r in conv_recs
        ]
        non_zero = [h for h in hist_series if h > 0]
        if len(non_zero) >= 3 and hist_series[-1] >= 1500 and hist_series[-1] >= 3 * non_zero[0]:
            ratio = hist_series[-1] / max(1, non_zero[0])
            out.append({
                "severity": "high" if hist_series[-1] >= 8000 or ratio >= 6 else "medium",
                "text": f"Conversation history grew {ratio:.1f}x across {len(conv_recs)} turns (from {non_zero[0]:,} to {hist_series[-1]:,} tokens): compact older turns or isolate sub-tasks in fresh sessions",
                "see": LINKS["improve"],
            })
            break

    # Attribution quality.
    q = Counter(r.get("quality") for r in records if r.get("quality"))
    poor = q.get("poor", 0)
    if poor and poor / max(1, sum(q.values())) > 0.3:
        neg = sum(1 for r in records if (r.get("residual") or 0) < 0 and r.get("quality") == "poor")
        note = (
            " (reported input is smaller than counted segments on some calls; servers such as Ollama report only newly evaluated "
            "tokens in prompt_eval_count and omit KV-cached prefix tokens)"
            if neg
            else " (opaque content, hidden tokens, or an approximate tokenizer)"
        )
        out.append({"severity": "info", "text": f"{poor} of {sum(q.values())} calls have a large unexplained residual{note}; treat the breakdown as approximate", "see": LINKS["diagnose"]})
    return out


def endpoint_map(records: list) -> list:
    """Group calls by endpoint identity (class + salted host hash), so several hosts can be told apart without host names."""
    groups: dict = {}
    for r in records:
        cls = r.get("host_class") or "unknown"
        key = (cls, r.get("host_id") or "", r.get("provider") or "")
        g = groups.setdefault(key, {"class": cls, "id": (r.get("host_id") or "")[:6], "host": r.get("host"), "api": key[2], "calls": 0, "input": 0, "models": set()})
        g["calls"] += 1
        g["input"] += _in(r)
        if r.get("model"):
            g["models"].add(r["model"])
    out = []
    for g in groups.values():
        g["models"] = sorted(g["models"])
        out.append(g)
    return sorted(out, key=lambda g: -g["input"])


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
        "endpoints": endpoint_map(records),
        "fan_out": [len(g) for g in group_requests(records, gap_s)],
        "first_call_input": _in(first) if first else 0,
        "median_input": int(statistics.median(inputs)) if inputs else 0,
        "findings": findings(records, gap_s=gap_s),
        "kinds_order": list(KINDS) + ["residual"],
    }


# ---- cost estimate, comparison, and legacy export ---------------------------------------------

def estimate_cost(records: list, prices: dict) -> dict:
    """Estimate cost from reported usage. prices: {model: {"input", "cached", "output"}} in currency per 1M tokens.

    A "default" entry covers models without their own entry. Treat the result as an estimate: provider prices change.
    """
    total = 0.0
    missing: set = set()
    for r in records:
        u = r.get("usage") or {}
        p = prices.get(r.get("model") or "") or prices.get("default")
        if not p or u.get("input_total") is None:
            missing.add(r.get("model") or "?")
            continue
        cached = u.get("cached") or 0
        uncached = max(0, u["input_total"] - cached)
        total += (uncached * p.get("input", 0) + cached * p.get("cached", p.get("input", 0)) + (u.get("output") or 0) * p.get("output", 0)) / 1_000_000
    return {"total": total, "models_without_price": sorted(missing)}


def _group_metrics(records: list, gap_s: float) -> dict:
    groups = group_requests(records, gap_s)
    return {
        "input_per_request": [sum(_in(r) for r in g) for g in groups],
        "calls_per_request": [len(g) for g in groups],
    }


def bootstrap_diff(a: list, b: list, n: int = 2000, seed: int = 0) -> Optional[tuple]:
    """Mean(b) - mean(a) with a 95% percentile bootstrap interval; None when a sample is empty."""
    import random

    if not a or not b:
        return None
    rng = random.Random(seed)
    diffs = []
    for _ in range(n):
        ma = sum(rng.choice(a) for _ in a) / len(a)
        mb = sum(rng.choice(b) for _ in b) / len(b)
        diffs.append(mb - ma)
    diffs.sort()
    point = sum(b) / len(b) - sum(a) / len(a)
    return point, diffs[int(0.025 * n)], diffs[int(0.975 * n) - 1]


def compare(before: list, after: list, gap_s: float = 5.0) -> dict:
    ma, mb = _group_metrics(before, gap_s), _group_metrics(after, gap_s)
    out = {"requests": {"before": len(ma["input_per_request"]), "after": len(mb["input_per_request"])}, "metrics": {}}
    for name in ("input_per_request", "calls_per_request"):
        a, b = ma[name], mb[name]
        bd = bootstrap_diff(a, b)
        mean_a = sum(a) / len(a) if a else 0.0
        out["metrics"][name] = {
            "before_mean": mean_a,
            "after_mean": (sum(b) / len(b)) if b else 0.0,
            "diff": bd[0] if bd else None,
            "ci95": [bd[1], bd[2]] if bd else None,
            "relative_change_pct": (100 * bd[0] / mean_a) if (bd and mean_a) else None,
            "significant": bool(bd and (bd[1] > 0 or bd[2] < 0)),
        }
    out["reliable"] = min(out["requests"].values()) >= 5
    return out


def to_legacy(records: list, gap_s: float = 5.0) -> list:
    """Convert records to the JSONL schema used by examples/token_report.py."""
    rows: list = []
    for gi, g in enumerate(group_requests(records, gap_s), 1):
        for r in g:
            u = r.get("usage") or {}
            defs = [{"name": s.get("name") or "tool", "tokens": s["tokens"]} for s in r.get("segments", []) if s["kind"] == "tool_def"]
            rows.append({
                "session": r.get("conv") or r.get("run") or "s",
                "turn": gi,
                "model": r.get("model"),
                "input_tokens": u.get("input_total", r.get("est_input", 0)),
                "cached_input_tokens": u.get("cached", 0) or 0,
                "output_tokens": u.get("output", r.get("output_est") or 0) or 0,
                "tool_defs": defs,
                "tool_calls": [{"name": n, "error": None, "output_tokens": 0} for n in r.get("tool_calls", [])],
            })
    return rows
