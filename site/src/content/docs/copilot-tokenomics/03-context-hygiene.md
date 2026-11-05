---
title: Keep context lean
description: New chats, forks, compaction, focused attachments, and exclusions.
track: copilot
verified: 2026-11-03
sidebar:
  order: 3
---

## Problem

A conversation accumulates messages, tool outputs, and file contents. Every new request re-sends that
history as input, so an unrelated old topic keeps costing tokens without improving answers.

### Context window and context rot

The **context window** is the maximum the model can consider in one request (input and output share it). It is
a hard limit *and* a soft one: models use information in the middle of long contexts less reliably than at the
start or end ([Lost in the Middle](https://arxiv.org/abs/2307.03172)), so quality can degrade well before the
window is full. Cost and quality both argue for **as little context as the task needs, but enough**.

<figure class="at-doc-figure">
  <div class="at-doc-figure-grid">
    <img src="https://code.visualstudio.com/assets/docs/chat/copilot-chat/chat-context-window-control.png" alt="VS Code Chat context window control showing token usage by category" loading="lazy" />
    <img src="https://code.visualstudio.com/assets/docs/chat/copilot-chat/copilot-chat-view-chat-variables.png" alt="VS Code Chat context picker for adding relevant context" loading="lazy" />
  </div>
  <figcaption>Monitor context usage (left) and add relevant context (right). Sources: <a href="https://code.visualstudio.com/docs/copilot/chat/copilot-chat-context">VS Code Docs — Add context to chat</a> and <a href="https://code.visualstudio.com/docs/agents/guides/optimize-usage">Optimize AI credit usage</a>.</figcaption>
</figure>

## Actions

| Situation | Action |
|-----------|--------|
| Switching to an unrelated task | Start a **new chat session** (`Ctrl+N` / `⌘N`). |
| Want to try an alternative or ask a side question | **Fork** the conversation (`/fork`, or hover a message → *Fork Conversation*) instead of re-explaining. |
| Long session you still need | Run `/compact` (optionally `/compact focus on the API design decisions`) to summarize older turns. |
| You know the relevant code | **#-mention** the file, folder, or symbol rather than letting the agent search broadly. |
| You need codebase search | `#codebase` runs a search; it does **not** add the whole codebase to context. |
| Noisy search results (logs, build output, generated files) | Exclude them: `.gitignore` (index + agent search), `files.exclude` (hide entirely), `search.exclude` (hide from agent search only). |
| Web reference | Use `#fetch` with a URL only when needed; fetched pages enter the context. |
| Independent research | Delegate to a **subagent** so the exploration stays out of the main conversation. |

Search-match snippets count toward the context window even when the agent never opens the file, so
excluding noisy paths helps directly.

## How to verify

Repeat the baseline task from [Measure your usage](../02-measure-usage/) after the change and compare
session credits and the context-window token count. Confirm the answer quality is unchanged.

## Trade-offs

- `/compact` is lossy: details you still need may be summarized away. Add focus instructions, and say what
  must survive (decisions, constraints, file paths, open items).
- Compacting changes the start of the history, so the provider's prompt cache for that prefix is rebuilt. Compact
  once at a natural pause rather than repeatedly; for a short remaining task, finishing it is often cheaper.
- If a long session is dominated by big tool results (file reads, command output), trimming those (or starting a
  new chat with a short hand-off note) usually helps more than summarizing the conversation.
- Starting a new chat loses history; put durable facts into [custom instructions](../04-instructions-and-tools/)
  so you do not re-explain them.
- Attach the **smallest set of authoritative context** that lets the task succeed — not everything available.

## Further reading (papers)

Ordered by venue tier, then citations. Findings are hypotheses to verify on your own workload; see the full list in [Papers](../../papers/).

- [Lost in the Middle: How Language Models Use Long Contexts](https://arxiv.org/abs/2307.03172) — TACL 2023, 5,818 citations. Accuracy drops when the relevant information sits in the middle of a long input.
- [Large Language Models Can Be Easily Distracted by Irrelevant Context](https://arxiv.org/abs/2302.00093) — ICML 2023, 1,358 citations. Irrelevant context in the prompt can substantially reduce model accuracy.
- [LLMs Get Lost In Multi-Turn Conversation](https://arxiv.org/abs/2505.06120) — ICLR 2025, 562 citations. Models perform worse in multi-turn, underspecified conversations than in single-turn ones.
- [Same Task, More Tokens: the Impact of Input Length on the Reasoning Performance of Large Language Models](https://arxiv.org/abs/2402.14848) — ACL 2024, 270 citations. Isolates input length as a factor and shows reasoning degrades as inputs grow.
- [The Complexity Trap: Simple Observation Masking Is as Efficient as LLM Summarization for Agent Context Management](https://arxiv.org/abs/2508.21433) — preprint 2025, 66 citations. Simple observation masking is as efficient as LLM summarization for agent context management.

## Sources

- [Add context to chat](https://code.visualstudio.com/docs/chat/copilot-chat-context)
- [Optimize AI credit usage in VS Code](https://code.visualstudio.com/docs/agents/guides/optimize-usage)
