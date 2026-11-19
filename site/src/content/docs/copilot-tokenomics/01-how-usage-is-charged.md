---
created: 2026-11-03
updated: 2026-11-19
dateSource: git
title: How Copilot usage is charged
description: "Copilot tokenomics & token economics billing guide: understand GitHub AI Credits (input, cached, output token rates) vs. legacy Premium Request multipliers."
track: copilot
verified: 2026-11-19
sidebar:
  order: 1
---

> **Scope: public documentation reviewed on 2026-11-19.** Prices, policies, UI, and settings can change. Check the original source for your client/version/harness. This date does not claim a new installation, billing, or model-quality experiment.

## Problem

Total tokens, GitHub AI Credits, and legacy premium requests are different meters. Decide which applies before choosing a saving tactic.

## Actions

| Billing path | What to measure | Important boundary |
|---|---|---|
| Usage-based individual plan | Model-priced input, cached input, cache writes where applicable, and output; account credits | Base credits and variable flex allowance are not a universal token quota. |
| Business / Enterprise | Consumption, shared allowance, user budget, paid-usage policy | User budgets hard-stop; spending limits need the stop-usage option. |
| Legacy annual Pro/Pro+ | User prompts and applicable model/feature multipliers | Autonomous tool calls do not each become a premium request. |
| SDK BYOK | Provider usage and billing | Separate from GitHub-billed model usage. |

[GitHub defines one AI credit as $0.01 USD](https://docs.github.com/en/copilot/concepts/billing-and-usage/individuals/billing). Use [GitHub's model/context-tier rate card](https://docs.github.com/en/copilot/reference/copilot-billing/models-and-pricing), not an assumed provider discount. Check [legacy eligibility and feature rules](https://docs.github.com/en/copilot/reference/copilot-billing/request-based-billing-legacy/copilot-requests) and [SDK BYOK](https://docs.github.com/en/copilot/how-tos/copilot-sdk/auth/byok) separately.

### Cached input and billing

Logical conversation input can recur without identical network retransmission. An unchanged eligible prefix may be reused at its cached-input rate. Do not add cached input to an inclusive input total or add reasoning to inclusive billed output.

Cache behavior depends on model, harness, request layout, and retention. A model or effort change can affect reuse, but **not every effort change guarantees a full cache miss**. Inspect [reported reuse and request divergence](https://code.visualstudio.com/docs/agents/agent-troubleshooting/cache-explorer); a diff is not proof of provider billing. Earlier engineering rollouts are not permanent defaults.

## How to verify

Record plan, client/version, harness, authentication, model, task success, and account readings. Compare request/session credits with the account meter; label missing categories **not exposed**, not zero. See [the full product comparison](../../agent-tokenomics/coding-agent-quotas-and-caching/).

## Trade-offs

Less input is not a win if it causes retries. Compare cost per successful task, including failures and delegated work.

## Sources

- [code.visualstudio.com: harness](https://code.visualstudio.com/blogs/2026/11/18/copilot-harness)
- [docs.github.com: billing](https://docs.github.com/en/copilot/concepts/billing-and-usage/individuals/billing)
- [docs.github.com: org](https://docs.github.com/en/copilot/concepts/billing-and-usage/organizations-and-enterprises/billing)
- [docs.github.com: rates](https://docs.github.com/en/copilot/reference/copilot-billing/models-and-pricing)
- [docs.github.com: legacy](https://docs.github.com/en/copilot/reference/copilot-billing/request-based-billing-legacy/copilot-requests)
- [code.visualstudio.com: cache](https://code.visualstudio.com/docs/agents/agent-troubleshooting/cache-explorer)
