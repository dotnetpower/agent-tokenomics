---
created: 2026-11-03
updated: 2026-11-19
dateSource: git
title: Instructions and tools
description: "Cut recurring input token overhead in VS Code GitHub Copilot by trimming always-on custom instructions (copilot-instructions.md, AGENTS.md) and tool schemas."
track: copilot
verified: 2026-11-19
sidebar:
  order: 4
---

> **Scope: public documentation reviewed on 2026-11-19.** Prices, policies, UI, and settings can change. Check the original source for your client/version/harness. This date does not claim a new installation, billing, or model-quality experiment.

## Problem

Loaded instructions and tool schemas can recur across requests. More guidance is not automatically better, but deleting non-obvious project constraints can create expensive rework.

## Actions

1. Keep concise, confirmed build/test commands and repository-specific constraints. Remove duplicate rules and discoverable project summaries.
2. Distinguish always-on guidance, path-scoped rules, and on-demand skills. Applicability and discovery depend on the harness; follow [the instruction reference](https://code.visualstudio.com/docs/agents/reference/custom-instructions-reference).
3. Use a canonical rule per scope. Do not assume every discovered file was included, or every included instruction was followed.
4. Configure task-relevant tools using [the client tool controls](https://code.visualstudio.com/docs/agents/run/tools). An enabled MCP server does not prove that every full schema was sent upfront.
5. For deterministic batches, use a reviewed script and return a bounded summary including failures. Do not replace required permission checks with a broad terminal workaround.

For the **Copilot harness**, use **Chat: Open Customizations → Tools**. Client-tool selections persist for the harness/host and can affect other sessions; they are not request-only overrides. Local-harness controls differ. Review scope before changing shared selections.

| Surface | Inspect | Important limit |
|---|---|---|
| VS Code | Customization diagnostics and Chat Debug request content | Local and Copilot activation/tool behavior can differ. |
| CLI | copilot instruction list; /context; /mcp | Discovery listing is not proof of inclusion in every request. |
| SDK | Configured session instructions/tools and usage events | Confirm behavior against the pinned SDK/runtime schema. |

See [instruction discovery, inclusion, and adherence diagnostics](https://code.visualstudio.com/docs/agents/agent-troubleshooting/chat-debug-view). Use [the MCP audit](../09-mcp-audit/) for catalog measurements.

## How to verify

Remove one duplicate or scope one rule. Repeat a fixed task; compare first-request input, full-session credits, missed constraints, retries, and tests. Keep the change only if it preserves success.

[Formula notation examples](../06-prompt-and-output-style/#4-bpe-aligned-formula-notation) can measure text reduction, but a shorter rule or retained keyword does not establish model comprehension. Earlier recognition-accuracy claims without a reproducible model-evaluation record are not used as evidence here.

## Trade-offs

An overly small tool set may cause more searching or manual work. Defer optional guidance rather than deleting indispensable constraints. Mid-session changes can affect request layout; inspect reported reuse instead of assuming a cache miss.

## Further reading (papers)

Ordered by venue tier, then citations. Findings are hypotheses to verify on your own workload; see the full list in [Papers](../../papers/).

- [Batch Prompting: Efficient Inference with Large Language Model APIs](https://arxiv.org/abs/2301.08721) - EMNLP 2023, 154 citations. Packs several samples into one prompt so instruction tokens are shared.
- [MCP-Zero: Active Tool Discovery for Autonomous LLM Agents](https://arxiv.org/abs/2506.01056) - preprint 2025, 55 citations. Lets the agent request tools on demand instead of injecting all tool schemas.
- [RAG-MCP: Mitigating Prompt Bloat in LLM Tool Selection via Retrieval-Augmented Generation](https://arxiv.org/abs/2505.03275) - preprint 2025, 41 citations. Retrieves only the relevant MCP tool descriptions instead of loading every tool (prompt bloat).
- [Evaluating AGENTS.md: Are Repository-Level Context Files Helpful for Coding Agents?](https://arxiv.org/abs/2602.11988) - preprint 2026, 36 citations. Repository context files did not generally improve task success and raised inference cost by over 20% on average.

## Sources

- [code.visualstudio.com: harness](https://code.visualstudio.com/blogs/2026/11/18/copilot-harness)
- [code.visualstudio.com: instructions](https://code.visualstudio.com/docs/agents/reference/custom-instructions-reference)
- [code.visualstudio.com: debug](https://code.visualstudio.com/docs/agents/agent-troubleshooting/chat-debug-view)
- [code.visualstudio.com: tools](https://code.visualstudio.com/docs/agents/run/tools)
- [docs.github.com: context](https://docs.github.com/en/copilot/concepts/prompting-and-context/context-management)
- [docs.github.com: mcp](https://docs.github.com/en/copilot/how-tos/copilot-cli/customize-copilot/add-mcp-servers)
