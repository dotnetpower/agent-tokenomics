---
title: "Case study: TokenEconomizer"
description: What microsoft/TokenEconomizer does and how it maps to the troubleshooting stages.
track: agent
verified: 2026-11-03
sidebar:
  order: 6
---

[TokenEconomizer](https://github.com/microsoft/TokenEconomizer) is an experience-driven, black-box
cost-control plugin for coding agents (Claude Code, Codex, GitHub Copilot CLI, Pi; experimental
opencode and Cursor Agent). It learns from an agent's earlier sessions **in the same repository** and
does not change the agent's model or code. This page summarizes its published design in our own words.

## Three mechanisms

| Mechanism | What it does | Defaults reported |
|-----------|--------------|-------------------|
| **Micro-skills** | Learns repo facts (command spellings, env vars, build order, file locations) from repeated failures and from a model that reviews recent sessions; states a fact at session start once confirmed, drops it if contradicted | Review every *k* = 3 sessions; confirm after 3 sessions |
| **Tool pruning** | Leaves out tool definitions unused over *p* prompts across ≥ *m* sessions, or over *s* sessions, via the agent's own settings; one command restores them | *p* = 10, *m* = 2, *s* = 5 |
| **Failure-loop intervention (loop guard)** | A judge model checks a stuck turn; if ≥ *c* sure, injects one redirect; if still stuck, tells the user how to stop — never stops a turn itself | *c* = 0.7, *r* = 1 redirect per turn |

<figure class="at-doc-figure">
<div class="at-diag-stack">
<div class="at-diag-row"><span>Repo facts</span><div class="at-diag-bar"><span class="at-diag-seg seg-prefix" style="width: 100%">Micro-skills</span></div><span></span></div>
<div class="at-diag-row"><span>Tool selection</span><div class="at-diag-bar"><span class="at-diag-seg seg-tool" style="width: 100%">Pruning</span></div><span></span></div>
<div class="at-diag-row"><span>Failure handling</span><div class="at-diag-bar"><span class="at-diag-seg seg-hit" style="width: 100%">Loop guard</span></div><span></span></div>
</div>
<figcaption>Three black-box mechanisms of <a href="https://github.com/microsoft/TokenEconomizer">TokenEconomizer</a>. The authors report 18.4–30.9% lower mean cost across five SWE-Bench agent–model pairs. This is not independently reproduced here; see the <a href="https://arxiv.org/abs/2610.22732">paper</a> and quality results below.</figcaption>
</figure>

<figure class="at-doc-figure">
  <img src="https://raw.githubusercontent.com/microsoft/TokenEconomizer/main/docs/assets/teaser.gif" alt="TokenEconomizer demonstration of its cost-control workflow" loading="lazy" />
  <img src="https://raw.githubusercontent.com/microsoft/TokenEconomizer/main/docs/assets/framework_overview.png" alt="Overview of the TokenEconomizer cost-control framework" loading="lazy" />
  <figcaption>Official project visuals. Source: <a href="https://github.com/microsoft/TokenEconomizer">microsoft/TokenEconomizer (Apache 2.0)</a>; <a href="https://arxiv.org/abs/2610.22732">Experience-Driven Black-Box Cost Control for Coding Agents (arXiv:2610.22732)</a>.</figcaption>
</figure>

## Reported results

On 277 SWE-Bench tasks across six repositories and five agent–model combinations, the authors report
**18.4–30.9% lower mean cost**, cheaper in all 30 repository × combination cells, with Pass@1 higher in 4
of 5 combinations. Costs include every model call TokenEconomizer itself makes. See the
[paper](https://arxiv.org/abs/2610.22732) and [project page](https://tokeneconomizer.github.io) for details.

## Mapping to this guide

| TokenEconomizer | Waste category ([stage 2](../02-isolate/)) | Fix ([stage 4](../04-improve/)) |
|-----------------|--------------------------------------------|---------------------------------|
| Micro-skills | Re-learning repo facts | Short confirmed facts at session start |
| Tool pruning | Unused tools | Prune tool definitions |
| Loop guard | Failure loops | One redirect, then surface to the user |

Design lessons worth copying: act through the agent's **own settings**; require **confirmation** before
stating a learned fact; **cap** interventions; **include the optimizer's own cost** in the accounting;
**fail open** (if the optimizer fails, the session continues as if it were not installed).

## Recommended adoption order for your own agent

1. **Failure-loop guard:** detect repeated errors within the current turn, redirect once, and surface persistent failure. This does not require cross-session training state and can limit costly tail runs.
2. **Unused-tool pruning:** start with a manually reviewed, task-scoped tool set; it also needs no cross-session training state and can cut repeated schema overhead. Keep an escape hatch for rare tools. TokenEconomizer's *automatic* pruning uses usage across sessions, as shown above.
3. **Repo micro-skills:** once session logs accumulate, confirm recurring facts before injecting short, reversible guidance at session start.

This is a practical starting order, not a guarantee that these are the largest savings for every agent. Measure each change separately against cost and task success; prioritize the largest observed waste instead when your baseline points elsewhere.

## Further reading (papers)

Ordered by venue tier, then citations. Findings are hypotheses to verify on your own workload; see the full list in [Papers](../../papers/).

- [SWE-agent: Agent-Computer Interfaces Enable Automated Software Engineering](https://arxiv.org/abs/2405.15793) — NeurIPS 2024, 2,357 citations. Shows that interface design, including concise and bounded observations, changes agent success.
- [Agent Workflow Memory](https://arxiv.org/abs/2409.07429) — ICML 2024, 451 citations. Agents induce reusable workflows from past trajectories and reuse them, similar in spirit to micro-skills.
- [RAG-MCP: Mitigating Prompt Bloat in LLM Tool Selection via Retrieval-Augmented Generation](https://arxiv.org/abs/2505.03275) — preprint 2025, 41 citations. Retrieves only the relevant MCP tool descriptions instead of loading every tool (prompt bloat).
- [Evaluating AGENTS.md: Are Repository-Level Context Files Helpful for Coding Agents?](https://arxiv.org/abs/2602.11988) — preprint 2026, 36 citations. Repository context files did not generally improve task success and raised inference cost by over 20% on average.
- [TokenEconomizer: Experience-Driven Black-Box Cost Control for Coding Agents](https://arxiv.org/abs/2610.22732) — preprint 2026, 0 citations. Micro-skills, tool pruning, and a loop guard cut coding-agent cost using experience from earlier sessions.
