---
created: 2026-11-03
updated: 2026-11-19
dateSource: git
title: Checklist
description: "One-page Copilot tokenomics checklist: high-impact habits before, during, and after VS Code GitHub Copilot sessions to cut token and AI credit spend."
track: copilot
verified: 2026-11-19
sidebar:
  order: 10
---

> **Scope: public documentation reviewed on 2026-11-19.** Prices, policies, UI, and settings can change. Check the original source for your client/version/harness. This date does not claim a new installation, billing, or model-quality experiment.

## Before a session

- [ ] Record client/version, Copilot or Local harness, authentication, and billing mode.
- [ ] Choose task-appropriate model/effort; record Auto's actual choice where exposed.
- [ ] Supply relevant files, error, constraints, and acceptance tests.
- [ ] Confirm loaded instructions and needed tools, not merely installed servers.
- [ ] Keep a clear language and required test/failure reporting; English and delegation are not mandatory.
- [ ] Start unrelated work in a new chat; fork only when existing context is useful.

## During

- [ ] Use focused search and complete relevant definitions; bound new outputs.
- [ ] Keep exit status, warnings, test counts, and unresolved failures.
- [ ] Use planning for uncertainty and subagents for substantial independent work.
- [ ] Count parent and child usage once; record retries.
- [ ] Compact when useful; verify critical decisions survived.
- [ ] Distinguish prompt retry guards, supported request limits, and enforced budgets.

## After and weekly

- [ ] Fill [the baseline record](../02-measure-usage/): actual credits, time, retries, and success.
- [ ] Label unavailable fields as not exposed; do not add cached input or reasoning twice.
- [ ] Separate local retrieval estimates from provider cache and actual billing.
- [ ] Check the account allowance and paid-usage/budget policy.
- [ ] Keep a change only if cost and quality together improve.

## How to verify

Choose one completed task and trace each checked item to logs, actual settings, or test results. A ticked checklist is not proof on its own.

## Trade-offs

This is a workflow, not a savings guarantee. [Model choice](../05-models-and-planning/), [MCP audit](../09-mcp-audit/), and [budget guards](../08-agent-modes-and-budget/) contain the conditions. Do not optimize reported cache reuse by removing necessary context.

## Sources

- [code.visualstudio.com: harness](https://code.visualstudio.com/blogs/2026/11/18/copilot-harness)
- [code.visualstudio.com: optimize](https://code.visualstudio.com/docs/agents/guides/optimize-usage)
- [code.visualstudio.com: cache](https://code.visualstudio.com/docs/agents/agent-troubleshooting/cache-explorer)
- [docs.github.com: billing](https://docs.github.com/en/copilot/concepts/billing-and-usage/individuals/billing)
