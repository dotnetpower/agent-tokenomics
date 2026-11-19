---
created: 2026-11-03
updated: 2026-11-19
dateSource: git
title: Models, planning, delegation
description: "Optimize LLM token economics in VS Code Copilot: choose models by cost per completed task, plan before coding, and isolate exploration in subagents."
track: copilot
verified: 2026-11-19
sidebar:
  order: 5
---

> **Scope: public documentation reviewed on 2026-11-19.** Prices, policies, UI, and settings can change. Check the original source for your client/version/harness. This date does not claim a new installation, billing, or model-quality experiment.

## Problem

Optimize **cost per successful result**, not price per token. Planning and delegation can help difficult work, but both add model usage.

## Actions

| Choice | Use when | Verify |
|---|---|---|
| Auto | Low-maintenance routing is appropriate | Record the selected model and actual credits; do not assume globally minimal cost. |
| Explicit model | Reproducible workflow or a known capability requirement | Pin an available model/version and comparable effort. |
| Plan first | Requirements or approach are uncertain | A plan can research and call tools; it is not necessarily one model call. |
| Subagents | Independent, substantial tasks benefit from isolation | Count parent, child, review, and retry work once. |
| Direct execution | Small, well-specified change | Do not add a planning/delegation round merely to follow a recipe. |

Use [available model and effort controls](https://code.visualstudio.com/docs/agent-customization/language-models) and [the current planning workflow](https://code.visualstudio.com/docs/agents/run/planning). [Subagent model selection is harness-specific](https://code.visualstudio.com/docs/agents/run/subagents); a cheaper worker is not guaranteed to be available or suitable.

### Failure-retry paradox

A cheaper model can cost more after repeated attempts. Compare actual categorized charges and acceptance results; an average rate-times-token estimate does not capture different cached-input and output prices.

## How to verify

Run representative tasks repeatedly under three configurations: Auto, one explicit model, and a reviewed plan plus bounded workers where useful. Hold task/revision and quality criteria fixed. Record actual models, effort, all credits, elapsed time, retries, and tests passed in [the baseline template](../02-measure-usage/). Do not silently discard failed runs.

## Trade-offs

Higher reasoning effort can help difficult tasks while consuming more tokens. Defaults are a starting point, not a universal optimum. Record cache state when changing settings. Delegation may reduce latency without reducing total cost; report both.

## Further reading (papers)

Ordered by venue tier, then citations. Findings are hypotheses to verify on your own workload; see the full list in [Papers](../../papers/).

- [FrugalGPT: How to Use Large Language Models While Reducing Cost and Improving Performance](https://arxiv.org/abs/2305.05176) - TMLR 2023, 1,247 citations. Selects and cascades LLMs to approach a strong model's quality at lower cost.
- [Hybrid LLM: Cost-Efficient and Quality-Aware Query Routing](https://arxiv.org/abs/2404.14618) - ICLR 2024, 519 citations. A router predicts query difficulty and sends easy queries to a smaller model.
- [AutoMix: Automatically Mixing Language Models](https://arxiv.org/abs/2310.12963) - NeurIPS 2023, 169 citations. Has a small model verify its own answer and escalates to a larger model only when needed.
- [Cost-of-Pass: An Economic Framework for Evaluating Language Models](https://arxiv.org/abs/2504.13359) - ICLR 2025, 56 citations. Defines the expected cost to obtain a correct answer and compares models on it.
- [RouteLLM: Learning to Route LLMs with Preference Data](https://arxiv.org/abs/2406.18665) - preprint 2024, 922 citations. Learns from preference data to route queries between a strong and a cheaper model.

## Sources

- [code.visualstudio.com: harness](https://code.visualstudio.com/blogs/2026/11/18/copilot-harness)
- [code.visualstudio.com: models](https://code.visualstudio.com/docs/agent-customization/language-models)
- [code.visualstudio.com: planning](https://code.visualstudio.com/docs/agents/run/planning)
- [code.visualstudio.com: subagents](https://code.visualstudio.com/docs/agents/run/subagents)
- [code.visualstudio.com: optimize](https://code.visualstudio.com/docs/agents/guides/optimize-usage)
- [docs.github.com: rates](https://docs.github.com/en/copilot/reference/copilot-billing/models-and-pricing)
