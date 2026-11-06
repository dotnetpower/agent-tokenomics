---
title: Measure your usage
description: "Measure GitHub Copilot tokenomics and AI credit spend in VS Code using Agent Debug Logs, Cache Explorer, context window controls, and cctx usage telemetry."
track: copilot
verified: 2026-11-03
sidebar:
  order: 2
---

## Problem

"Copilot feels expensive" is not actionable. You need to know *which* session, request, or component
costs the most.

## Actions

1. **Per request** — hover over a chat response to see the credits used by that turn.
2. **Per session** — hover over (or select) the **context window control** in the chat input. The popover
   shows total session cost in credits and cumulative context-window token usage.
3. **Per month** — open the **Copilot status dashboard** from the VS Code status bar to see the
   percentage of your monthly allowance used, with a link to usage details on GitHub.
4. **What is inside a session** — open **Agent Debug Logs** (the `...` menu in the Chat view →
   *Show Agent Debug Logs*). The **Summary** view shows aggregate token usage, tool calls, and
   duration; the **Logs** view shows individual requests, errors, and tool calls.
5. **Prompt caching** — use the **Cache Explorer** (Agent Debug Logs → Summary → *Cache Explorer*) when
   the Summary reports low cache reuse together with unexpected duration or input tokens. It diffs
   consecutive requests and shows the *first divergence* in the prompt signature (system
   instructions, tool definitions, messages).
6. **Personalized tips** — run `/chronicle:cost-tips` in a chat session for recommendations based on
   your recent activity. This requires a recent VS Code / Copilot Chat build with the Chronicle cost-tips participant enabled. If it is unavailable, use Agent Debug Logs → **Summary** and **Cache Explorer** directly.
7. **Model cost** — the model picker's hover shows cost per token type and a Low / Medium / High tier.
8. **Live retrieval & multi-session savings (`cctx`)** — if you install the [Code Context (`cctx`) VS Code extension](../cctx/) from the Marketplace (`dotnetpower.code-context-native`), hover over the `CCTX N` status bar badge or run `cctx usage --hours 24` in the terminal to see per-model token spend, prompt-cache hit rates, lookup latency, and estimated AIC/USD savings across recent sessions.

<figure class="at-doc-figure">
  <div class="at-doc-figure-grid">
    <img src="https://code.visualstudio.com/assets/docs/agents/chat-debug-view/agent-logs-summary-v3.png" alt="VS Code Agent Logs Summary view showing aggregate token usage and tool call counts" loading="lazy" />
    <img src="https://code.visualstudio.com/assets/docs/agents/cache-explorer/cache-explorer.png" alt="VS Code Cache Explorer side-by-side comparison showing cached prefix tokens and the first divergence point" loading="lazy" />
  </div>
  <figcaption>
    VS Code Agent Debug Logs Summary view (left) and Cache Explorer side-by-side prompt prefix comparison (right). Sources: <a href="https://code.visualstudio.com/docs/copilot/chat/chat-debug-view">VS Code Docs — Agent Debug View</a> and <a href="https://code.visualstudio.com/docs/agents/agent-troubleshooting/cache-explorer">Cache Explorer</a>.
  </figcaption>
</figure>

## How to verify

Pick one representative task and run it twice with the same model. Record per-request credits, session
total, and cache hit %. This is your baseline for every later change.

For a before/after comparison, copy the request or session summary values into your baseline record, or select the session in Agent Debug Logs and use **Export** to save its OpenTelemetry JSON. Compare the two runs with the same task and model; review exports for sensitive content before sharing.

## Trade-offs and cautions

- The Cache Explorer diff explains a reported cache metric; it does **not** prove how a provider
  processed or billed the request. Cache information may be absent for some models.
- Do not remove necessary instructions or tools just to raise cache reuse. Quality comes first.

## Further reading (papers)

Ordered by venue tier, then citations. Findings are hypotheses to verify on your own workload; see the full list in [Papers](../../papers/).

- [SGLang: Efficient Execution of Structured Language Model Programs](https://arxiv.org/abs/2312.07104) — NeurIPS 2023, 1,783 citations. RadixAttention reuses KV cache across calls that share a prefix; foundation for prefix caching.
- [Prompt Cache: Modular Attention Reuse for Low-Latency Inference](https://arxiv.org/abs/2311.04934) — MLSys 2023, 417 citations. Reuses precomputed attention states of recurring prompt modules to cut latency.
- [Don't Break the Cache: An Evaluation of Prompt Caching for Long-Horizon Agentic Tasks](https://arxiv.org/abs/2601.06007) — preprint 2026, 25 citations. Evaluates prompt caching on long-horizon agentic tasks.

## Sources

- [Optimize AI credit usage in VS Code — Monitor and Inspect sections](https://code.visualstudio.com/docs/agents/guides/optimize-usage)
- [Diagnose prompt caching with the Cache Explorer](https://code.visualstudio.com/docs/agents/agent-troubleshooting/cache-explorer)
- [Debug chat interactions](https://code.visualstudio.com/docs/agents/agent-troubleshooting/chat-debug-view)
