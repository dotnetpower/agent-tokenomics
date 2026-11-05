---
title: Agent modes and budget guards
description: Ask vs Plan vs Agent, agent loop limits, reasoning effort, and cloud-agent setup.
track: copilot
verified: 2026-11-03
sidebar:
  order: 8
---

## Problem

An agent run is a **loop**: each step is a model call, and each call re-sends the instructions, tool definitions,
and conversation so far. A vague prompt can trigger a long exploration that ends in a misunderstanding and a restart.

<figure class="at-doc-figure">
  <div class="at-doc-figure-grid">
    <img src="https://code.visualstudio.com/assets/docs/agents/chat-debug-view/agent-flow-chart-v2.png" alt="Agent Debug View flow chart showing model requests and tool calls" loading="lazy" />
    <img src="https://code.visualstudio.com/assets/docs/agent-customization/customization/create-custom-agent-2.png" alt="VS Code custom agent creation interface" loading="lazy" />
  </div>
  <figcaption>Inspect an agent run (left) and create a custom agent (right). Sources: <a href="https://code.visualstudio.com/docs/copilot/chat/chat-debug-view">Agent Debug View</a> and <a href="https://code.visualstudio.com/docs/agent-customization/custom-agents">Custom Agents</a>.</figcaption>
</figure>

## Actions

### Pick the cheapest mode that fits

| Mode | Typical shape | Use for |
|------|---------------|---------|
| Ask | One model call | Questions, explanations, lookups |
| Plan | One planning pass | Scoping a change before any code is written |
| Agent | Many calls (tool loop) | Multi-file refactors, building a feature from scratch |

Default to Ask for questions; use Plan to clarify scope; use Agent when the task genuinely needs tools.
The expensive pattern is running Agent on an ambiguous prompt.

### Bound the loop

- **`chat.agent.maxRequests`** — the maximum number of requests the agent can make in a run (default 25 in the
  VS Code settings reference we checked on 2026-11-03). Lower it if runaway loops worry you; raise it
  deliberately for known long tasks.
- Give a **clear completion criterion** ("done = tests pass and lint is clean") so the agent stops exploring.
- For complex work, have the agent write a **plan file** first and then execute it.
- Name the tool to use when you know it ("use only the GitHub create-PR tool") to reduce tool searching.
- Give **concrete context**: files, error text, constraints (see [Keep context lean](../03-context-hygiene/)).

For Autopilot or a background agent, add an explicit loop guard to the task prompt:

```text
Run focused tests first. If the same command or test fails twice with the same error, stop and report the blocker instead of retrying in a loop.
```

This is an instruction, not a hard enforcement mechanism; check the logs and use request or budget limits as additional safeguards.

### Models and reasoning

- Use **auto model selection** as a default; pin a stronger model for planning and a cheaper one for bounded
  implementation (see [Models, planning, delegation](../05-models-and-planning/)).
- Auto is a convenient default, **not a cost optimizer**: confirm in the docs how it picks models before relying
  on it to control spend.
- Leave **thinking effort** at the default; raise it only for hard problems. Higher effort writes more thinking
  tokens (cost and latency).
- A model switch starts a fresh prompt cache: caching is provider-, model-, and deployment-specific, so
  switching models mid-session does not reuse the earlier cache.

### Copilot cloud agent (issue → PR)

A cloud-agent task can consume a large amount of tokens. Reduce the start-up cost:

- Write a precise issue: acceptance criteria (tests or behavior), reproduction steps, links to relevant files,
  explicit scope — not just "fix this".
- Pre-install dependencies and pre-run the build in the repository's **Copilot setup steps** workflow,
  `.github/workflows/copilot-setup-steps.yml`, so the agent does not rediscover the environment each time.

### Where limits apply

Usage-based billing adds account-level budget controls on GitHub (for example per-user or cost-center
spending limits) in addition to the in-editor loop limit. Check your plan's current options in the
[GitHub Docs](https://docs.github.com/en/copilot/concepts/billing/usage-based-billing-for-individuals).
Note that automation which calls Copilot (CI review, generated tests, scans) consumes credits on every run.

## How to verify

Run one multi-step task in Ask, Plan, and Agent where each is appropriate and compare session credits and
number of tool calls in **Agent Debug Logs**. Confirm that the loop ends on your completion criterion rather
than at the request limit.

## Trade-offs

A low `maxRequests` can cut off legitimate long tasks mid-way (wasting the tokens already spent). Prefer a clear
done-criterion and a plan file over tight limits.

## Further reading (papers)

Ordered by venue tier, then citations. Findings are hypotheses to verify on your own workload; see the full list in [Papers](../../papers/).

- [Training Language Models to Reason Efficiently](https://arxiv.org/abs/2502.04463) — NeurIPS 2025, 285 citations. Trains reasoning models to trade accuracy against reasoning length.
- [Token-Budget-Aware LLM Reasoning](https://arxiv.org/abs/2412.18547) — ACL 2024, 259 citations. Estimates a token budget per problem to shorten chain-of-thought reasoning.
- [Do NOT Think That Much for 2+3=? On the Overthinking of o1-Like LLMs](https://arxiv.org/abs/2412.21187) — preprint 2024, 605 citations. Shows o1-like models spend excessive tokens on simple questions and proposes ways to cut them.
- [Agentless: Demystifying LLM-based Software Engineering Agents](https://arxiv.org/abs/2407.01489) — preprint 2024, 566 citations. A fixed localize-repair-validate pipeline competes with open-ended agent loops on SWE-bench Lite at low cost.
- [L1: Controlling How Long A Reasoning Model Thinks With Reinforcement Learning](https://arxiv.org/abs/2503.04697) — preprint 2025, 416 citations. Trains reasoning models to obey a user-specified length budget.
- [Efficient Agents: Building Effective Agents While Reducing Cost](https://arxiv.org/abs/2508.02694) — preprint 2025, 40 citations. Systematic study of the efficiency-effectiveness trade-off in agent system design.

## Sources

- [VS Code Copilot settings reference](https://code.visualstudio.com/docs/copilot/reference/copilot-settings) (`chat.agent.maxRequests`)
- [Optimize AI credit usage in VS Code](https://code.visualstudio.com/docs/agents/guides/optimize-usage)
- [Customize the cloud agent environment (copilot-setup-steps.yml)](https://docs.github.com/en/copilot/how-tos/use-copilot-agents/cloud-agent/customize-the-agent-environment)
- Token Efficiency and Context Engineering — developer talk deck (June 2026): mode comparison and cloud-agent
  tips (qualitative points only; the deck's token figures are not reproduced here)
