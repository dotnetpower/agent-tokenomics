---
created: 2026-11-03
updated: 2026-11-19
dateSource: git
title: "Copilot Tokenomics (Token Economics)"
description: "Practical tokenomics and token economics for GitHub Copilot in VS Code: save tokens and AI credits, trim MCP overhead, and speed up coding with cctx."
track: copilot
verified: 2026-11-19
---

> **Scope: public documentation reviewed on 2026-11-19.** Prices, policies, UI, and settings can change. Check the original source for your client/version/harness. This date does not claim a new installation, billing, or model-quality experiment.

## Problem

Track A helps developers reduce GitHub Copilot cost **without reducing task success**. Start with the actual billing mode and client/harness, not a promised saving percentage.

## Actions

The [2026-11-18 announcement](https://code.visualstudio.com/blogs/2026/11/18/copilot-harness) makes the Copilot harness the default for new VS Code agent sessions while Local is in transition. A Local-only setting or editor-native tool is not automatically supported in every SDK/Agent Host session.

| Step | Guide |
|---|---|
| Establish the meter and baseline | [Billing](01-how-usage-is-charged/) · [Measure usage](02-measure-usage/) |
| Reduce repeated context | [Context hygiene](03-context-hygiene/) · [Instructions and tools](04-instructions-and-tools/) |
| Match capability to the task | [Models, planning, delegation](05-models-and-planning/) |
| Keep concise but complete requests | [Prompt/output style](06-prompt-and-output-style/) · [Language/tokenization](07-language-and-tokenization/) |
| Control execution and catalog size | [Modes/budgets](08-agent-modes-and-budget/) · [MCP audit](09-mcp-audit/) |
| Apply and re-check | [Checklist](10-checklist/) · [Optional cctx retrieval](cctx/) |
| Compare other products | [Copilot, Codex, Claude Code anatomy](coding-agent-quotas-and-caching/) |

## How to verify

Use the same representative task/revision, record model/effort, client/harness, authentication, and cache state, then change one variable. Compare actual credits, elapsed time, retries, and acceptance tests. Missing metrics are not zero.

## Trade-offs

Each guide distinguishes product behavior from provider API behavior and local text measurements. Linked research/author benchmarks are hypotheses, not a guarantee of savings in your workflow. Existing paper lists retain their original metadata retrieval date; a document modification is not a new experiment.

## Document dates

Every site document displays creation, modification, and any verification date separately. Historical creation uses the first Git record, not a claim about the author's first draft. Pages without reliable creation evidence show a recorded-on date. Modification changes with content, not rebuilding; verification changes only after checking the relevant sources.

## Sources

- [code.visualstudio.com: harness](https://code.visualstudio.com/blogs/2026/11/18/copilot-harness)
- [code.visualstudio.com: optimize](https://code.visualstudio.com/docs/agents/guides/optimize-usage)
- [docs.github.com: billing](https://docs.github.com/en/copilot/concepts/billing-and-usage/individuals/billing)
