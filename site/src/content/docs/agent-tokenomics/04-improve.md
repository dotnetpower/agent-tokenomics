---
title: "4. Improve"
description: Apply one targeted fix mapped to the proven cause.
track: agent
stage: improve
verified: 2026-11-03
sidebar:
  order: 4
---

**Goal:** the smallest change that removes the proven waste. One fix per experiment.

## Fix catalog

| Proven cause | Fix | Notes |
|--------------|-----|-------|
| Re-learning repo facts | Put short, confirmed repo facts at the start of the session (instructions file or injected "micro-skills"): correct test command, required env var, where generated files live | Keep facts few and verified; drop a fact when later sessions contradict it |
| Unused tools | **Prune** tool definitions; load tools on demand; give sub-agents narrow tool sets | Keep a one-step way to re-enable a pruned tool |
| Failure loops | Detect repeated identical errors in a turn; inject **one** redirect ("try a different approach"); if still stuck, tell the user how to stop | Never silently kill a turn; cap redirects per turn |
| Bloated tool output | Truncate or paginate results; search before reading whole files; return summaries; run bulk loops in a script and return only the result | Make truncation visible so the agent can ask for more |
| History growth | Compact/summarize older turns; start fresh sessions per task; isolate exploration in sub-agents | Summaries are lossy — see [Compaction without losing the cache](#compaction-without-losing-the-cache) |
| Prefix churn | Put stable content first (system prompt, instructions, tool definitions), volatile content last; do not edit the prefix mid-session | Check with a cache-diff view |
| Over-large model / effort | Route easy steps to a smaller model; lower reasoning effort by default | Gate with the stage-3 quality check |
| Retries and rework | Better success criteria in the prompt; validate cheaply before expensive steps | Often also improves quality |
| Context rot | Put critical rules at the start *and* restate key constraints near the end; compact or restart before the window is nearly full | Models attend less reliably to the middle of long contexts ([Lost in the Middle](https://arxiv.org/abs/2307.03172)) |
| Tool-count × step-count multiplier | Per-workspace tool sets; audit servers by usage; cap steps (`maxTurns`-style limit) and define a done-criterion | See [MCP audit](../../copilot-tokenomics/09-mcp-audit/) |
| Over-broad prompt / instructions | Keep only "landmine" facts; declarative rules; one task per prompt; a plan file for complex work | Context files often add cost without raising success ([arXiv:2602.11988](https://arxiv.org/abs/2602.11988)) |
| Long, stable knowledge in every prompt | Package as an on-demand **skill** instead of always-on text | Keep one source of truth |
| Verbose model output | Standing output limits ("code only", fixed formats); structured output | Output tokens usually cost more than input |
| Large static documents × call count | Attach a reference document only when the question needs it (rule or retrieval); split into sections and include the relevant ones; load on demand through a tool or skill; attach it once rather than on every call of a multi-call step | Gate the condition with the component-level eval from stage 3; measure `tokens × calls` |
| Redundant pipeline steps (fan-out) | Skip a step with a deterministic check; merge neighboring classification steps into one call; use a smaller model for guardrails and routing; drop steps whose output is rarely used | Each removed step must pass the stage-3 eval; keep the old path behind a flag until proven |
| Unbounded per-session state | Cap or window internal messages; keep only what later steps need (a short result, not the full report); summarize by the compaction rules below | Search for the missing truncation first: it is often a one-line fix |
| Short, uncacheable prompts | Merge the step into a neighbor, replace it with a rule, or accept it (it is small); never pad to reach the minimum | Check the provider's current minimum and `cached_tokens` |
| Bulk / polling tool loops | Run the loop in a script and return only the summary | Benchmark scripted vs interactive |

## Compaction without losing the cache

Compaction and prompt caching pull in opposite directions: rewriting the *beginning* of the history changes the
request prefix, so the provider can no longer reuse its cache and you pay full input price again.

<figure class="at-doc-figure">
<div class="at-diag-stack">
<div class="at-diag-row"><span>Stable prefix</span><div class="at-diag-bar"><span class="at-diag-seg seg-prefix" style="width: 40%">Stable prefix</span><span class="at-diag-seg seg-hist" style="width: 30%">Summary</span><span class="at-diag-seg seg-user" style="width: 30%">Recent turns</span></div><span>CACHE HIT</span></div>
<div class="at-diag-row"><span>Full rewrite</span><div class="at-diag-bar"><span class="at-diag-seg seg-miss" style="width: 100%">Rewritten prefix</span></div><span>CACHE MISS</span></div>
</div>
<figcaption>Conceptual comparison: placing a summary and recent turns after a stable prefix can preserve prefix cache reuse; rewriting the prefix invalidates the old match. Actual hits depend on provider minimum length, retention, and other cache conditions.</figcaption>
</figure>

- **Keep the prefix stable.** System prompt, instructions, and tool definitions first and unchanged; keep the
  history append-only between compactions.
- **Compact rarely, in one batch,** when a threshold is reached (for example, a fraction of the context window)
  or at a task boundary — not every turn.
- **Break-even check.** Compact only if
  `saved tokens × remaining turns × (1 − cached-input discount)` exceeds
  `summarization call cost + cache rebuild cost`. Short remaining sessions rarely pay back.
- **Structure the summary** by what the agent needs, not by chronology:

```text
Goal:            <one line>
Decisions:       <what was decided and why, briefly>
Constraints:     <rules, versions, "do not touch X">
Done / artifacts:<files changed, commands that worked, with paths>
Open items:      <unresolved questions, next step>
Failures to avoid: <error signatures already seen and what fixed them>
```

  A few lines of natural language can carry nuance the fields cannot; use the structure as the core and add
  prose only for what does not fit.
- **Summarize in the background, off the critical path:** trigger when the context passes a token threshold, run
  asynchronously after the answer, and use the result from the next turn on.
- **Mix summary and raw tail:** send the summary plus the last few raw turns, and cap the summary length. The
  summarizer's own prompt is usually short (often below the cacheable minimum), so its cost is mostly
  uncached input; make sure the savings on later calls outweigh it (break-even above).
- **Domain owners decide what must survive** (for example, the facts a human advisor would never want lost);
  engineers implement and test it. Review the list when the domain changes.
- **Offload instead of summarizing** when you can: save long raw output to a file and keep a one-line summary
  plus the path, so the agent can re-read it on demand (near-lossless).
- **Guard the summary:** do not summarize a summary repeatedly (errors compound); keep the original; redact
  secrets and personal data before they enter a summary.
- Validate with the [lossy-fix criteria](../03-prove/#when-the-fix-is-lossy-summaries-compaction).

### Worked example: multi-turn counselor dialogue compaction

In customer-support and counseling agents, older dialogue turns repeat greetings, apologies, raw `<tool_call>` JSON wrappers, duplicate session UUIDs, and closing boilerplate. Compacting completed episodes into a BPE-aligned chain (`intent -> tool(args) => outcome | ...`) cuts multi-turn history tokens while retaining **100%** of customer IDs, order IDs, dates, prices, tracking numbers, and escalation outcomes. English examples live in [`examples/formula_prompt_en.py`](https://github.com/dotnetpower/agent-tokenomics/blob/main/examples/formula_prompt_en.py) and Korean examples live in [`examples/formula_prompt_ko.py`](https://github.com/dotnetpower/agent-tokenomics/blob/main/examples/formula_prompt_ko.py) (unified runner: [`examples/formula_prompt.py`](https://github.com/dotnetpower/agent-tokenomics/blob/main/examples/formula_prompt.py) `--lang {en,ko,all}`, verified 2026-11-04):

**English multi-turn chat & support dialogues (`examples/formula_prompt_en.py`):**

| English dialogue & Hugging Face dataset (`row`) | Raw chat (`o200k` / `cl100k`) | Compacted BPE chain (`o200k` / `cl100k`) | `o200k_base` delta | `cl100k_base` delta | Slots kept |
|-------------------------------------------------|------------------------------:|-----------------------------------------:|-------------------:|--------------------:|-----------:|
| Login OTP fallback call (13 turns, [`NebulaByte/E-Commerce_Customer_Support_Conversations`](https://huggingface.co/datasets/NebulaByte/E-Commerce_Customer_Support_Conversations) `train[0]`) | 336 / 350 | 57 / 58 | **-83.0%** | **-83.4%** | 7/7 |
| Recalled monitor return & refund (13 turns, [`NebulaByte/E-Commerce_Customer_Support_Conversations`](https://huggingface.co/datasets/NebulaByte/E-Commerce_Customer_Support_Conversations) `train[1]`) | 339 / 347 | 53 / 53 | **-84.4%** | **-84.7%** | 8/8 |
| Warranty replacement & Cancel glitch (19 turns, [`NebulaByte/E-Commerce_Customer_Support_Conversations`](https://huggingface.co/datasets/NebulaByte/E-Commerce_Customer_Support_Conversations) `train[2]`) | 526 / 535 | 81 / 81 | **-84.6%** | **-84.9%** | 9/9 |
| OTP lockout & password reset (17 turns, [`NebulaByte/E-Commerce_Customer_Support_Conversations`](https://huggingface.co/datasets/NebulaByte/E-Commerce_Customer_Support_Conversations) `train[3]`) | 460 / 471 | 49 / 49 | **-89.3%** | **-89.6%** | 6/6 |
| Medical intake palpitations chat (10 turns, [`nwirandx/Korean-OSCE-history-taking-dialogues`](https://huggingface.co/datasets/nwirandx/Korean-OSCE-history-taking-dialogues) `sessions[0..9]`) | 321 / 322 | 74 / 73 | **-76.9%** | **-77.3%** | 9/9 |
| Production line robot support chat (4 turns, [`FunDialogues/customer-service-robot-support`](https://huggingface.co/datasets/FunDialogues/customer-service-robot-support) `train[1,2]`) | 112 / 114 | 35 / 36 | **-68.8%** | **-68.4%** | 6/6 |
| **Total (6 English multi-turn chat dialogues)** | **2,094 / 2,139** | **349 / 350** | **-83.3%** | **-83.6%** | **45/45 (100%)** |

**Korean multi-turn chat & counseling dialogues (`examples/formula_prompt_ko.py`):**

| Korean dialogue & Hugging Face dataset (`row`) | Raw chat (`o200k` / `cl100k`) | Compacted BPE chain (`o200k` / `cl100k`) | `o200k_base` delta | `cl100k_base` delta | Slots kept |
|------------------------------------------------|------------------------------:|-----------------------------------------:|-------------------:|--------------------:|-----------:|
| Address + shipping + spam report (14 turns, [`iamjoon/ecommerce-function-calling-datasets-korean`](https://huggingface.co/datasets/iamjoon/ecommerce-function-calling-datasets-korean) `train[0]`) | 304 / 460 | 84 / 129 | **-72.4%** | **-72.0%** | 9/9 |
| Cart + laptop defect + receipt (15 turns, [`iamjoon/ecommerce-function-calling-datasets-korean`](https://huggingface.co/datasets/iamjoon/ecommerce-function-calling-datasets-korean) `train[2]`) | 386 / 565 | 127 / 190 | **-67.1%** | **-66.4%** | 9/9 |
| Shipping + Gold tier + refund/cancel (16 turns, [`iamjoon/ecommerce-function-calling-datasets-korean`](https://huggingface.co/datasets/iamjoon/ecommerce-function-calling-datasets-korean) `train[5]`) | 436 / 638 | 158 / 225 | **-63.8%** | **-64.7%** | 11/11 |
| Return/exchange + refusal + orders (13 turns, [`iamjoon/ecommerce-function-calling-datasets-korean`](https://huggingface.co/datasets/iamjoon/ecommerce-function-calling-datasets-korean) `train[6]`) | 368 / 545 | 124 / 194 | **-66.3%** | **-64.4%** | 11/11 |
| Parcel lookup & locker redirect chat (6 turns, [`schneiderkamplab/dfm14-ko-synthetic-tool-dialogue-r1`](https://huggingface.co/datasets/schneiderkamplab/dfm14-ko-synthetic-tool-dialogue-r1) `train[2]`) | 195 / 226 | 55 / 61 | **-71.8%** | **-73.0%** | 5/5 |
| Medical intake palpitations chat (10 turns, [`nwirandx/Korean-OSCE-history-taking-dialogues`](https://huggingface.co/datasets/nwirandx/Korean-OSCE-history-taking-dialogues) `sessions[0..9]`) | 402 / 613 | 102 / 144 | **-74.6%** | **-76.5%** | 10/10 |
| Multi-session persona & family chat (18 turns, [`nayohan/141_korean_multi_session_dialogue`](https://huggingface.co/datasets/nayohan/141_korean_multi_session_dialogue) `train[0]`) | 359 / 567 | 77 / 103 | **-78.6%** | **-81.8%** | 11/11 |
| **Total (7 Korean multi-turn chat dialogues)** | **2,450 / 3,614** | **727 / 1,046** | **-70.3%** | **-71.1%** | **66/66 (100%)** |

Across **580 live chat, customer-support, and counseling sessions** (250 English rows across 5 datasets + 330 Korean rows across 7 datasets) fetched from public Hugging Face datasets (`python3 examples/formula_prompt.py --hf-live --hf-rows 50`), generalized BPE grammar and discourse compression reduces total tokens by **56.3% on `o200k_base`** (**54.9% on `cl100k_base`**) while preserving **1,731 / 1,731 (100.0%)** strict slots (statute numbers, court case IDs, emails, UUIDs, kebab-case IDs, order/user IDs, dates, amounts, and placeholders):

| Language & Hugging Face dataset (`--hf-live --hf-rows 50`) | Rows | `o200k_base` (Raw → BPE) | `cl100k_base` (Raw → BPE) | Strict slots kept |
|------------------------------------------------------------|-----:|-------------------------:|--------------------------:|------------------:|
| EN: [`NebulaByte/E-Commerce_Customer_Support_Conversations`](https://huggingface.co/datasets/NebulaByte/E-Commerce_Customer_Support_Conversations) | 50 | 23,979 → 11,538 (**-51.9%**) | 24,629 → 11,912 (**-51.6%**) | 124/124 |
| EN: [`bitext/Bitext-customer-support-llm-chatbot-training-dataset`](https://huggingface.co/datasets/bitext/Bitext-customer-support-llm-chatbot-training-dataset) | 50 | 11,431 → 3,390 (**-70.3%**) | 11,639 → 3,406 (**-70.7%**) | 281/281 |
| EN: [`FunDialogues/customer-service-robot-support`](https://huggingface.co/datasets/FunDialogues/customer-service-robot-support) | 50 | 3,146 → 1,548 (**-50.8%**) | 3,226 → 1,590 (**-50.7%**) | 0/0 (100%) |
| EN: [`Kaludi/Customer-Support-Responses`](https://huggingface.co/datasets/Kaludi/Customer-Support-Responses) | 50 | 1,837 → 893 (**-51.4%**) | 1,883 → 919 (**-51.2%**) | 1/1 |
| EN: [`nwirandx/Korean-OSCE-history-taking-dialogues`](https://huggingface.co/datasets/nwirandx/Korean-OSCE-history-taking-dialogues) (`sessions`, EN) | 50 | 1,462 → 608 (**-58.4%**) | 1,467 → 610 (**-58.4%**) | 11/11 |
| **English subtotal (5 datasets)** | **250** | **41,855 → 17,977 (-57.0%)** | **42,844 → 18,437 (-57.0%)** | **417/417 (100%)** |
| KO: [`iamjoon/ecommerce-function-calling-datasets-korean`](https://huggingface.co/datasets/iamjoon/ecommerce-function-calling-datasets-korean) | 50 | 19,243 → 7,774 (**-59.6%**) | 29,172 → 12,468 (**-57.3%**) | 220/220 |
| KO: [`jjun123/delivery-app-function-calling-datasets-korean`](https://huggingface.co/datasets/jjun123/delivery-app-function-calling-datasets-korean) | 30 | 42,129 → 18,182 (**-56.8%**) | 52,592 → 24,433 (**-53.5%**) | 325/325 |
| KO: [`jihye-moon/klac_legal_aid_counseling`](https://huggingface.co/datasets/jihye-moon/klac_legal_aid_counseling) | 50 | 27,325 → 12,357 (**-54.8%**) | 42,753 → 18,872 (**-55.9%**) | 465/465 |
| KO: [`seventy4/labor-counsel-qa`](https://huggingface.co/datasets/seventy4/labor-counsel-qa) | 50 | 5,788 → 2,498 (**-56.8%**) | 9,588 → 4,041 (**-57.9%**) | 38/38 |
| KO: [`schneiderkamplab/dfm14-ko-synthetic-tool-dialogue-r1`](https://huggingface.co/datasets/schneiderkamplab/dfm14-ko-synthetic-tool-dialogue-r1) | 50 | 7,437 → 3,119 (**-58.1%**) | 9,405 → 4,478 (**-52.4%**) | 176/176 |
| KO: [`nwirandx/Korean-OSCE-history-taking-dialogues`](https://huggingface.co/datasets/nwirandx/Korean-OSCE-history-taking-dialogues) (`sessions`, KO) | 50 | 1,854 → 882 (**-52.4%**) | 2,791 → 1,304 (**-53.3%**) | 13/13 |
| KO: [`nayohan/141_korean_multi_session_dialogue`](https://huggingface.co/datasets/nayohan/141_korean_multi_session_dialogue) | 50 | 18,307 → 8,914 (**-51.3%**) | 28,868 → 14,252 (**-50.6%**) | 77/77 |
| **Korean subtotal (7 datasets)** | **330** | **122,083 → 53,726 (-56.0%)** | **175,169 → 79,848 (-54.4%)** | **1,314/1,314 (100%)** |
| **Combined total (12 dataset benchmarks, 580 chat & counseling cases)** | **580** | **163,938 → 71,703 (-56.3%)** | **218,013 → 98,285 (-54.9%)** | **1,731/1,731 (100%)** |

## Rules

- **One fix at a time.** Combined changes make results unattributable.
- Define the **revert condition** in advance (e.g., success drops by more than your noise threshold).
- Prefer fixes that are **reversible and visible** (settings/config) over silent behavior changes.
- Black-box first: if you cannot modify the agent itself, act through its configuration, instructions,
  and tool settings.

## Evidence by cause

Published work behind each fix, ordered by venue tier then citations within a row (full list: [Papers](../../papers/)). Treat each as a hypothesis for *your* agent: prove it with [stage 3](../03-prove/).

| Cause | Papers |
|-------|--------|
| Re-learned repo facts | [Agentic Context Engineering](https://arxiv.org/abs/2510.04618) (ICLR 2025); [Agent Workflow Memory](https://arxiv.org/abs/2409.07429) (ICML 2024); [TokenEconomizer](https://arxiv.org/abs/2610.22732) (preprint 2026) |
| Unused tools / tool schema size | [MCP-Zero](https://arxiv.org/abs/2506.01056) (preprint 2025); [RAG-MCP](https://arxiv.org/abs/2505.03275) (preprint 2025); [Less is More (function calling)](https://arxiv.org/abs/2411.15399) (preprint 2024) |
| Failure loops, agent design | [SWE-agent](https://arxiv.org/abs/2405.15793) (NeurIPS 2024); [Agentless](https://arxiv.org/abs/2407.01489) (preprint 2024); [Efficient Agents](https://arxiv.org/abs/2508.02694) (preprint 2025) |
| Bloated tool output / trajectory | [SWE-agent](https://arxiv.org/abs/2405.15793) (NeurIPS 2024); [Trajectory Reduction](https://arxiv.org/abs/2509.23586) (PACMSE 2025); [The Complexity Trap (observation masking)](https://arxiv.org/abs/2508.21433) (preprint 2025) |
| History growth / memory | [Mem0](https://arxiv.org/abs/2504.19413) (ECAI 2025); [Recursively Summarizing](https://arxiv.org/abs/2308.15022) (Neurocomputing 2023); [MemGPT](https://arxiv.org/abs/2310.08560) (preprint 2023); [The Complexity Trap (observation masking)](https://arxiv.org/abs/2508.21433) (preprint 2025) |
| Context rot | [Lost in the Middle](https://arxiv.org/abs/2307.03172) (TACL 2023); [LLMs Get Lost in Multi-Turn Conversation](https://arxiv.org/abs/2505.06120) (ICLR 2025); [Same Task, More Tokens](https://arxiv.org/abs/2402.14848) (ACL 2024) |
| Prefix churn / cache misses | [SGLang](https://arxiv.org/abs/2312.07104) (NeurIPS 2023); [Prompt Cache](https://arxiv.org/abs/2311.04934) (MLSys 2023); [Don't Break the Cache](https://arxiv.org/abs/2601.06007) (preprint 2026) |
| Prompt and context compression | [LongLLMLingua](https://arxiv.org/abs/2310.06839) (ACL 2023); [LLMLingua-2](https://arxiv.org/abs/2403.12968) (ACL 2024); [LLMLingua](https://arxiv.org/abs/2310.05736) (EMNLP 2023) |
| Over-large model | [FrugalGPT](https://arxiv.org/abs/2305.05176) (TMLR 2023); [Hybrid LLM](https://arxiv.org/abs/2404.14618) (ICLR 2024); [RouteLLM](https://arxiv.org/abs/2406.18665) (preprint 2024) |
| Reasoning length / effort | [Token-Budget-Aware Reasoning](https://arxiv.org/abs/2412.18547) (ACL 2024); [L1 (length-controlled reasoning)](https://arxiv.org/abs/2503.04697) (preprint 2025); [Chain of Draft](https://arxiv.org/abs/2502.18600) (preprint 2025) |
| Redundant pipeline steps / unneeded model size | [FrugalGPT](https://arxiv.org/abs/2305.05176) (TMLR 2023); [AutoMix](https://arxiv.org/abs/2310.12963) (NeurIPS 2023); [Agentless](https://arxiv.org/abs/2407.01489) (preprint 2024) |
| Bulk calls | [Batch Prompting](https://arxiv.org/abs/2301.08721) (EMNLP 2023) |

## Next

Re-measure in [5. Result](../05-result/).
