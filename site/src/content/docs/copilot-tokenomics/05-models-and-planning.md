---
title: Models, planning, delegation
description: Pick models on evidence, plan before coding, and delegate bounded work.
track: copilot
verified: 2026-11-03
sidebar:
  order: 5
---

## Problem

The cheapest model per token is not always the cheapest per *result*, and jumping straight to code with
the wrong approach wastes whole sessions.

## Actions

1. **Choose models on evidence.** Define the expected result and how you will judge it; run each candidate
   more than once (output is nondeterministic); compare completion, quality, reliability, duration, and
   credits; pick the cheapest model that consistently meets the bar. Re-evaluate as models change.
2. **Low-maintenance default:** use **auto model selection**. For repeatable workflows, pin a model in a
   **custom agent**.
3. **Leave thinking effort at defaults.** Higher effort produces more thinking tokens (cost and latency).
   Raise it only for genuinely complex work such as architectural planning or multi-step debugging.
4. **Plan, then implement.** Use the Plan agent with a capable model, review the plan, split it into
   well-scoped tasks, and delegate those to **custom agents as subagents** with cheaper models, focused
   instructions, and minimal tools. The primary agent reviews results.

## How to verify

For a recurring task type, compare (a) one strong model end-to-end against (b) plan with a strong model +
workers on a cheaper one. Record credits, retries, and whether the result passes your acceptance check.

## Trade-offs

- Delegation itself uses tokens. Keep small tasks in the main session if isolation and a cheaper model do
  not offset the orchestration overhead.
- A cheaper model that needs retries or creates rework can cost more overall.

## Further reading (papers)

Ordered by venue tier, then citations. Findings are hypotheses to verify on your own workload; see the full list in [Papers](../../papers/).

- [FrugalGPT: How to Use Large Language Models While Reducing Cost and Improving Performance](https://arxiv.org/abs/2305.05176) — TMLR 2023, 1,247 citations. Selects and cascades LLMs to approach a strong model's quality at lower cost.
- [Hybrid LLM: Cost-Efficient and Quality-Aware Query Routing](https://arxiv.org/abs/2404.14618) — ICLR 2024, 519 citations. A router predicts query difficulty and sends easy queries to a smaller model.
- [AutoMix: Automatically Mixing Language Models](https://arxiv.org/abs/2310.12963) — NeurIPS 2023, 169 citations. Has a small model verify its own answer and escalates to a larger model only when needed.
- [Cost-of-Pass: An Economic Framework for Evaluating Language Models](https://arxiv.org/abs/2504.13359) — ICLR 2025, 56 citations. Defines the expected cost to obtain a correct answer and compares models on it.
- [RouteLLM: Learning to Route LLMs with Preference Data](https://arxiv.org/abs/2406.18665) — preprint 2024, 922 citations. Learns from preference data to route queries between a strong and a cheaper model.

## Sources

- [Optimize AI credit usage — Choose models, Plan and delegate, Thinking effort](https://code.visualstudio.com/docs/agents/guides/optimize-usage)
- [AI model comparison (GitHub Docs)](https://docs.github.com/en/copilot/reference/ai-models/model-comparison)
