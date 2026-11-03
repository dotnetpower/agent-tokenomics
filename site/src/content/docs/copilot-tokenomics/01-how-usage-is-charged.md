---
title: How Copilot usage is charged
description: Usage-based billing with AI credits, and why tokens matter.
track: copilot
verified: 2026-11-03
sidebar:
  order: 1
---

## Problem

You cannot reduce what you do not understand. Two billing models exist today, and they reward
different habits.

## What changed

- **Usage-based billing (current).** GitHub announced that from **June 1, 2026**, Copilot usage
  consumes **GitHub AI Credits**. Credits are consumed at different rates depending on the **model**
  and the **number of tokens processed**. Each plan includes a monthly allowance.
- **Legacy premium-request billing.** Some Pro / Pro+ subscribers on an existing annual plan stayed on
  request-based billing. There, each prompt in chat counts as one premium request multiplied by the
  model's rate, and autonomous tool calls inside an agent run do not count separately.

| | Usage-based (AI credits) | Legacy (premium requests) |
|---|---|---|
| Unit | Tokens × model rate | Prompts × model multiplier |
| Long chat history | Costs more (re-sent as input) | Not billed per token |
| Large tool catalog / instructions | Costs more (sent every request) | Not billed per token |
| Strongest lever | Smaller context, right model | Fewer prompts, lower-multiplier model |

Check which plan you are on in your GitHub Copilot billing settings before choosing tactics.

## Why tokens matter for every request

Each request sends **input tokens** (system prompt, instructions, tool definitions, the whole
conversation so far, attachments, tool results) and receives **output tokens** (including any
reasoning). Input grows with every turn, so a long session pays for old context again and again.
Providers can reuse an unchanged request prefix (**prompt caching**), which lowers cost and latency
when the beginning of the request stays stable.

## Trade-offs

Everything in this track trades *context* against *cost*. Removing context the model needs leads to
retries and rework, which can cost more than it saves.

## Sources

- [GitHub Copilot is moving to usage-based billing (GitHub Blog, 2026-04-27)](https://github.blog/news-insights/company-news/github-copilot-is-moving-to-usage-based-billing/)
- [Usage-based billing for individuals (GitHub Docs)](https://docs.github.com/en/copilot/concepts/billing/usage-based-billing-for-individuals)
- [Requests in GitHub Copilot — legacy (GitHub Docs)](https://docs.github.com/en/copilot/concepts/billing/copilot-requests)
- [Optimize AI credit usage in VS Code](https://code.visualstudio.com/docs/agents/guides/optimize-usage)
