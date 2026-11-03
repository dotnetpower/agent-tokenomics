#!/usr/bin/env python3
"""Summarize where an LLM agent's tokens go, from a JSONL request log.

One JSON object per model request. Fields (all optional except session/turn):

    session, turn           identifiers (turn = one user prompt)
    model                   model name
    input_tokens            prompt tokens billed for this request
    cached_input_tokens     part of input_tokens served from prompt cache
    output_tokens           completion tokens (include reasoning if billed together)
    tool_defs               [{"name": str, "tokens": int}] tool definitions sent
    tool_calls              [{"name": str, "error": str|null, "output_tokens": int}]

Usage:
    python token_report.py requests.jsonl [--loop-threshold 3] [--json]
"""

from __future__ import annotations

import argparse
import json
import statistics
import sys
from collections import Counter, defaultdict
from pathlib import Path


def load(path: Path) -> list[dict]:
    rows = []
    for n, line in enumerate(path.read_text().splitlines(), 1):
        if not line.strip():
            continue
        try:
            rows.append(json.loads(line))
        except json.JSONDecodeError as exc:
            sys.exit(f"{path}:{n}: invalid JSON: {exc}")
    return rows


def pct(part: int, whole: int) -> float:
    return round(100 * part / whole, 1) if whole else 0.0


def analyze(rows: list[dict], loop_threshold: int) -> dict:
    by_session: dict[str, list[dict]] = defaultdict(list)
    for r in rows:
        by_session[r["session"]].append(r)

    sessions = {}
    for sid, reqs in by_session.items():
        inp = sum(r.get("input_tokens", 0) for r in reqs)
        cached = sum(r.get("cached_input_tokens", 0) for r in reqs)
        out = sum(r.get("output_tokens", 0) for r in reqs)
        sessions[sid] = {
            "requests": len(reqs),
            "input_tokens": inp,
            "cached_input_tokens": cached,
            "output_tokens": out,
            "cache_hit_pct": pct(cached, inp),
            # First request approximates the fixed prefix: system prompt,
            # instructions, tool definitions, before any history exists.
            "fixed_overhead_tokens": reqs[0].get("input_tokens", 0),
            "context_growth": [r.get("input_tokens", 0) for r in reqs],
        }

    # Tool definitions are paid on every request; compare with actual use.
    defined: dict[str, int] = defaultdict(int)  # name -> tokens sent in total
    called: Counter[str] = Counter()
    out_by_tool: Counter[str] = Counter()
    for r in rows:
        for d in r.get("tool_defs", []):
            defined[d["name"]] += d.get("tokens", 0)
        for c in r.get("tool_calls", []):
            called[c["name"]] += 1
            out_by_tool[c["name"]] += c.get("output_tokens", 0)
    unused = {n: t for n, t in defined.items() if called[n] == 0}

    # Failure loops: the same error signature repeated within one turn.
    errors: dict[tuple[str, str], Counter[str]] = defaultdict(Counter)
    for r in rows:
        for c in r.get("tool_calls", []):
            if c.get("error"):
                errors[(r["session"], str(r["turn"]))][f'{c["name"]}: {c["error"]}'] += 1
    loops = [
        {"session": s, "turn": t, "signature": sig, "count": n}
        for (s, t), counter in errors.items()
        for sig, n in counter.items()
        if n >= loop_threshold
    ]

    return {
        "totals": {
            "requests": len(rows),
            "input_tokens": sum(s["input_tokens"] for s in sessions.values()),
            "cached_input_tokens": sum(s["cached_input_tokens"] for s in sessions.values()),
            "output_tokens": sum(s["output_tokens"] for s in sessions.values()),
        },
        "sessions": sessions,
        "unused_tools": dict(sorted(unused.items(), key=lambda kv: -kv[1])),
        "tool_output_tokens": dict(out_by_tool.most_common()),
        "failure_loops": loops,
        "median_fixed_overhead": statistics.median(
            s["fixed_overhead_tokens"] for s in sessions.values()
        ) if sessions else 0,
    }


def render(rep: dict) -> str:
    t = rep["totals"]
    lines = [
        "== Totals ==",
        f'requests={t["requests"]} input={t["input_tokens"]} '
        f'cached={t["cached_input_tokens"]} ({pct(t["cached_input_tokens"], t["input_tokens"])}%) '
        f'output={t["output_tokens"]}',
        "",
        "== Per session (fixed overhead = first request input) ==",
    ]
    for sid, s in rep["sessions"].items():
        growth = " -> ".join(str(x) for x in s["context_growth"])
        lines.append(
            f'{sid}: requests={s["requests"]} input={s["input_tokens"]} '
            f'cache_hit={s["cache_hit_pct"]}% overhead={s["fixed_overhead_tokens"]} '
            f'growth: {growth}'
        )
    lines += ["", "== Tool definitions never called (tokens paid in total) =="]
    lines += [f"{n}: {tok}" for n, tok in rep["unused_tools"].items()] or ["(none)"]
    lines += ["", "== Tool output tokens by tool =="]
    lines += [f"{n}: {tok}" for n, tok in rep["tool_output_tokens"].items()] or ["(none)"]
    lines += ["", "== Repeated identical errors within a turn =="]
    lines += [
        f'{l["session"]}/turn {l["turn"]}: {l["count"]}x {l["signature"]}'
        for l in rep["failure_loops"]
    ] or ["(none)"]
    return "\n".join(lines)


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("log", type=Path)
    ap.add_argument("--loop-threshold", type=int, default=3)
    ap.add_argument("--json", action="store_true")
    args = ap.parse_args()
    rep = analyze(load(args.log), args.loop_threshold)
    print(json.dumps(rep, indent=2) if args.json else render(rep))


if __name__ == "__main__":
    main()
