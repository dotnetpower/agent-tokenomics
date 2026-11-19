---
created: 2026-11-03
updated: 2026-11-19
dateSource: git
title: Measure your usage
description: "Measure GitHub Copilot tokenomics and AI credit spend in VS Code using Agent Debug Logs, Cache Explorer, context window controls, and cctx usage telemetry."
track: copilot
verified: 2026-11-19
sidebar:
  order: 2
---

> **Scope: public documentation reviewed on 2026-11-19.** Prices, policies, UI, and settings can change. Check the original source for your client/version/harness. This date does not claim a new installation, billing, or model-quality experiment.

## Problem

Separate **context occupancy**, **cumulative usage**, and **account allowance**. A percentage in one view is not interchangeable with another.

## Actions

| Surface | Inspection | Scope and limitation |
|---|---|---|
| VS Code | Response hover; context control; Copilot status dashboard | Request/session/monthly credit readings depend on the build and billing mode. |
| VS Code diagnostics | Developer: Open Agent Debug Logs → Summary → Cache Explorer | Enable capture **before** reproduction. The panel is Preview; logging settings differ for Agent Host and Local. |
| CLI | /context; /usage | Context composition versus session/per-model usage; AI Credits appear on token-billed accounts. |
| SDK | assistant.usage; session.usage_info | Per-call usage versus current context utilization. Events include subagent calls; do not count them twice. |

Follow [the official capture setup](https://code.visualstudio.com/docs/agents/agent-troubleshooting/chat-debug-view) for your harness. [Cache Explorer](https://code.visualstudio.com/docs/agents/agent-troubleshooting/cache-explorer) explains request divergence, not authoritative billing. [SDK usage events](https://docs.github.com/en/copilot/how-tos/copilot-sdk/features/usage-and-billing) are live and not replayed on resume; accumulated metrics RPCs are experimental. Treat [cctx](../cctx/) estimates as optional retrieval telemetry, not a second billing ledger.

## How to verify

Copy this unmeasured baseline template and repeat the same task while changing one variable:

| Field | Baseline | Candidate |
|---|---|---|
| Task / acceptance checks / revision | Not recorded | Not recorded |
| Client / version / harness / model / effort | Not recorded | Not recorded |
| Authentication / billing mode | Not recorded | Not recorded |
| Input / cached input / writes / inclusive output | Not measured | Not measured |
| Actual credits or provider cost / elapsed time | Not measured | Not measured |
| Retries / child calls / checks passed | Not measured | Not measured |
| Reported cache state and metric denominator | Not recorded | Not recorded |

State whether each reading is per call, turn, session, or account. Repeat representative tasks; fresh chats do not guarantee cold caches. Keep a change only if quality holds.

## Trade-offs and cautions

Missing fields are **not exposed**, not zero. Account readings can include other work or arrive later. Review exported logs for prompts, source, paths, and credentials before sharing. Cost tips can suggest hypotheses but do not replace measurements.

## Further reading (papers)

Ordered by venue tier, then citations. Findings are hypotheses to verify on your own workload; see the full list in [Papers](../../papers/).

- [SGLang: Efficient Execution of Structured Language Model Programs](https://arxiv.org/abs/2312.07104) - NeurIPS 2023, 1,783 citations. RadixAttention reuses KV cache across calls that share a prefix; foundation for prefix caching.
- [Prompt Cache: Modular Attention Reuse for Low-Latency Inference](https://arxiv.org/abs/2311.04934) - MLSys 2023, 417 citations. Reuses precomputed attention states of recurring prompt modules to cut latency.
- [Don't Break the Cache: An Evaluation of Prompt Caching for Long-Horizon Agentic Tasks](https://arxiv.org/abs/2601.06007) - preprint 2026, 25 citations. Evaluates prompt caching on long-horizon agentic tasks.

## Sources

- [code.visualstudio.com: harness](https://code.visualstudio.com/blogs/2026/11/18/copilot-harness)
- [code.visualstudio.com: debug](https://code.visualstudio.com/docs/agents/agent-troubleshooting/chat-debug-view)
- [code.visualstudio.com: cache](https://code.visualstudio.com/docs/agents/agent-troubleshooting/cache-explorer)
- [docs.github.com: context](https://docs.github.com/en/copilot/concepts/prompting-and-context/context-management)
- [docs.github.com: cli](https://docs.github.com/en/copilot/reference/copilot-cli-reference/cli-command-reference)
- [docs.github.com: sdk](https://docs.github.com/en/copilot/how-tos/copilot-sdk/features/usage-and-billing)
