---
title: MCP audit
description: Every enabled tool is a per-request tax. Audit servers by usage and scope them per workspace.
track: copilot
verified: 2026-11-03
sidebar:
  order: 9
---

## Problem

Each enabled MCP server contributes **tool definitions (schemas)** that are sent with every model request, used
or not. In an agent loop the cost multiplies:

```text
tokens per task ≈ (tool schema tokens × steps) + accumulated conversation + tool responses
```

An idle server still costs tokens on every step. The more steps the agent takes, the larger the tax.

<figure class="at-doc-figure">
  <div class="at-diag-stack">
    <div class="at-diag-row" aria-label="Five enabled MCP servers: tool definitions 65%, other prefix 35%">
      <span>5 MCP servers</span>
      <div class="at-diag-bar"><div class="at-diag-seg seg-tool" style="width:65%">Tools 65%</div><div class="at-diag-seg seg-prefix" style="width:35%">Other 35%</div></div>
      <span>100%</span>
    </div>
    <div class="at-diag-row" aria-label="One task-relevant MCP server: tool definitions 15%, other prefix 35%">
      <span>1 relevant server</span>
      <div class="at-diag-bar"><div class="at-diag-seg seg-hit" style="width:15%">Tools 15%</div><div class="at-diag-seg seg-prefix" style="width:35%">Other 35%</div></div>
      <span>50%</span>
    </div>
  </div>
  <figcaption>Synthetic illustration, not a measured result: against the original prefix, tool definitions fall from 65% to 15%, while the other prefix stays at 35%; the new total is 50% of the original. Keeping one relevant server reduces the definitions sent on each turn; actual savings vary. Green indicates the pruned configuration, not a cache hit.</figcaption>
</figure>

## Actions

### Run a monthly audit

1. List every MCP server and its tool count (the tools picker, **Configure Tools**, shows them).
2. For each server, note how often you used it last month (Agent Debug Logs show tool calls).
3. Decide per server:

| Usage | Decision |
|-------|----------|
| Used most days | Keep |
| Used occasionally | Keep **disabled by default**, enable per task |
| Rarely/never, or duplicated by another tool | Remove |
| Large server where you need two tools | Use a narrower server or a small CLI the agent calls from the terminal |

4. Re-measure the first-request input size and the baseline task ([Measure your usage](../02-measure-usage/)).

### Scope per workspace, not globally

Put MCP configuration in the repository (`.vscode/mcp.json`) so each project loads only what it needs: a
backend repo does not need design-tool servers; a frontend repo does not need database-schema tools. Reviewing
a per-repo file is also easier than a global list.

### Replace long knowledge with Skills

Repeated or large reference material (a house style, a runbook, API notes) can be packaged as an **agent
skill** that loads on demand instead of sitting in always-on instructions. Benefits: less repeated input,
one place to update, consistent results.

### Keep tool output small

Tool results are often the largest part of the context (file reads, grep, shell output). Prefer targeted
searches over reading whole files, and exclude noisy paths ([Keep context lean](../03-context-hygiene/)).

## How to verify

Record the first-request input tokens in Agent Debug Logs before and after pruning, then run the same task and
compare credits and success. Keep the change only if quality held.

## Trade-offs

A pruned tool the task needed leads to workarounds and extra steps. Keep a fast way to re-enable tools, and prune
by evidence (usage), not by guesswork.

## Illustrative numbers

The talk deck this page draws on describes a team that reduced 187 tools to 52 and reports about a 72% drop in
agent-task token use. We could not verify that case independently; use it as an example of the *method*,
and measure your own result. TokenEconomizer automates the same idea by pruning tools that go unused across
sessions (see the [case study](../../agent-tokenomics/case-tokeneconomizer/)).

## Further reading (papers)

Ordered by venue tier, then citations. Findings are hypotheses to verify on your own workload; see the full list in [Papers](../../papers/).

- [MCP-Zero: Active Tool Discovery for Autonomous LLM Agents](https://arxiv.org/abs/2506.01056) — preprint 2025, 55 citations. Lets the agent request tools on demand instead of injecting all tool schemas.
- [RAG-MCP: Mitigating Prompt Bloat in LLM Tool Selection via Retrieval-Augmented Generation](https://arxiv.org/abs/2505.03275) — preprint 2025, 41 citations. Retrieves only the relevant MCP tool descriptions instead of loading every tool (prompt bloat).
- [Less is More: Optimizing Function Calling for LLM Execution on Edge Devices](https://arxiv.org/abs/2411.15399) — preprint 2024, 33 citations. Presents a smaller selected tool set at function-calling time to cut cost on edge devices.

## Sources

- [Optimize AI credit usage — Keep the tool catalog focused](https://code.visualstudio.com/docs/agents/guides/optimize-usage)
- [Use tools with agents (VS Code)](https://code.visualstudio.com/docs/agents/run/tools)
- Token Efficiency and Context Engineering — developer talk deck (June 2026), MCP sections (figures as reported)
