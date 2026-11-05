---
title: Instructions and tools
description: Trim what is sent on every request - custom instructions and the tool catalog.
track: copilot
verified: 2026-11-03
sidebar:
  order: 4
---

## Problem

Some content is paid for on **every request**: custom instructions and the definitions of every enabled
tool (including each MCP server's tools). Unused or bloated items are a fixed tax.

<figure class="at-doc-figure">
  <div class="at-diag-stack">
    <div class="at-diag-row" aria-label="Unscoped instructions and 40 tools: fixed overhead 82%, task 18%">
      <span>Unscoped instructions + 40 tools</span>
      <div class="at-diag-bar"><div class="at-diag-seg seg-miss" style="width:82%">Fixed 82%</div><div class="at-diag-seg seg-user" style="width:18%">Task 18%</div></div>
      <span>100%</span>
    </div>
    <div class="at-diag-row" aria-label="applyTo scoped and pruned tools: fixed overhead 22%, task 18%">
      <span>applyTo scoped + pruned tools</span>
      <div class="at-diag-bar"><div class="at-diag-seg seg-hit" style="width:22%">Fixed 22%</div><div class="at-diag-seg seg-user" style="width:18%">Task 18%</div></div>
      <span>40%</span>
    </div>
  </div>
  <figcaption>Synthetic illustration, not a measured result: against the original input, fixed overhead falls from 82% to 22%, and the task remains at 18%; the new total is 40% of the original. Scoping and pruning reduce fixed overhead while the task stays the same; actual savings vary. Colors distinguish configurations, not cache hits or misses.</figcaption>
</figure>

## Actions

**Custom instructions** (`.github/copilot-instructions.md`, `*.instructions.md`, `AGENTS.md`)

- Keep only *stable, high-value* facts: build/test commands that differ from defaults, required env
  vars, where generated files live, conventions the agent cannot infer.
- Scope with `applyTo` in `*.instructions.md` so language- or folder-specific rules load only where they
  apply. Think in three tiers: **always-on** (few rules that apply every time), **conditional** (`applyTo`
  path rules), and **on-demand** (skills or notes for occasional workflows such as PR review).
- Delete anything the agent can discover cheaply from the code. Do not paste long style guides.
- Facts stated once in instructions save the agent from rediscovering them in every session.
- Merge duplicates across `AGENTS.md` and `copilot-instructions.md`; duplicated text is billed twice.
- Write the rules tersely (declarative rules, no filler); the file is re-sent on every request.
- For tool-routing and fallback instructions, consider a **BPE-aligned formula layout** (`need => tool`, `condition -> action`, `>` priority chains; see [Prompt and output style](../06-prompt-and-output-style/#4-bpe-aligned-formula-notation)). As a VS Code reference case, converting the auto-loaded `.instructions.md` in [dotnetpower/code-context](https://github.com/dotnetpower/code-context) from 106-line prose (9,127 bytes, 2,048 `o200k_base` tokens) to a 26-line BPE-aligned formula specification (2,688 bytes, 718 tokens) cut per-turn instruction tokens by **64.9%** while retaining **100% (31/31)** rule-recognition accuracy across 12 edge-case scenarios.

**Do not auto-generate and forget.** An ETH Zurich study of repository context files (arXiv 2602.11988) found
that context files do not generally improve task success while increasing inference cost by over 20% on
average; repository overviews were not helpful, while instructions for **non-standard practices** were
followed. Treat the file like a bug tracker: add a line when the agent hits a real "landmine", delete lines the
agent can learn from the code, and re-evaluate when the code changes.

| Keep (landmines) | Delete (self-discoverable) |
|------------------|----------------------------|
| "Use `uv`, not `pip`" | "This is a Python project" |
| "Run tests with `--no-cache`" | "Tests live in `tests/`" |
| "Do not refactor `auth/` — audit pending" | "We use PostgreSQL" |
| "Deploy requires VPN" | "Code quality matters" (empty advice) |

**Tool catalog**

- Use **Configure Tools** in the chat input to enable only the tools or MCP servers the current task needs.
- In custom agents, set the `tools` property to a narrow, task-specific list.
- Prefer one focused CLI (invoked through the terminal tool) over loading dozens of single-purpose MCP tools
  — if your organization can maintain and secure it.
- Choose tools **before** starting a session: enabling an MCP server mid-session changes the tool
  definitions and can break prompt-cache reuse (visible as the first divergence in the Cache Explorer).

**Batch deterministic loops**

- For bulk or polling workflows, run a reviewed script via the terminal and give the model only the summary,
  rather than many tool calls whose outputs all land in context.

## How to verify

In Agent Debug Logs, compare input tokens of the first request before and after trimming. Then run the
baseline task and compare session credits. Re-check quality: did the agent start rediscovering things?

## Trade-offs

- Over-pruned instructions cause repeated discovery; over-pruned tools cause failed or manual workarounds.
- Benchmark scripted versus interactive versions — savings depend on the tools and results.

## Further reading (papers)

Ordered by venue tier, then citations. Findings are hypotheses to verify on your own workload; see the full list in [Papers](../../papers/).

- [Batch Prompting: Efficient Inference with Large Language Model APIs](https://arxiv.org/abs/2301.08721) — EMNLP 2023, 154 citations. Packs several samples into one prompt so instruction tokens are shared.
- [MCP-Zero: Active Tool Discovery for Autonomous LLM Agents](https://arxiv.org/abs/2506.01056) — preprint 2025, 55 citations. Lets the agent request tools on demand instead of injecting all tool schemas.
- [RAG-MCP: Mitigating Prompt Bloat in LLM Tool Selection via Retrieval-Augmented Generation](https://arxiv.org/abs/2505.03275) — preprint 2025, 41 citations. Retrieves only the relevant MCP tool descriptions instead of loading every tool (prompt bloat).
- [Evaluating AGENTS.md: Are Repository-Level Context Files Helpful for Coding Agents?](https://arxiv.org/abs/2602.11988) — preprint 2026, 36 citations. Repository context files did not generally improve task success and raised inference cost by over 20% on average.

## Sources

- [Optimize AI credit usage — Keep the tool catalog focused, Batch repetitive operations, Ground the agent](https://code.visualstudio.com/docs/agents/guides/optimize-usage)
- [Diagnose prompt caching with the Cache Explorer](https://code.visualstudio.com/docs/agents/agent-troubleshooting/cache-explorer)
- [Evaluating AGENTS.md: Are Repository-Level Context Files Helpful for Coding Agents? (arXiv:2602.11988)](https://arxiv.org/abs/2602.11988)
- [MCP audit](../09-mcp-audit/) for the deeper tool-catalog procedure
