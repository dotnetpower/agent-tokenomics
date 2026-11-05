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

<figure class="at-doc-figure">
  <div class="at-diag-stack">
    <div class="at-diag-row">
      <span>Step 1 (Call #1)</span>
      <div class="at-diag-bar">
        <div class="at-diag-seg seg-prefix" style="width:42%">Prefix (System + Tools)</div>
        <div class="at-diag-seg seg-user" style="width:10%">User</div>
      </div>
      <span>9,200 tok</span>
    </div>
    <div class="at-diag-row">
      <span>Step 2 (Call #2)</span>
      <div class="at-diag-bar">
        <div class="at-diag-seg seg-prefix" style="width:42%">Prefix (re-sent)</div>
        <div class="at-diag-seg seg-hist" style="width:12%">History</div>
        <div class="at-diag-seg seg-tool" style="width:18%">Tool Result 1</div>
      </div>
      <span>13,800 tok</span>
    </div>
    <div class="at-diag-row">
      <span>Step 3 (Call #3)</span>
      <div class="at-diag-bar">
        <div class="at-diag-seg seg-prefix" style="width:42%">Prefix (re-sent)</div>
        <div class="at-diag-seg seg-hist" style="width:20%">History + Result 1</div>
        <div class="at-diag-seg seg-tool" style="width:28%">Tool Result 2</div>
      </div>
      <span>19,500 tok</span>
    </div>
  </div>
  <figcaption>
    Figure 1. Across an agent loop, the static prefix (<code>n × prefix</code>) is re-sent on every step while prior history and tool outputs (<code>Σ</code>) accumulate. References: <a href="https://arxiv.org/abs/2210.03629">ReAct (ICLR 2022)</a> and <a href="https://github.com/microsoft/TokenEconomizer">microsoft/TokenEconomizer</a>.
  </figcaption>
</figure>

**Long-context degradation in custom agents.** As history and tool results accumulate, models can use information buried in the middle less reliably than information at the beginning or end ([Lost in the Middle](https://arxiv.org/abs/2307.03172)); [RULER](https://arxiv.org/abs/2404.06654) also tests how effective context capacity varies by task. Unbounded history can therefore hurt task accuracy as well as increase token cost; verify the effect on your own agent.

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

### How tokens are counted across languages (tiktoken)

Models do not count characters or words. They split UTF-8 bytes into subword pieces using a **Byte Pair Encoding (BPE)** tokenizer such as OpenAI's [`tiktoken`](https://github.com/openai/tiktoken) (`o200k_base` for GPT-4o, GPT-4.1, GPT-5, and o-series; `cl100k_base` for GPT-4 and GPT-3.5). Because BPE vocabularies are trained predominantly on English and source code, the same meaning costs different token counts across languages (`examples/count_tokens.py`, `tiktoken` 0.14.0):

| Language | Sample sentence | Chars | UTF-8 bytes | `o200k_base` tokens | `cl100k_base` tokens |
|----------|-----------------|------:|------------:|--------------------:|---------------------:|
| English | I met a huge dog | 16 | 16 | **5** (1.0×) | **5** (1.0×) |
| Spanish | Conocí a un perro enorme | 24 | 25 | **7** (1.4×) | **8** (1.6×) |
| Korean | 나는 커다란 개를 만났다 | 13 | 35 | **8** (1.6×) | **13** (2.6×) |
| Japanese | 大きな犬に出会った | 9 | 27 | **8** (1.6×) | **11** (2.2×) |
| Chinese | 我遇見了一隻大狗 | 8 | 24 | **8** (1.6×) | **12** (2.4×) |

<figure class="at-doc-figure">
  <div class="at-tok-box">
    <div class="at-tok-row">
      <span class="at-tok-label">English (5 tokens)</span>
      <span class="at-tok-chip c1">I</span>
      <span class="at-tok-chip c2">·met</span>
      <span class="at-tok-chip c3">·a</span>
      <span class="at-tok-chip c4">·huge</span>
      <span class="at-tok-chip c5">·dog</span>
    </div>
    <div class="at-tok-row">
      <span class="at-tok-label">Korean o200k_base (8 tokens)</span>
      <span class="at-tok-chip c1">나는</span>
      <span class="at-tok-chip c2">·커</span>
      <span class="at-tok-chip c3">다</span>
      <span class="at-tok-chip c4">란</span>
      <span class="at-tok-chip c5">·개</span>
      <span class="at-tok-chip c1">를</span>
      <span class="at-tok-chip c2">·만</span>
      <span class="at-tok-chip c3">났다</span>
    </div>
    <div class="at-tok-row">
      <span class="at-tok-label">Korean cl100k_base (13 tokens)</span>
      <span class="at-tok-chip c1">나</span>
      <span class="at-tok-chip c2">는</span>
      <span class="at-tok-chip c-byte">·커[1/2]</span>
      <span class="at-tok-chip c-byte">커[2/2]</span>
      <span class="at-tok-chip c3">다</span>
      <span class="at-tok-chip c-byte">란[1/2]</span>
      <span class="at-tok-chip c-byte">란[2/2]</span>
      <span class="at-tok-chip c4">·개</span>
      <span class="at-tok-chip c5">를</span>
      <span class="at-tok-chip c1">·만</span>
      <span class="at-tok-chip c-byte">났[1/2]</span>
      <span class="at-tok-chip c-byte">났[2/2]</span>
      <span class="at-tok-chip c2">다</span>
    </div>
  </div>
  <figcaption>
    Figure 2. Actual <code>tiktoken</code> BPE token segmentation for the same sentence (<code>·</code> denotes a leading space). In <code>cl100k_base</code>, less common syllables (<code>커</code>, <code>란</code>, <code>났</code>) split into two UTF-8 byte fragments each (13 tokens total), whereas <code>o200k_base</code> merges common Korean words and syllables into 8 tokens. Sources: <a href="https://github.com/openai/tiktoken">openai/tiktoken</a>, <a href="https://platform.openai.com/tokenizer">OpenAI Tokenizer</a>, and <a href="https://github.com/dotnetpower/agent-tokenomics/blob/main/examples/count_tokens.py"><code>examples/count_tokens.py</code></a>.
  </figcaption>
</figure>

Three practical implications for agent builders:

1. **Shorter character count does not mean fewer tokens.** Korean, Japanese, and Chinese use fewer characters than English, but each syllable or character takes 2–3 UTF-8 bytes and often splits into multiple tokens.
2. **Match the tokenizer encoding to your model.** Newer vocabularies (`o200k_base`) cut the non-English multiplier significantly compared with `cl100k_base` (for Korean, `2.6× → 1.6×`). When profiling locally with `tokprof`, select the matching encoding (`tokprof --tokenizer o200k_base`).
3. **Keep repeated prefix content in English or BPE-friendly notation.** System prompts, standing rules, and tool descriptions are sent on every call (`tokens × calls`), so language overhead in the prefix multiplies across the entire session. See [Language and tokenization](../../copilot-tokenomics/07-language-and-tokenization/) for BPE-aligned prompt techniques.

## 5. Prompt caching

Providers can reuse the **longest matching prefix** of a request seen recently. A cache hit needs the start of
the request to be *identical*, so one early change (a reordered instruction, an edited tool list, a rewritten
history) invalidates the cache from that point on. Behavior is provider-, model-, and deployment-specific:
switching models does not reuse the old cache. Design rule: **stable content first, volatile content last,
append-only history**. For agents you build or operate, [`tokprof`](../01-diagnose/#profile-without-changing-code-tokprof)
tracks `cached_tokens` share and system-prompt hash stability across calls. Official provider guides:
[Azure OpenAI](https://learn.microsoft.com/en-us/azure/ai-foundry/openai/how-to/prompt-caching),
[OpenAI](https://developers.openai.com/api/docs/guides/prompt-caching),
[Anthropic Claude](https://platform.claude.com/docs/en/build-with-claude/prompt-caching),
[Google Gemini](https://ai.google.dev/gemini-api/docs/caching).

<figure class="at-doc-figure">
  <div class="at-diag-stack">
    <div class="at-diag-row">
      <span>Stable Prefix</span>
      <div class="at-diag-bar">
        <div class="at-diag-seg seg-hit" style="width:74%">✓ CACHE HIT: System + Tools + History (≥ 1,024 tok identical)</div>
        <div class="at-diag-seg seg-user" style="width:26%">New Turn</div>
      </div>
      <span>Discounted</span>
    </div>
    <div class="at-diag-row">
      <span>Early Change</span>
      <div class="at-diag-bar">
        <div class="at-diag-seg seg-hit" style="width:18%">Match</div>
        <div class="at-diag-seg seg-miss" style="width:82%">✗ CACHE MISS: Dynamic timestamp or reordered tools invalidates rest</div>
      </div>
      <span>Full Price</span>
    </div>
  </div>
  <figcaption>
    Figure 3. Prompt caching reuses the matching leading prefix (≥ 1,024 tokens); any early mutation (such as a dynamic timestamp or reordered tool definitions) invalidates the cache from that point onward. References: <a href="https://learn.microsoft.com/en-us/azure/ai-foundry/openai/how-to/prompt-caching">Azure OpenAI Prompt Caching</a>, <a href="https://developers.openai.com/api/docs/guides/prompt-caching">OpenAI Prompt Caching</a>, and <a href="https://arxiv.org/abs/2312.07104">SGLang RadixAttention (NeurIPS 2023)</a>.
  </figcaption>
</figure>

### Provider caching: what to verify

Caching rules differ by provider, model family, and deployment type, and they change. Before you design around
caching, get written answers to these. The right-hand column records what the official docs said on 2026-11-03
for [Azure OpenAI](https://learn.microsoft.com/en-us/azure/ai-foundry/openai/how-to/prompt-caching) and
[OpenAI](https://developers.openai.com/api/docs/guides/prompt-caching); re-verify for your model.

| Question | Why it matters | What the docs said (2026-11-03) |
|----------|----------------|---------------------------------|
| Minimum cacheable prefix? | Short prompts (guardrails, small classifiers, summarizers) may never cache | [Azure OpenAI](https://learn.microsoft.com/en-us/azure/ai-foundry/openai/how-to/prompt-caching): the request needs ≥ 1,024 tokens and the first 1,024 must be identical (increments of 128 tokens thereafter). [OpenAI](https://developers.openai.com/api/docs/guides/prompt-caching): 1,024 minimum for GPT-5.6 and later; varies for earlier models. ([Anthropic](https://platform.claude.com/docs/en/build-with-claude/prompt-caching): 1,024–4,096 tokens depending on Claude model) |
| Which models and retention policies? | A model without support, or without extended retention, changes the plan | [Azure OpenAI supported models](https://learn.microsoft.com/en-us/azure/ai-foundry/openai/how-to/prompt-caching#supported-models): in-memory retention for GPT-4o or newer; extended retention only for a listed set of models, so check the list for your exact model, including "mini" variants |
| Where do hits show up? | You cannot tune what you cannot see | [Azure chat completions](https://learn.microsoft.com/en-us/azure/ai-foundry/openai/how-to/prompt-caching): `usage.prompt_tokens_details.cached_tokens` (0 means a miss); on GPT-5.6 and later Standard deployments also `cache_write_tokens` |
| Does a cache key help? | Routing related requests together raises hit rate | [Azure / OpenAI (`prompt_cache_key`)](https://developers.openai.com/api/docs/guides/prompt-caching): on GPT-5.6 and later, reuse one `prompt_cache_key` per shared prefix; above about 15 requests per minute for one prefix and key, some requests may miss, so partition across keys with a stable mapping |
| How long is it kept? | Determines whether a quiet period wastes the cache | [Azure OpenAI retention](https://learn.microsoft.com/en-us/azure/ai-foundry/openai/how-to/prompt-caching): in-memory is typically cleared after 5–10 minutes idle, always within an hour. Extended: up to 24 hours (key/value tensors offloaded to GPU-local storage). On GPT-5.6 and later the policy field is deprecated in favor of a 30-minute minimum lifetime |
| How is it billed and counted? | Determines whether caching saves money, capacity, or both | [Azure OpenAI pricing](https://azure.microsoft.com/en-us/pricing/details/cognitive-services/openai-service/): discounted cache reads on Standard; up to a 100% input discount on Provisioned deployments; on GPT-5.6 and later, cache writes can incur charges. [OpenAI](https://developers.openai.com/api/docs/guides/prompt-caching): pricing varies by model. Ask how cached tokens count toward your capacity utilization |
| What is stored, and where? | Security and data-residency review | [OpenAI](https://developers.openai.com/api/docs/guides/prompt-caching): the cache stores key-value tensors, not raw text. [Azure OpenAI](https://learn.microsoft.com/en-us/azure/ai-foundry/openai/how-to/prompt-caching): not shared between subscriptions; extended retention stays within the regional or data-zone boundary of the deployment type |
| Can it be disabled or cleared? | Needed for some compliance cases | [Azure OpenAI](https://learn.microsoft.com/en-us/azure/ai-foundry/openai/how-to/prompt-caching): on GPT-5.6 and later Standard deployments, `explicit` mode with no breakpoints disables caching; earlier models and managed provisioned deployments cannot disable it |

Short prompts that cannot be cached are a design signal, not a problem to pad away: merge the step into a
neighbor, move it to a deterministic check, or accept that it is small. Do not add filler tokens to reach a
threshold.

## 6. Tools have two costs

1. **Definition cost** — every enabled tool's schema is in the prefix of every request, used or not.
2. **Result cost** — what a tool returns is appended to the history and re-sent on all later steps.

Pruning unused tools attacks the first; truncating, paginating, or summarizing results attacks the second.

## 7. Cost without quality is meaningless

A cheaper run that fails is not a saving. Track **cost per successful task**. Also note that per-step errors
compound across a long loop (see the table in [Prove](../03-prove/#multi-step-reliability)), so shortening a
loop can improve success and cost together.

## 8. From concepts to causes

| Concept | Waste it creates | Where you diagnose it |
|---------|------------------|-----------------------|
| Stateless re-send, big prefix | Fixed overhead; re-learned facts | [Diagnose](../01-diagnose/), [Isolate](../02-isolate/) |
| Tool definition cost | Unused tools | [Isolate](../02-isolate/) |
| Tool result cost | Bloated output | [Improve](../04-improve/) |
| Loop multiplication | Failure loops, retries | [Prove](../03-prove/) |
| History accumulation | History growth, long-context degradation | [Improve: compaction](../04-improve/#compaction-without-losing-the-cache) |
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

- [Lost in the Middle (arXiv:2307.03172)](https://arxiv.org/abs/2307.03172)

- [openai/tiktoken — fast BPE tokenizer for OpenAI models](https://github.com/openai/tiktoken) and [`examples/count_tokens.py`](https://github.com/dotnetpower/agent-tokenomics/blob/main/examples/count_tokens.py)
- [Prompt caching in Azure OpenAI (Microsoft Learn)](https://learn.microsoft.com/en-us/azure/ai-foundry/openai/how-to/prompt-caching) and [Azure OpenAI Service pricing](https://azure.microsoft.com/en-us/pricing/details/cognitive-services/openai-service/)
- [Prompt caching — OpenAI](https://developers.openai.com/api/docs/guides/prompt-caching), [Anthropic Claude](https://platform.claude.com/docs/en/build-with-claude/prompt-caching), [Google Gemini](https://ai.google.dev/gemini-api/docs/caching)
- [TokenEconomizer](https://github.com/microsoft/TokenEconomizer): describes the same carried-along costs (instructions, loaded tools, conversation so far)
