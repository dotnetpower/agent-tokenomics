---
created: 2026-11-06
updated: 2026-11-19
dateSource: git
title: "cctx: VS Code Copilot Token Saver & Accelerator"
description: "Install Code Context (cctx) from the VS Code Marketplace to cut Copilot lookup tokens, speed up coding tasks, and inspect live AIC/USD cost savings."
track: copilot
verified: 2026-11-19
sidebar:
  order: 11
---

> **Scope: public documentation reviewed on 2026-11-19.** Prices, policies, UI, and settings can change. Check the original source for your client/version/harness. This date does not claim a new installation, billing, or model-quality experiment.

## Problem

[Code Context (cctx)](https://github.com/dotnetpower/code-context) provides local indexed search and bounded source context through native VS Code tools and MCP. It is an optional retrieval implementation, not a billing service or a guarantee of savings.

## Action: choose the supported integration

Install the extension through [the Marketplace](https://marketplace.visualstudio.com/items?itemName=dotnetpower.code-context-native) and follow the project's current backend/setup instructions. Record the extension/backend revision, operating environment, and VS Code harness. Installation and benchmark compatibility were not independently rerun for this editorial revision.

| Session | What to verify |
|---|---|
| Foreground VS Code with native tools | Confirm the native tools are available and calls actually execute. |
| Copilot SDK / Agent Host / background | Verify the backend MCP or other supported runtime integration; do not assume editor-native dispatch is callable. |
| Cold or unavailable index | A cache miss is not an empty search result. Use an allowed fallback without bypassing exclusions. |

Keep source access policy, secret exclusions, and workspace trust boundaries intact. Indexed retrieval does not authorize access the client otherwise denies.

## How to verify

Use scoped built-in search/read as the baseline, not a deliberately oversized whole-file dump. Hold task, revision, model/effort, and acceptance checks fixed. Compare cold and warm lookup latency separately from full-session credits, retries, and success.

| Signal | Meaning | Not equivalent to |
|---|---|---|
| Local warm-index hit | Retrieval reused the local index | Provider prompt-cache hit |
| Delivered payload reduction | Fewer source-result tokens under the stated counter | Actual billed credits saved |
| Counterfactual saving estimate | Model/rate/baseline-dependent estimate | An observed invoice or guaranteed saving |
| Account credit difference | Difference between measured runs | Causal proof without controlled conditions |

[GitHub AI Credits use $0.01 USD per credit](https://docs.github.com/en/copilot/concepts/billing-and-usage/individuals/billing). Do not reuse older dashboard examples labeled `$0.001/AIC` as GitHub credit conversions. If a tool reports a different internal unit, preserve its name and documented conversion rather than calling it AIC. Likewise, do not add reasoning to already inclusive output totals.

The [project benchmark report](https://github.com/dotnetpower/code-context/blob/main/docs/BENCHMARKS.md) contains author-reported results, **not independently reproduced here**. Pin its revision and task/model/sample conditions before quoting a number. This page does not claim a new acceleration, accuracy, or monetary-saving percentage.

## Trade-offs

Daemon startup, indexing, installed tool definitions, and routing instructions also have costs. Small known-range reads may not benefit. Keep cctx only if measured task success, latency, and cost justify it; see [measurement](../02-measure-usage/) and [context management](../03-context-hygiene/).

## Sources

- [code.visualstudio.com: harness](https://code.visualstudio.com/blogs/2026/11/18/copilot-harness)
- [marketplace.visualstudio.com: marketplace](https://marketplace.visualstudio.com/items?itemName=dotnetpower.code-context-native)
- [github.com: cctx](https://github.com/dotnetpower/code-context)
- [github.com: benchmarks](https://github.com/dotnetpower/code-context/blob/main/docs/BENCHMARKS.md)
- [docs.github.com: billing](https://docs.github.com/en/copilot/concepts/billing-and-usage/individuals/billing)
- [code.visualstudio.com: debug](https://code.visualstudio.com/docs/agents/agent-troubleshooting/chat-debug-view)
