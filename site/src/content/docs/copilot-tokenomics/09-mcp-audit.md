---
created: 2026-11-03
updated: 2026-11-19
dateSource: git
title: MCP audit
description: Every enabled tool is a per-request tax. Audit servers by usage and scope them per workspace.
track: copilot
verified: 2026-11-19
sidebar:
  order: 9
---

> **Scope: public documentation reviewed on 2026-11-19.** Prices, policies, UI, and settings can change. Check the original source for your client/version/harness. This date does not claim a new installation, billing, or model-quality experiment.

## Problem

Distinguish an installed server, a discovered tool, a loaded full schema, and an executed call. Only measuring server/tool counts cannot establish billed token savings.

## Actions

| Audit field | Record |
|---|---|
| Scope | Client/version/harness/model and repository configuration |
| Catalog | Enabled servers, discovered tools, loaded definitions where visible |
| Actual work | Call counts, result size, failures, latency |
| Outcome | Credits, retries, and tests/task success |

In VS Code's Copilot harness, use [Chat: Open Customizations → Tools](https://code.visualstudio.com/docs/agents/run/tools) and inspect [actual requests](https://code.visualstudio.com/docs/agents/agent-troubleshooting/chat-debug-view). The selection persists for the harness/host and can affect other sessions; Local's controls differ. In CLI, use [/mcp and explicit tool selection](https://docs.github.com/en/copilot/how-tos/copilot-cli/customize-copilot/add-mcp-servers). Keep repository settings reviewable and do not commit credentials.

### On-demand tool loading and virtual grouping

Deferred loading and grouping depend on the current harness, model, and runtime. Old experimental keys such as `github.copilot.chat.virtualTools.threshold` are **not a universal current tuning recipe**. Confirm that a setting is supported before changing it. Compare loaded schemas, discovery overhead, and later calls rather than assuming "unused means free" or "enabled means full schema every turn."

### Scope per workspace, not globally

Keep only required servers/tools active for a task. Configuration paths differ by client; do not copy VS Code `.vscode/mcp.json` into another runtime and assume it is discovered. A tool allowlist and an execution permission policy solve different problems.

### Keep tool output small

Filter new results before adding them to history, preserving failures and counts. Copilot CLI supports large-output offloading; inspect the file with bounded reads rather than loading it all again. See [context and output controls](https://docs.github.com/en/copilot/concepts/prompting-and-context/context-management).

## How to verify

Disable one unused server or narrow one catalog. Repeat the same task and compare first-request input, search/loading calls, whole-session credits, and success using [the baseline template](../02-measure-usage/). Restore a needed tool instead of accepting a broken cheaper workflow.

## Trade-offs

Delayed discovery can add a call. A curated terminal script can replace repetitive operations but needs maintenance and security review. Unlinked deck figures previously used here are excluded; fewer tool definitions are not themselves a measured end-to-end saving.

## Further reading (papers)

Ordered by venue tier, then citations. Findings are hypotheses to verify on your own workload; see the full list in [Papers](../../papers/).

- [MCP-Zero: Active Tool Discovery for Autonomous LLM Agents](https://arxiv.org/abs/2506.01056) - preprint 2025, 55 citations. Lets the agent request tools on demand instead of injecting all tool schemas.
- [RAG-MCP: Mitigating Prompt Bloat in LLM Tool Selection via Retrieval-Augmented Generation](https://arxiv.org/abs/2505.03275) - preprint 2025, 41 citations. Retrieves only the relevant MCP tool descriptions instead of loading every tool (prompt bloat).
- [Less is More: Optimizing Function Calling for LLM Execution on Edge Devices](https://arxiv.org/abs/2411.15399) - preprint 2024, 33 citations. Presents a smaller selected tool set at function-calling time to cut cost on edge devices.

## Sources

- [code.visualstudio.com: harness](https://code.visualstudio.com/blogs/2026/11/18/copilot-harness)
- [code.visualstudio.com: tools](https://code.visualstudio.com/docs/agents/run/tools)
- [docs.github.com: mcp](https://docs.github.com/en/copilot/how-tos/copilot-cli/customize-copilot/add-mcp-servers)
- [code.visualstudio.com: debug](https://code.visualstudio.com/docs/agents/agent-troubleshooting/chat-debug-view)
- [code.visualstudio.com: cache](https://code.visualstudio.com/docs/agents/agent-troubleshooting/cache-explorer)
- [docs.github.com: context](https://docs.github.com/en/copilot/concepts/prompting-and-context/context-management)
