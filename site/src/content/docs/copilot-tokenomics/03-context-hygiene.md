---
created: 2026-11-03
updated: 2026-11-19
dateSource: git
title: Keep context lean
description: "Reduce GitHub Copilot token usage with context hygiene: new chats per task, /fork, /compact, focused #file attachments, workspace exclusions, and cctx."
track: copilot
verified: 2026-11-19
sidebar:
  order: 3
---

> **Scope: public documentation reviewed on 2026-11-19.** Prices, policies, UI, and settings can change. Check the original source for your client/version/harness. This date does not claim a new installation, billing, or model-quality experiment.

## Problem

Irrelevant history enlarges later logical inputs. Too little context can also cause failed edits and retries. Optimize for sufficient, authoritative context, not the fewest bytes.

## Actions

| Situation | Action | Check |
|---|---|---|
| Unrelated task | New chat with a short handoff | No old topic; required decisions remain available. |
| Alternative approach | Fork where supported | History is copied; cache reuse is **not guaranteed**. |
| Related long task | /compact where supported | Confirm decisions, constraints, files, and open work survived. |
| Known relevant source | Attach focused files/symbols | Inspect actual context instead of assuming an entire folder was included. |
| Noisy search | Scope paths and exclusions | Exclusion from search is not an access-control boundary. |
| Large new tool result | Filter before insertion | Retain exit status, warning/test counts, and relevant failures. |

Use [client-specific context controls](https://docs.github.com/en/copilot/concepts/prompting-and-context/context-management) and [VS Code grounding guidance](https://code.visualstudio.com/docs/agents/guides/optimize-usage). [cctx](../cctx/) is optional; built-in scoped search remains a valid baseline.

### Subagent delegation break-even rule

Delegate substantial independent work when isolating its intermediate output and any model-cost advantage outweigh startup and coordination. Do not turn a heuristic tool-call count into a universal break-even threshold. [Local subagents do not inherit main conversation history](https://code.visualstudio.com/docs/agents/run/subagents); Copilot-harness delegation uses different runtime tools. Count parent and child work once.

## How to verify

Repeat one task with fixed model/effort and the same acceptance tests. Record context size, actual credits, retries, elapsed time, and success using [the baseline template](../02-measure-usage/).

## Trade-offs

Compaction is lossy and can change reusable prefixes; it is not always cheaper immediately. Do not silently shorten **past** tool messages: [prefix changes can affect cache and reasoning continuity](https://developers.openai.com/api/docs/guides/keep-prefix-unchanged). Filter new results first. Forking keeps history, whereas a fresh session starts without it. Preserve needed context even if cache reuse falls.

## Further reading (papers)

Ordered by venue tier, then citations. Findings are hypotheses to verify on your own workload; see the full list in [Papers](../../papers/).

- [Lost in the Middle: How Language Models Use Long Contexts](https://arxiv.org/abs/2307.03172) - TACL 2023, 5,818 citations. Accuracy drops when the relevant information sits in the middle of a long input.
- [Large Language Models Can Be Easily Distracted by Irrelevant Context](https://arxiv.org/abs/2302.00093) - ICML 2023, 1,358 citations. Irrelevant context in the prompt can substantially reduce model accuracy.
- [LLMs Get Lost In Multi-Turn Conversation](https://arxiv.org/abs/2505.06120) - ICLR 2025, 562 citations. Models perform worse in multi-turn, underspecified conversations than in single-turn ones.
- [Same Task, More Tokens: the Impact of Input Length on the Reasoning Performance of Large Language Models](https://arxiv.org/abs/2402.14848) - ACL 2024, 270 citations. Isolates input length as a factor and shows reasoning degrades as inputs grow.
- [The Complexity Trap: Simple Observation Masking Is as Efficient as LLM Summarization for Agent Context Management](https://arxiv.org/abs/2508.21433) - preprint 2025, 66 citations. Simple observation masking is as efficient as LLM summarization for agent context management.

## Sources

- [code.visualstudio.com: harness](https://code.visualstudio.com/blogs/2026/11/18/copilot-harness)
- [code.visualstudio.com: optimize](https://code.visualstudio.com/docs/agents/guides/optimize-usage)
- [docs.github.com: context](https://docs.github.com/en/copilot/concepts/prompting-and-context/context-management)
- [code.visualstudio.com: subagents](https://code.visualstudio.com/docs/agents/run/subagents)
- [developers.openai.com: prefix](https://developers.openai.com/api/docs/guides/keep-prefix-unchanged)
- [code.visualstudio.com: cache](https://code.visualstudio.com/docs/agents/agent-troubleshooting/cache-explorer)
