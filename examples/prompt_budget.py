#!/usr/bin/env python3
"""Static token map for a multi-step LLM pipeline (no production logs needed).

Describe each step's prompt and how many times it is called per user request, and the script
prints a per-step token map and ranks the largest components by (tokens x calls).

The result is a LOWER BOUND: it counts only the fixed parts you list (system prompts, tool
definitions, output schemas, reference documents). Content that arrives per request (user
data, retrieved passages, conversation history, tool results) is not included.

Input JSON:
{
  "steps": [
    {"name": "guardrail", "model": "small", "calls": 1, "fixed_tokens": 800},
    {"name": "answer", "model": "large", "calls": 2,
     "components": [{"name": "rules", "file": "prompts/answer.txt"},
                    {"name": "reference_doc", "tokens": 20000}]}
  ]
}
A step takes either "fixed_tokens", or "components" (each with "tokens" or a "file" counted
with tiktoken: pip install tiktoken). "min_cacheable" in the top level sets the threshold
used to flag short prompts (default 1024; check your provider's current minimum).

Usage:
    python3 prompt_budget.py sample-pipeline.json [--encoding o200k_base]
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path


def count_file(path: Path, encoding: str) -> int:
    try:
        import tiktoken
    except ImportError:
        raise SystemExit("counting files needs tiktoken: pip install tiktoken")
    return len(tiktoken.get_encoding(encoding).encode(path.read_text()))


def component_tokens(c: dict, base: Path, encoding: str) -> int:
    if "tokens" in c:
        return int(c["tokens"])
    return count_file(base / c["file"], encoding)


def analyze(spec: dict, base: Path, encoding: str) -> dict:
    min_cacheable = int(spec.get("min_cacheable", 1024))
    steps, comps = [], []
    for s in spec["steps"]:
        calls = int(s.get("calls", 1))
        if "components" in s:
            parts = [(c["name"], component_tokens(c, base, encoding)) for c in s["components"]]
            per_call = sum(t for _, t in parts)
        else:
            parts = [("fixed", int(s["fixed_tokens"]))]
            per_call = parts[0][1]
        steps.append({
            "name": s["name"], "model": s.get("model", "-"), "calls": calls,
            "per_call": per_call, "total": per_call * calls,
            "below_min_cacheable": per_call < min_cacheable,
        })
        comps += [{"step": s["name"], "name": n, "per_call": t, "calls": calls, "total": t * calls} for n, t in parts]
    grand = sum(s["total"] for s in steps)
    for s in steps:
        s["share_pct"] = round(100 * s["total"] / grand, 1) if grand else 0.0
    comps.sort(key=lambda c: -c["total"])
    for c in comps:
        c["share_pct"] = round(100 * c["total"] / grand, 1) if grand else 0.0
    return {
        "min_cacheable": min_cacheable,
        "llm_calls_per_request": sum(s["calls"] for s in steps),
        "fixed_tokens_per_request": grand,
        "steps": steps,
        "top_components": comps[:5],
    }


def render(rep: dict) -> str:
    lines = [
        f"LLM calls per user request : {rep['llm_calls_per_request']}",
        f"Fixed tokens per request   : {rep['fixed_tokens_per_request']:,}  (lower bound)",
        "",
        f"{'step':<14}{'model':<8}{'calls':>5}{'per call':>10}{'total':>10}{'share':>8}  note",
    ]
    for s in rep["steps"]:
        note = f"< {rep['min_cacheable']} tokens: likely not cacheable" if s["below_min_cacheable"] else ""
        lines.append(f"{s['name']:<14}{s['model']:<8}{s['calls']:>5}{s['per_call']:>10,}{s['total']:>10,}{s['share_pct']:>7}%  {note}")
    lines += ["", "Largest components (tokens x calls):"]
    for c in rep["top_components"]:
        lines.append(f"  {c['step']}/{c['name']}: {c['per_call']:,} x {c['calls']} = {c['total']:,} ({c['share_pct']}%)")
    return "\n".join(lines)


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("spec", type=Path)
    ap.add_argument("--encoding", default="o200k_base")
    ap.add_argument("--json", action="store_true")
    args = ap.parse_args()
    rep = analyze(json.loads(args.spec.read_text()), args.spec.parent, args.encoding)
    print(json.dumps(rep, indent=2) if args.json else render(rep))


if __name__ == "__main__":
    main()
