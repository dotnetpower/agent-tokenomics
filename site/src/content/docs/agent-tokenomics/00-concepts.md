---
title: "0. Concepts: how an agent spends tokens"
description: The mental model you need before diagnosing - request anatomy, the agent loop, pricing dimensions, caching, and context.
track: agent
verified: 2026-11-03
sidebar:
  order: 0
---

Read this once before the [troubleshooting stages](../01-diagnose/). Every later page assumes these ideas.

## 1. A model call is stateless

The model remembers nothing between calls. An agent "remembers" only because **every request re-sends** the
instructions, tool definitions, and the conversation so far. Token cost is therefore driven less by what you
type than by what the framework **carries along** on each call.

## 2. Anatomy of one request

| Part | Direction | Changes between requests? | Typical waste |
|------|-----------|---------------------------|---------------|
| System prompt (framework + yours) | input | No — stable | Long, repeated rules |
| Instruction files / standing facts | input | Rarely | Facts the code already shows; duplicates |
| **Tool definitions** (schemas) | input | Rarely | Tools never called |
| Conversation history (user, assistant) | input | Grows every turn | Old topics, resolved detours |
| **Tool results** (file reads, command output) | input | Grows every step | Whole files, verbose logs |
| Attachments / retrieved context | input | Per task | Irrelevant or duplicated context |
| Current user prompt | input | Yes | Usually the smallest part |
| Model text, tool calls | output | Yes | Verbosity, restating the question |
| Reasoning ("thinking") | output (typically billed as output) | Yes | Effort higher than the task needs |

The top rows form the **prefix**: the part that stays the same from call to call. That matters for caching
(section 5).

## 3. The agent loop multiplies everything

```text
step 1: model call ─► tool call ─► result appended
step 2: model call (re-sends prefix + history + result 1) ─► tool call ─► result appended
step 3: model call (re-sends prefix + history + result 1..2) ─► …
```

Each step is a billed model call, and each call re-sends all earlier content. Over a task of *n* steps:

```text
input tokens ≈ n × prefix + Σ(history and tool results accumulated before each step)
```

The first term grows linearly with steps; the second grows faster than linearly because every step adds to
what all later steps re-send. This is why **fewer steps**, **smaller results**, and **smaller prefixes** all pay
off, and why a loop that gets stuck is expensive.

**Pipelines multiply too.** Not every agent is a free-form loop. Many production agents run a fixed chain of
model calls per user request (for example: safety check → query rewrite → router → classifier → answer →
follow-up question → final reply). The user sees one question; the provider bills **one call per step**, and
each step carries its own system prompt and reference documents. Call this **fan-out** (LLM calls per user
request). Anything large and attached to several steps (or to a step that is called twice) is paid
`tokens × calls`.

## 4. What you are billed for

| Dimension | What it is | Why it matters |
|-----------|------------|----------------|
| Uncached input tokens | Input processed from scratch | Full input price |
| **Cached input tokens** | Input served from the provider's prompt cache | Billed at a reduced rate on most providers |
| Output tokens (incl. reasoning) | Everything the model generates | Usually priced higher per token than input |

Rates differ by provider and model, and change; read the current price list of the model you use instead of
relying on a ratio from an article. Total cost for a request is
`uncached_in × p_in + cached_in × p_cached + out × p_out`, summed over all requests.

**Cost is not the only goal.** Decide which you are optimizing, because the metrics differ:

| Goal | Typical situation | Metrics |
|------|-------------------|---------|
| Money | Pay-as-you-go or credits | Tokens by type per task, cost per successful task |
| Capacity | Reserved / provisioned throughput, rate limits | Tokens per minute per request, utilization, concurrent users served |
| Latency | Interactive use | Input tokens per call, time to first token, p95 end-to-end time |

Fewer input tokens generally help all three. Caching helps money and capacity only if your provider counts
cached tokens favorably for your deployment type (see the checklist in section 5).

## 5. Prompt caching

Providers can reuse the **longest matching prefix** of a request seen recently. A cache hit needs the start of
the request to be *identical*, so one early change (a reordered instruction, an edited tool list, a rewritten
history) invalidates the cache from that point on. Behavior is provider-, model-, and deployment-specific:
switching models does not reuse the old cache. Design rule: **stable content first, volatile content last,
append-only history**. In VS Code, the
[Cache Explorer](https://code.visualstudio.com/docs/agents/agent-troubleshooting/cache-explorer) shows the
first divergence between consecutive requests. Provider docs:
[Anthropic](https://platform.claude.com/docs/en/build-with-claude/prompt-caching),
[OpenAI](https://developers.openai.com/api/docs/guides/prompt-caching).

### Provider caching: what to verify

Caching rules differ by provider, model family, and deployment type, and they change. Before you design around
caching, get written answers to these. The right-hand column records what the official docs said on 2026-11-03
for Azure OpenAI and OpenAI; re-verify for your model.

| Question | Why it matters | What the docs said (2026-11-03) |
|----------|----------------|---------------------------------|
| Minimum cacheable prefix? | Short prompts (guardrails, small classifiers, summarizers) may never cache | Azure OpenAI: the request needs ≥ 1,024 tokens and the first 1,024 must be identical. OpenAI: 1,024 minimum for GPT-5.6 and later; varies for earlier models |
| Which models and retention policies? | A model without support, or without extended retention, changes the plan | Azure: in-memory retention for GPT-4o or newer; extended retention only for a listed set of models, so check the list for your exact model, including "mini" variants |
| Where do hits show up? | You cannot tune what you cannot see | Azure chat completions: `usage.prompt_tokens_details.cached_tokens` (0 means a miss); on GPT-5.6 and later Standard deployments also `cache_write_tokens` |
| Does a cache key help? | Routing related requests together raises hit rate | Azure (GPT-5.6 and later): reuse one `prompt_cache_key` per shared prefix; above about 15 requests per minute for one prefix and key, some requests may miss, so partition across keys with a stable mapping |
| How long is it kept? | Determines whether a quiet period wastes the cache | Azure in-memory: typically cleared after 5–10 minutes idle, always within an hour. Extended: up to 24 hours (key/value tensors offloaded to GPU-local storage). On GPT-5.6 and later the policy field is deprecated in favor of a 30-minute minimum lifetime |
| How is it billed and counted? | Determines whether caching saves money, capacity, or both | Azure: discounted cache reads on Standard; up to a 100% input discount on Provisioned deployments; on GPT-5.6 and later, cache writes can incur charges. OpenAI: pricing varies by model. Ask how cached tokens count toward your capacity utilization |
| What is stored, and where? | Security and data-residency review | OpenAI: the cache stores key-value tensors, not the tokens. Azure: not shared between subscriptions; extended retention stays within the regional or data-zone boundary of the deployment type |
| Can it be disabled or cleared? | Needed for some compliance cases | Azure: on GPT-5.6 and later Standard deployments, `explicit` mode with no breakpoints disables caching; earlier models and managed provisioned deployments cannot disable it |

Short prompts that cannot be cached are a design signal, not a problem to pad away: merge the step into a
neighbor, move it to a deterministic check, or accept that it is small. Do not add filler tokens to reach a
threshold.

## 6. Context window and context rot

The **context window** is the maximum the model can consider in one request (input and output share it). It is
a hard limit *and* a soft one: models use information in the middle of long contexts less reliably than at the
start or end ([Lost in the Middle](https://arxiv.org/abs/2307.03172)), so quality can degrade well before the
window is full. Cost and quality both argue for **as little context as the task needs, but enough**.

## 7. Tools have two costs

1. **Definition cost** — every enabled tool's schema is in the prefix of every request, used or not.
2. **Result cost** — what a tool returns is appended to the history and re-sent on all later steps.

Pruning unused tools attacks the first; truncating, paginating, or summarizing results attacks the second.

## 8. Cost without quality is meaningless

A cheaper run that fails is not a saving. Track **cost per successful task**. Also note that per-step errors
compound across a long loop (see the table in [Prove](../03-prove/#multi-step-reliability)), so shortening a
loop can improve success and cost together.

## 9. From concepts to causes

| Concept | Waste it creates | Where you diagnose it |
|---------|------------------|-----------------------|
| Stateless re-send, big prefix | Fixed overhead; re-learned facts | [Diagnose](../01-diagnose/), [Isolate](../02-isolate/) |
| Tool definition cost | Unused tools | [Isolate](../02-isolate/) |
| Tool result cost | Bloated output | [Improve](../04-improve/) |
| Loop multiplication | Failure loops, retries | [Prove](../03-prove/) |
| History accumulation | History growth, context rot | [Improve: compaction](../04-improve/#compaction-without-losing-the-cache) |
| Cache prefix rule | Prefix churn, cache misses | [Diagnose](../01-diagnose/) |
| Output pricing, reasoning | Verbosity, over-sized effort | [Improve](../04-improve/) |

## Worked example

In the synthetic log [`sample-requests.jsonl`](https://github.com/dotnetpower/agent-tokenomics/blob/main/examples/sample-requests.jsonl),
the first request of session `s1` is 9,200 input tokens. Its four tool definitions add up to
220 + 180 + 900 + 1,100 = **2,400 tokens (about 26% of that request)**, and two of those tools are never called.
From request 2 on, 9,000 of 9,600 input tokens are served from cache — *because the prefix stayed identical*.
Remove a tool or reorder instructions mid-session and that hit rate would fall. That single log already shows
three concepts: prefix overhead, tool-definition cost, and caching.

## Next

Start measuring in [1. Diagnose](../01-diagnose/).

## Further reading (papers)

Ordered by venue tier, then citations. Findings are hypotheses to verify on your own workload; see the full list in [Papers](../../papers/).

- [ReAct: Synergizing Reasoning and Acting in Language Models](https://arxiv.org/abs/2210.03629) — ICLR 2022, 13,383 citations. Interleaves reasoning and tool actions: the agent loop whose cost this guide analyzes.
- [Efficient Memory Management for Large Language Model Serving with PagedAttention](https://arxiv.org/abs/2309.06180) — SOSP 2023, 9,724 citations. PagedAttention manages KV-cache memory to raise serving throughput; basis of vLLM.
- [Lost in the Middle: How Language Models Use Long Contexts](https://arxiv.org/abs/2307.03172) — TACL 2023, 5,818 citations. Accuracy drops when the relevant information sits in the middle of a long input.
- [SGLang: Efficient Execution of Structured Language Model Programs](https://arxiv.org/abs/2312.07104) — NeurIPS 2023, 1,783 citations. RadixAttention reuses KV cache across calls that share a prefix; foundation for prefix caching.
- [RULER: What's the Real Context Size of Your Long-Context Language Models?](https://arxiv.org/abs/2404.06654) — preprint 2024, 1,580 citations. Benchmark showing the effective context length is often shorter than the advertised window.

## Sources

- [Diagnose prompt caching with the Cache Explorer (VS Code)](https://code.visualstudio.com/docs/agents/agent-troubleshooting/cache-explorer)
- [Optimize AI credit usage in VS Code](https://code.visualstudio.com/docs/agents/guides/optimize-usage)
- [Prompt caching — Anthropic](https://platform.claude.com/docs/en/build-with-claude/prompt-caching), [OpenAI](https://developers.openai.com/api/docs/guides/prompt-caching)
- [Lost in the Middle (arXiv:2307.03172)](https://arxiv.org/abs/2307.03172)
- [TokenEconomizer](https://github.com/microsoft/TokenEconomizer): describes the same carried-along costs (instructions, loaded tools, conversation so far)
