---
created: 2026-11-03
updated: 2026-11-19
dateSource: git
title: Agent modes and budget guards
description: Ask vs Plan vs Agent, agent loop limits, reasoning effort, and cloud-agent setup.
track: copilot
verified: 2026-11-19
sidebar:
  order: 8
---

> **Scope: public documentation reviewed on 2026-11-19.** Prices, policies, UI, and settings can change. Check the original source for your client/version/harness. This date does not claim a new installation, billing, or model-quality experiment.

## Problem

A mode name is not a fixed model-call count or dollar budget. Read-only research and planning can still search, call tools, and make multiple model requests.

## Actions

### Pick the cheapest mode that fits

| Workflow | Intent | Do not assume |
|---|---|---|
| Ask / read-only research | Understand without intended code edits | Exactly one model call, no retrieval, or no cost |
| Plan | Research and review an approach before implementation | A single planning pass or free work |
| Agent implementation | Execute a bounded change and verify it | Permission to expand scope until a limit is hit |

Use [the planning workflow](https://code.visualstudio.com/docs/agents/run/planning) when the approach is uncertain. Small, well-defined edits may go directly to implementation.

### Bound the loop

Set acceptance checks and an explicit retry guard:

```text
Run focused tests. If the same command fails twice with the same error,
stop and report the blocker, attempts, and unresolved checks.
Do not silently skip failed verification.
```

This is a **prompt instruction**, not an enforced spending cap. Treat `chat.agent.maxRequests` as a setting whose availability, meaning, and default must be checked for the installed version and harness in [the current reference](https://code.visualstudio.com/docs/copilot/reference/copilot-settings). No universal default is asserted here. Copilot-harness/Agent Host behavior is not automatically Local behavior.

### Where limits apply

| Control | What it bounds | Limitation |
|---|---|---|
| Retry instruction | Intended agent behavior | Requires log verification |
| Supported request limit | Request count under its documented scope | Not a dollar/token limit; child/other-session scope must be checked |
| GitHub user budget / spending limit | Account or organization consumption/spend | User budgets hard-stop; spending limits require enforcement enabled |

Check [organization budget semantics](https://docs.github.com/en/copilot/concepts/billing-and-usage/organizations-and-enterprises/billing) or [individual additional-usage rules](https://docs.github.com/en/copilot/concepts/billing-and-usage/individuals/billing). A blocked credit-consuming feature does not imply all completions stop.

### Copilot cloud agent (issue → PR)

Provide files, repro steps, scope, and acceptance checks. Use [Copilot setup steps](https://docs.github.com/en/copilot/how-tos/use-copilot-agents/cloud-agent/customize-the-agent-environment) to prepare supported dependencies. Include setup/review and any separately billed infrastructure costs; do not treat all automation as free.

## How to verify

Use one representative bounded change. Record model requests, retries, child work, credits, tests, and why execution stopped. Distinguish completion, prompt compliance, request-limit stop, budget stop, and errors.

## Trade-offs

An overly tight limit can strand useful work. Planning and strong models may cost more initially but avoid retries; compare successful outcomes rather than the mode label.

## Further reading (papers)

Ordered by venue tier, then citations. Findings are hypotheses to verify on your own workload; see the full list in [Papers](../../papers/).

- [Training Language Models to Reason Efficiently](https://arxiv.org/abs/2502.04463) - NeurIPS 2025, 285 citations. Trains reasoning models to trade accuracy against reasoning length.
- [Token-Budget-Aware LLM Reasoning](https://arxiv.org/abs/2412.18547) - ACL 2024, 259 citations. Estimates a token budget per problem to shorten chain-of-thought reasoning.
- [Do NOT Think That Much for 2+3=? On the Overthinking of o1-Like LLMs](https://arxiv.org/abs/2412.21187) - preprint 2024, 605 citations. Shows o1-like models spend excessive tokens on simple questions and proposes ways to cut them.
- [Agentless: Demystifying LLM-based Software Engineering Agents](https://arxiv.org/abs/2407.01489) - preprint 2024, 566 citations. A fixed localize-repair-validate pipeline competes with open-ended agent loops on SWE-bench Lite at low cost.
- [L1: Controlling How Long A Reasoning Model Thinks With Reinforcement Learning](https://arxiv.org/abs/2503.04697) - preprint 2025, 416 citations. Trains reasoning models to obey a user-specified length budget.
- [Efficient Agents: Building Effective Agents While Reducing Cost](https://arxiv.org/abs/2508.02694) - preprint 2025, 40 citations. Systematic study of the efficiency-effectiveness trade-off in agent system design.

## Sources

- [code.visualstudio.com: harness](https://code.visualstudio.com/blogs/2026/11/18/copilot-harness)
- [code.visualstudio.com: planning](https://code.visualstudio.com/docs/agents/run/planning)
- [code.visualstudio.com: settings](https://code.visualstudio.com/docs/copilot/reference/copilot-settings)
- [docs.github.com: org](https://docs.github.com/en/copilot/concepts/billing-and-usage/organizations-and-enterprises/billing)
- [docs.github.com: billing](https://docs.github.com/en/copilot/concepts/billing-and-usage/individuals/billing)
- [docs.github.com: setup](https://docs.github.com/en/copilot/how-tos/use-copilot-agents/cloud-agent/customize-the-agent-environment)
