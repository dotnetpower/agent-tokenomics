---
title: Resources
description: Repositories, papers, and docs for saving tokens. Curated and checked for existence on 2026-11-03.
sidebar:
  order: 90
---

A curated list. Every link was checked on **2026-11-03**. Inclusion is **not an endorsement**: each project's
savings figures are its authors' own claims, and we have not reproduced them. Benchmark on your own tasks
and track a quality metric next to cost (see [Agent Tokenomics](../agent-tokenomics/)).

:::caution
Proxies, gateways, and compressors sit between your agent and the model, so they **see your prompts and code**.
Read the license and data-handling notes, and review before using them with private repositories.
:::

## Start here

| Resource | Why |
|----------|-----|
| [microsoft/TokenEconomizer](https://github.com/microsoft/TokenEconomizer) | Experience-driven cost control for coding agents: micro-skills, tool pruning, loop guard. Paper: [arXiv:2610.22732](https://arxiv.org/abs/2610.22732), [project page](https://tokeneconomizer.github.io). |
| [Optimize AI credit usage in VS Code](https://code.visualstudio.com/docs/agents/guides/optimize-usage) | Official practical guide. |
| [Diagnose prompt caching with the Cache Explorer](https://code.visualstudio.com/docs/agents/agent-troubleshooting/cache-explorer) | Official: find the first prefix divergence between requests. |
| [Azure OpenAI prompt caching](https://learn.microsoft.com/en-us/azure/foundry/openai/how-to/prompt-caching) | Minimum length, `cached_tokens`, `prompt_cache_key`, retention, billing and data-residency rules for Azure OpenAI. |
| [OpenAI prompt caching](https://developers.openai.com/api/docs/guides/prompt-caching) | Cacheable prefix rules, settings that invalidate the cache, diagnostics. |
| [Usage-based billing for individuals](https://docs.github.com/en/copilot/concepts/billing/usage-based-billing-for-individuals) | How AI credits are charged. |
| [github/awesome-copilot](https://github.com/github/awesome-copilot) | Community instructions, agents, and skills for Copilot. |
| [pleasedodisturb/awesome-llm-token-optimization](https://github.com/pleasedodisturb/awesome-llm-token-optimization) | Another curated list of strategies, tools, and papers. |

## Reduce tool output and command noise

| Project | What it does |
|---------|--------------|
| [rtk-ai/rtk](https://github.com/rtk-ai/rtk) | CLI proxy that condenses output of common dev commands before the model sees it. |
| [headroomlabs-ai/headroom](https://github.com/headroomlabs-ai/headroom) | Compresses tool outputs, logs, files, and RAG chunks before they reach the LLM. |
| [gglucass/headroom-desktop](https://github.com/gglucass/headroom-desktop) | Desktop app wrapper around the same idea for coding agents. |
| [Dave-London/Pare](https://github.com/Dave-London/Pare) | Structured, token-efficient MCP servers for git, test runners, npm, Docker. |
| [claw-compactor](https://github.com/open-compress/claw-compactor) | Multi-stage, reversible compression pipeline with code-aware analysis. |
| [optscale-kompressor](https://github.com/hystax/optscale-kompressor) | Deterministic prompt compression for coding agents in one Rust binary. |
| [fkiene/llmtrim](https://github.com/fkiene/llmtrim) | Local proxy that trims prompts, history, and tool output in API requests. |
| [Paritok-official/paritok-4b-v1](https://github.com/Paritok-official/paritok-4b-v1) | Non-destructive compression gateway for coding agents. |

## Compress prompts and data

| Project | What it does |
|---------|--------------|
| [microsoft/LLMLingua](https://github.com/microsoft/LLMLingua) | Prompt compression research (LLMLingua, LLMLingua-2). Paper: [arXiv:2310.05736](https://arxiv.org/abs/2310.05736). |
| [jia-gao/leanctx](https://github.com/jia-gao/leanctx) | Drop-in prompt compression SDK for production apps (LLMLingua-2 based). |
| [3DAgentWorld/Toolkit-for-Prompt-Compression](https://github.com/3DAgentWorld/Toolkit-for-Prompt-Compression) | Toolkit collecting prompt-compression methods. |
| [ZongqianLi/Prompt-Compression-Survey](https://github.com/ZongqianLi/Prompt-Compression-Survey) | Survey of prompt-compression methods (NAACL 2025). |
| [sriinnu/clipforge-PAKT](https://github.com/sriinnu/clipforge-PAKT) | Lossless-first compression for JSON, YAML, CSV, Markdown. |
| [ISON-format/ison](https://github.com/ISON-format/ison) | Token-efficient JSON alternative for LLM data. |

## Terse style and measured evaluation

| Project | What it does |
|---------|--------------|
| [JuliusBrussee/caveman](https://github.com/JuliusBrussee/caveman) | Skill/proxy that makes agents answer tersely. |
| [danielle34/cavewoman](https://github.com/danielle34/cavewoman) | Evaluation framework measuring what input vs output compression actually saves. |
| [YangJunMan/claude-code-token-savings-benchmark](https://github.com/YangJunMan/claude-code-token-savings-benchmark) | Reproducible benchmark comparing several token-saving tools. |

## Retrieve less code and keep memory small

| Project | What it does |
|---------|--------------|
| [Graphify-Labs/graphify](https://github.com/Graphify-Labs/graphify) | Turns a folder (code, docs, PDFs, images) into a queryable knowledge graph. |
| [jgravelle/jcodemunch-mcp](https://github.com/jgravelle/jcodemunch-mcp) | Symbol-level retrieval: fetch the function you need, not the whole file. |
| [redhat-et/ripwire](https://github.com/redhat-et/ripwire) | CLI + MCP server to find relevant code without reading the repo. |
| [yamadashy/repomix](https://github.com/yamadashy/repomix) | Packs a repository into one AI-friendly file (use selectively; big packs cost tokens). |
| [radimsem/remindb](https://github.com/radimsem/remindb) | Portable agent memory in a SQLite file. |
| [cytostack/openwolf](https://github.com/cytostack/openwolf) | Project memory across agents, with token accounting from transcripts. |
| [fajarhide/contexere](https://github.com/fajarhide/contexere) | One place for skills, keys, and memory across agents. |

## Audit tokens, tools, and cost

| Project | What it does |
|---------|--------------|
| [alexgreensh/token-optimizer](https://github.com/alexgreensh/token-optimizer) | Finds "ghost tokens" and context-quality decay in agent sessions. |
| [activeing123/mcptoon](https://github.com/activeing123/mcptoon) | MCP gateway CLI aimed at tool discovery and context compression. |
| [ccusage/ccusage](https://github.com/ccusage/ccusage) | Usage and cost reports from local Claude Code logs. |
| [Maciek-roboblog/Claude-Code-Usage-Monitor](https://github.com/Maciek-roboblog/Claude-Code-Usage-Monitor) | Real-time usage monitor with predictions. |
| [junhoyeo/tokscale](https://github.com/junhoyeo/tokscale) | Token usage across several AI coding agents from the terminal. |

## Observability and gateways (for agents you operate)

| Project | What it does |
|---------|--------------|
| [langfuse/langfuse](https://github.com/langfuse/langfuse) | Open-source tracing, evals, and cost tracking for LLM apps. |
| [Arize-ai/phoenix](https://github.com/Arize-ai/phoenix) | AI observability and evaluation on OpenTelemetry ([docs](https://arize.com/docs/phoenix)). |
| [Helicone/helicone](https://github.com/Helicone/helicone) | LLM observability platform and OpenAI-compatible gateway ([docs](https://docs.helicone.ai/getting-started/quick-start)). |
| [LangSmith](https://docs.langchain.com/langsmith/cost-tracking) | LangChain's hosted tracing and evaluation platform with token and cost breakdowns (SDKs: [Python](https://github.com/langchain-ai/langsmith-python), [JS](https://github.com/langchain-ai/langsmith-javascript)). |
| [Portkey-AI/gateway](https://github.com/Portkey-AI/gateway) | AI gateway with guardrails; Portkey's platform adds monitoring ([docs](https://portkey.ai/docs/introduction/what-is-portkey)). |
| [openlit/openlit](https://github.com/openlit/openlit) | OpenTelemetry-based tracing and evaluation for agents. |
| [AgentOps-AI/agentops](https://github.com/AgentOps-AI/agentops) | Agent monitoring and LLM cost tracking SDK. |
| [BerriAI/litellm](https://github.com/BerriAI/litellm) | Gateway with cost tracking and routing across 100+ providers. |
| [maximhq/bifrost](https://github.com/maximhq/bifrost) | LLM / MCP / agent gateway with failover. |

These map to [stage 1 (Diagnose)](../agent-tokenomics/01-diagnose/#collect-the-request-log), which compares Phoenix, Langfuse, LangSmith, Helicone, and Portkey. They give you the per-request log that the
[example script](https://github.com/dotnetpower/agent-tokenomics/tree/main/examples) consumes.

## Context engineering and spec-driven workflows

| Project | What it does |
|---------|--------------|
| [github/spec-kit](https://github.com/github/spec-kit) | Toolkit for spec-driven development: plan first, then implement. |
| [Fission-AI/OpenSpec](https://github.com/Fission-AI/OpenSpec) | Spec-driven development for AI coding assistants. |
| [shanraisshan/claude-code-best-practice](https://github.com/shanraisshan/claude-code-best-practice) | Practices for agentic engineering with Claude Code. |
| [microsoft/vscode-copilot-chat](https://github.com/microsoft/vscode-copilot-chat) | Source of the VS Code Copilot Chat extension. |

## Papers

The full ranked list (by venue tier, then citations) is on the [Papers](../papers/) page. Highlights:

| Paper | Takeaway |
|-------|----------|
| [Evaluating AGENTS.md (arXiv:2602.11988)](https://arxiv.org/abs/2602.11988) | Context files did not generally improve task success and raised cost by over 20% on average; useful for non-standard practices. |
| [Lost in the Middle (arXiv:2307.03172)](https://arxiv.org/abs/2307.03172) | Models use information in the middle of long contexts less reliably than at the start or end. |
| [TokenEconomizer (arXiv:2610.22732)](https://arxiv.org/abs/2610.22732) | 18.4–30.9% lower mean cost on SWE-Bench across 5 agent–model combinations, as reported. |
| [LLMLingua (arXiv:2310.05736)](https://arxiv.org/abs/2310.05736) | Prompt compression with small language models. |
| [tiktoken](https://github.com/openai/tiktoken) | Tokenizer used by our [token counter](https://github.com/dotnetpower/agent-tokenomics/blob/main/examples/count_tokens.py). |

## Suggest a resource

Open an issue or PR with the link, a one-line description in your own words, and how you verified it exists.
