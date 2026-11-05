---
title: "Copilot Tokenomics (Token Economics)"
description: "Practical tokenomics and token economics for GitHub Copilot in VS Code: save tokens and AI credits, trim context and MCP overhead, and speed up coding with cctx."
track: copilot
verified: 2026-11-03
---

Track A covers **Copilot Tokenomics (token economics)** for developers who **use** GitHub Copilot in VS Code. Every page follows the same shape:
*Problem → Action → How to verify → Trade-offs*.

Check your Copilot billing settings first; the best levers depend on your plan.

| Billing model | What is charged | Start with |
|---|---|---|
| AI Credits (usage-based) | Input, cached input, and output tokens × their model rates | [Measure usage](02-measure-usage/), [context](03-context-hygiene/), and [tools](04-instructions-and-tools/) |
| Legacy Premium Requests | Each prompt/turn × the model multiplier | [Billing](01-how-usage-is-charged/), [models](05-models-and-planning/), and [budget guards](08-agent-modes-and-budget/) |

<figure class="at-doc-figure">
  <img src="https://code.visualstudio.com/assets/docs/agents/concepts/context-assembly.png" alt="VS Code diagram showing how agent context is assembled" loading="lazy" />
  <figcaption>Agent context is assembled from multiple sources. Source: <a href="https://code.visualstudio.com/docs/agents/concepts/context">VS Code Docs — Context in Agents</a>.</figcaption>
</figure>

| Page | Question it answers |
|------|---------------------|
| [How Copilot usage is charged](01-how-usage-is-charged/) | What am I actually spending? |
| [Measure your usage](02-measure-usage/) | Which chats and requests are expensive? |
| [Keep context lean](03-context-hygiene/) | How do I stop paying for irrelevant history? |
| [Instructions and tools](04-instructions-and-tools/) | What is sent on every request, and can it shrink? |
| [Models, planning, delegation](05-models-and-planning/) | When is a cheaper model or a plan-first flow worth it? |
| [Prompt and output style](06-prompt-and-output-style/) | How do I shorten prompts and cap answers? |
| [Language and tokenization](07-language-and-tokenization/) | Is writing in my language more expensive? |
| [Agent modes and budget guards](08-agent-modes-and-budget/) | Ask vs Plan vs Agent; how do I bound the loop? |
| [MCP audit](09-mcp-audit/) | Which tool servers should stay enabled? |
| [Checklist](10-checklist/) | A one-page routine. |
| [cctx: VS Code Token Saver & Accelerator](cctx/) | How do I cut lookup tokens, speed up coding tasks, and see live AIC/USD savings in VS Code? |
| [Resources](../resources/) | Tools and repos worth knowing. |
| [Papers](../papers/) | Research behind these tactics, ranked by authority. |

:::note
Copilot billing, UI, and settings change quickly. Each page states the date it was verified
and links the official source. If something differs in your VS Code, trust the official docs.
:::
