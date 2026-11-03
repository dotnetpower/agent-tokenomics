#!/usr/bin/env python3
"""Build the ranked paper list from papers.json + papers-meta.json.

Ranking ("authority" proxy, stated openly on the page):
  tier 1  peer-reviewed at a flagship ML / NLP / systems venue
  tier 2  other peer-reviewed venue
  tier 3  preprint (no venue listed)
  within a tier: citation count, descending.

Usage:
    python3 build_papers.py                       # write site/src/content/docs/papers.md
    python3 build_papers.py --topic caching --top 4   # print a markdown bullet list for one topic
"""

from __future__ import annotations

import argparse
import json
import re
from pathlib import Path

HERE = Path(__file__).parent
OUT = HERE.parent / "site" / "src" / "content" / "docs" / "papers.md"

TIER1 = re.compile(
    r"International Conference on Learning Representations|Neural Information Processing Systems|"
    r"Annual Meeting of the Association for Computational Linguistics|"
    r"Conference on Empirical Methods in Natural Language Processing|"
    r"North American Chapter of the Association for Computational Linguistics|"
    r"Transactions of the Association for Computational Linguistics|Trans\. Mach\. Learn\. Res\.|"
    r"International Conference on Machine Learning|Symposium on Operating Systems Principles|"
    r"Conference on Machine Learning and Systems"
)
SHORT = {
    "International Conference on Learning Representations": "ICLR",
    "Neural Information Processing Systems": "NeurIPS",
    "Annual Meeting of the Association for Computational Linguistics": "ACL",
    "Conference on Empirical Methods in Natural Language Processing": "EMNLP",
    "North American Chapter of the Association for Computational Linguistics": "NAACL",
    "Transactions of the Association for Computational Linguistics": "TACL",
    "Trans. Mach. Learn. Res.": "TMLR",
    "International Conference on Machine Learning": "ICML",
    "Symposium on Operating Systems Principles": "SOSP",
    "Conference on Machine Learning and Systems": "MLSys",
    "ACM Transactions on Computer Systems": "ACM TOCS",
    "ACM Transactions on Storage": "ACM ToS",
    "Proc. ACM Softw. Eng.": "PACMSE",
    "European Conference on Artificial Intelligence": "ECAI",
    "Neurocomputing": "Neurocomputing",
}
TOPICS = {
    "foundations": ("Foundations", "[Concepts](../agent-tokenomics/00-concepts/)"),
    "tokenization": ("Tokenization and language", "[Language and tokenization](../copilot-tokenomics/07-language-and-tokenization/)"),
    "long-context": ("Long context and context rot", "[Concepts](../agent-tokenomics/00-concepts/), [Keep context lean](../copilot-tokenomics/03-context-hygiene/), [Improve](../agent-tokenomics/04-improve/)"),
    "caching": ("Prompt / KV caching", "[Concepts](../agent-tokenomics/00-concepts/), [Measure your usage](../copilot-tokenomics/02-measure-usage/), [Improve](../agent-tokenomics/04-improve/)"),
    "compression": ("Prompt compression", "[Prompt and output style](../copilot-tokenomics/06-prompt-and-output-style/), [Improve](../agent-tokenomics/04-improve/)"),
    "format": ("Output format", "[Prompt and output style](../copilot-tokenomics/06-prompt-and-output-style/)"),
    "batching": ("Batching", "[Instructions and tools](../copilot-tokenomics/04-instructions-and-tools/), [Improve](../agent-tokenomics/04-improve/)"),
    "routing": ("Model routing and cascades", "[Models, planning, delegation](../copilot-tokenomics/05-models-and-planning/), [Improve](../agent-tokenomics/04-improve/)"),
    "reasoning": ("Efficient reasoning / output length", "[Agent modes and budget guards](../copilot-tokenomics/08-agent-modes-and-budget/), [Improve](../agent-tokenomics/04-improve/)"),
    "agents": ("Agent cost and context management", "[Isolate](../agent-tokenomics/02-isolate/), [Improve](../agent-tokenomics/04-improve/), [TokenEconomizer case study](../agent-tokenomics/case-tokeneconomizer/)"),
    "tools": ("Tool selection and schema size", "[MCP audit](../copilot-tokenomics/09-mcp-audit/), [Improve](../agent-tokenomics/04-improve/)"),
    "instructions": ("Instruction / context files", "[Instructions and tools](../copilot-tokenomics/04-instructions-and-tools/)"),
    "memory": ("Memory and summarization", "[Prove](../agent-tokenomics/03-prove/), [Improve: compaction](../agent-tokenomics/04-improve/#compaction-without-losing-the-cache)"),
    "evaluation": ("Evaluation and statistics", "[Prove](../agent-tokenomics/03-prove/), [Result](../agent-tokenomics/05-result/)"),
    "survey": ("Surveys", "[Resources](../resources/)"),
}


def load() -> list[dict]:
    papers = json.loads((HERE / "papers.json").read_text())
    meta = json.loads((HERE / "papers-meta.json").read_text())
    rows = []
    for p in papers:
        m = meta["papers"][p["id"]]
        venue = m["venue"]
        tier = 1 if TIER1.search(venue) else (2 if venue and venue != "arXiv.org" else 3)
        rows.append({**p, **m, "tier": tier, "venue_short": SHORT.get(venue, "") or "preprint"})
    rows.sort(key=lambda r: (r["tier"], -r["citations"], r["id"]))
    for i, r in enumerate(rows, 1):
        r["rank"] = i
    return rows


def link(r: dict) -> str:
    title = r["title"].replace("|", "\\|")
    return f"[{title}](https://arxiv.org/abs/{r['id']})"


def bullets(rows: list[dict], topic: str, top: int) -> str:
    sel = [r for r in rows if r["topic"] == topic][:top]
    return "\n".join(
        f"- {link(r)} — {r['venue_short']} {r['year']}, {r['citations']:,} citations. {r['note']}" for r in sel
    )


SHORT_NAMES = {
    "2307.03172": "Lost in the Middle",
    "2309.06180": "vLLM / PagedAttention",
    "2310.06201": "Selective Context",
    "2305.14788": "AutoCompressors",
    "2502.18600": "Chain of Draft",
    "2412.18547": "Token-Budget-Aware Reasoning",
    "2412.21187": "Overthinking of o1-like LLMs",
    "2502.04463": "Training LMs to Reason Efficiently",
    "2503.04697": "L1 (length-controlled reasoning)",
    "2508.21433": "The Complexity Trap (observation masking)",
    "2509.23586": "Trajectory Reduction",
    "2508.02694": "Efficient Agents",
    "2407.01489": "Agentless",
    "2405.15793": "SWE-agent",
    "2310.08560": "MemGPT",
    "2308.15022": "Recursively Summarizing",
    "2504.19413": "Mem0",
    "2504.13359": "Cost-of-Pass",
    "2407.01502": "AI Agents That Matter",
    "2406.12045": "tau-bench (pass^k)",
    "2411.00640": "Error Bars for Evals",
    "2408.04667": "Non-Determinism of LLMs",
    "2410.10813": "LongMemEval",
    "2402.17753": "LoCoMo",
    "2505.06120": "LLMs Get Lost in Multi-Turn Conversation",
    "2402.14848": "Same Task, More Tokens",
    "2302.00093": "Distracted by Irrelevant Context",
    "2305.15425": "Tokenizer Unfairness Across Languages",
    "2305.13707": "Do All Languages Cost the Same?",
    "1508.07909": "BPE subword units",
    "2408.02442": "Let Me Speak Freely?",
    "2301.08721": "Batch Prompting",
    "2602.11988": "Evaluating AGENTS.md",
    "2610.22732": "TokenEconomizer",
    "2601.06007": "Don't Break the Cache",
    "2505.03275": "RAG-MCP",
    "2506.01056": "MCP-Zero",
    "2411.15399": "Less is More (function calling)",
    "2409.07429": "Agent Workflow Memory",
    "2510.04618": "Agentic Context Engineering",
    "2305.05176": "FrugalGPT",
    "2406.18665": "RouteLLM",
    "2404.14618": "Hybrid LLM",
    "2310.12963": "AutoMix",
}


def short(r: dict) -> str:
    return SHORT_NAMES.get(r["id"]) or r["title"].split(":")[0]


def by_ids(rows: list[dict], ids: list[str]) -> list[dict]:
    wanted = {r["id"]: r for r in rows}
    missing = [i for i in ids if i not in wanted]
    if missing:
        raise SystemExit(f"unknown ids: {missing}")
    return sorted((wanted[i] for i in ids), key=lambda r: r["rank"])


def inline(r: dict) -> str:
    return f"[{short(r)}](https://arxiv.org/abs/{r['id']}) ({r['venue_short']} {r['year']})"


def page(rows: list[dict], retrieved: str) -> str:
    out = [
        "---",
        "title: Papers",
        f"description: {len(rows)} papers on saving tokens, ranked by venue tier and citations.",
        "sidebar:",
        "  order: 91",
        "---",
        "",
        f"{len(rows)} papers relevant to saving tokens, compiled on **{retrieved}** and ranked by a transparent",
        "authority proxy. Each entry links to its arXiv page; takeaways are our own one-line summaries.",
        "",
        "## How the ranking works",
        "",
        "1. **Tier 1** — peer-reviewed at a flagship ML / NLP / systems venue (ICLR, NeurIPS, ICML, ACL, EMNLP, NAACL,",
        "   TACL, TMLR, SOSP, MLSys).",
        "2. **Tier 2** — other peer-reviewed venues (journals and other conferences).",
        "3. **Tier 3** — preprints (no venue listed).",
        "4. Within a tier, **more citations first**.",
        "",
        ":::caution[Read the ranking as a reading order, not a verdict]",
        "- Year is the year Semantic Scholar lists for the paper (often its first preprint year, which can precede the",
        "  venue year).",
        "- Venue and citation count come from the [Semantic Scholar](https://www.semanticscholar.org/) Graph API on",
        f"  {retrieved}. Venues can lag the final publication, and citations keep changing. Re-run",
        "  `examples/fetch_paper_meta.py` then `examples/build_papers.py` to refresh.",
        "- Citations favor older and foundational work; recent preprints rank low simply because they are new.",
        "- Peer review and citations do not guarantee a result holds for your agent or model. Treat every finding as a",
        "  hypothesis and verify it with the [troubleshooting procedure](../agent-tokenomics/).",
        "- We summarize the claim, not the numbers. Read the paper for the setup before relying on a figure.",
        ":::",
        "",
        "## Where each topic is applied",
        "",
        "| Topic | Papers | Used in |",
        "|-------|-------:|---------|",
    ]
    for key, (label, where) in TOPICS.items():
        n = sum(1 for r in rows if r["topic"] == key)
        out.append(f"| {label} | {n} | {where} |")
    out += [
        "",
        "## Ranked list",
        "",
        "| # | Paper | Venue · Year | Cited | Topic | Takeaway |",
        "|--:|-------|--------------|------:|-------|----------|",
    ]
    for r in rows:
        label = TOPICS[r["topic"]][0]
        out.append(
            f"| {r['rank']} | {link(r)} | {r['venue_short']} · {r['year']} | {r['citations']:,} | {label} | {r['note']} |"
        )
    out += [
        "",
        "## Suggest a paper",
        "",
        "Add an entry to `examples/papers.json` (arXiv id, one topic, a one-line note in your own words), run the two",
        "scripts, and open a PR. The metadata fetch fails on an unknown id, which guards against typos.",
        "",
    ]
    return "\n".join(out)


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--topic")
    ap.add_argument("--top", type=int, default=4)
    ap.add_argument("--ids", help="comma-separated arXiv ids; prints bullets ordered by rank")
    ap.add_argument("--inline", help="comma-separated arXiv ids; prints a compact inline list")
    args = ap.parse_args()
    rows = load()
    if args.ids:
        print("\n".join(
            f"- {link(r)} — {r['venue_short']} {r['year']}, {r['citations']:,} citations. {r['note']}"
            for r in by_ids(rows, args.ids.split(","))))
        return
    if args.inline:
        print("; ".join(inline(r) for r in by_ids(rows, args.inline.split(","))))
        return
    if args.topic:
        print(bullets(rows, args.topic, args.top))
        return
    retrieved = json.loads((HERE / "papers-meta.json").read_text())["retrieved"]
    OUT.write_text(page(rows, retrieved))
    print(f"wrote {OUT} ({len(rows)} papers)")


if __name__ == "__main__":
    main()
