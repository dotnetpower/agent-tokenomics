---
title: Prompt and output style
description: Shorter prompts, structured requests, and capped output - the levers on both sides of the request.
track: copilot
verified: 2026-11-03
sidebar:
  order: 6
---

## Problem

You pay for the tokens you send **and** the tokens the model writes. Filler in a prompt is small; filler in
every *answer* repeats on every request. Output tokens are also priced higher than input tokens on most
models, so response length is often the best lever per minute of effort. (Check your model's rates in the
GitHub Docs model pricing; the ratio differs by model.)

<figure class="at-doc-figure">
  <img src="https://code.visualstudio.com/assets/docs/agent-customization/customization/create-prompt-file-2.png" alt="VS Code prompt file creation interface" loading="lazy" />
  <figcaption>Create a reusable prompt file. Source: <a href="https://code.visualstudio.com/docs/agent-customization/prompt-files">VS Code Docs — Prompt Files</a>.</figcaption>
</figure>

## Actions

### 1. Cap the output (highest return)

Add a standing rule to `copilot-instructions.md` and/or end a prompt with a format limit:

| Instruction | Effect |
|-------------|--------|
| `Code only, no explanation.` | No prose around the change. Ask "why" explicitly when you need it. |
| `Answer in one sentence.` | Limits verbosity. |
| `3 bullet points max.` | Hard cap on items. |
| `Yes or no, then one line why.` | Shortest useful answer. |
| `Reply as JSON.` / `Table format.` | Structured output, no narrative. |

A concise *prompt* does not make the *answer* concise. Set both.

**Structured-output trade-off.** A `response_format` or strict JSON schema can simplify parsing, but forcing reasoning into a rigid format can reduce accuracy on multi-step tasks. [Let Me Speak Freely?](https://arxiv.org/abs/2408.02442) reports this effect for the models and tasks it evaluated, not every model or schema. If accuracy falls, allow brief reasoning before formatting or extract structured fields in a final step, then verify both schema validity and task success. Availability of these controls depends on the client and provider.

### 2. Trim the prompt

- Drop politeness, hedging, and filler (`please`, `I think maybe`, `basically`). Keep technical terms, code, and
  concrete details exactly.
- Prefer **structured** input (bullets, `key: value`) over prose paragraphs.
- Prefer **declarative** rules over procedures: `All exported functions: JSDoc required.` instead of a
  step-by-step "first read the file, then find…".
- Often the shortest accurate form is code-like: a type signature, pseudo-code, or "like `getUserById` but for
  emails. Return 404 if missing."
- Use common abbreviations (`DB`, `auth`, `config`, `env`, `repo`) only if the model already knows them. Define
  project-specific abbreviations in your instructions first.
- **One task per prompt.** A prompt that mixes refactor + tests + docs + security tends to do each part
  partially; split it, review each step, and redo only the step that failed.

### 3. Terse-style instructions

Community "caveman" style skills compress both input and output with a short rule such as *drop articles and
filler, fragments OK, code unchanged*. See [caveman](https://github.com/JuliusBrussee/caveman) and the
[measured evaluation](https://github.com/danielle34/cavewoman). Compression levels trade clarity for tokens:
heavier compression can make complex instructions ambiguous, so keep it light for customer-facing or
onboarding text.

### 4. BPE-aligned formula notation

When expressing routing rules, preconditions, or multi-step workflows, a **BPE-aligned ASCII formula** is shorter and less ambiguous than prose — but **Unicode math symbols (`∀`, `∈`, `∪`, `≻`, `∘`) are a trap**. In `o200k_base` and `cl100k_base`, multi-byte math symbols split into 2–3 tokens and prevent adjacent English words from merging with a leading space. Spaced ASCII operators (` ->`, ` =>`, ` >`, ` <=`, ` >=`, ` &`, ` |`) each encode as **1 token** including their leading space.

Reproduced with [`examples/formula_prompt_en.py`](https://github.com/dotnetpower/agent-tokenomics/blob/main/examples/formula_prompt_en.py) (or `python3 examples/formula_prompt.py --lang en`, `tiktoken` 0.14.0, verified 2026-11-04):

| Style | Text | Bytes | `o200k_base` | vs. Prose | `cl100k_base` |
|-------|------|------:|-------------:|----------:|--------------:|
| Prose (with backticks) | ``When you need to find a symbol or literal and read its surrounding lines, use `code_context` in one call instead of searching first and reading later.`` | 150 | 31 | 0.0% | 31 |
| Unicode math (unspaced) | `∀x∈Sym∪Lit: need(x+ctx) ⇒ code_context(x) ≻ (grep(x) ∘ read(x))` | 75 | 30 | -3.2% | 31 |
| ASCII formula (unspaced) | `δ(symbol\|literal+surrounding_lines)=>code_context(root,pattern,radius<=20)` | 75 | 20 | -35.5% | 20 |
| **BPE-aligned formula (spaced)** | `symbol + context => code_context (1 call > grep + read)` | **55** | **14** | **-54.8%** | **14** |

Rules for BPE-aligned formulas:
- Use **spaced ASCII operators** (` ->`, ` =>`, ` >=`, ` <=`, ` !=`, ` &`, ` |`), not Unicode math (`∀`, `∈`, `≻`, `∘`).
- Avoid wrapping every identifier in inline backticks (`` `name` `` adds 2 extra tokens per identifier).
- Keep spaces around `+`, `|`, and `=>` so BPE tokenizers merge `" word"` into a single token instead of splitting after punctuation.

Applied to 7 English customer-support SOP and live-chat responses from public Hugging Face datasets (`examples/formula_prompt_en.py`), BPE-aligned formulas reduce tokens by **75.9% on `o200k_base`** while preserving **43/43 (100%)** required slots and template placeholders (for Korean counseling and chat examples in `examples/formula_prompt_ko.py`, see [Language and tokenization](../07-language-and-tokenization/#hugging-face-customer-support-and-counseling-benchmark)):

| English SOP / chat case & Hugging Face dataset (`row`) | Prose (`o200k` / `cl100k`) | BPE formula (`o200k` / `cl100k`) | `o200k_base` delta | `cl100k_base` delta | Slots kept |
|--------------------------------------------------------|---------------------------:|---------------------------------:|-------------------:|--------------------:|-----------:|
| Order cancel SOP ([`bitext/Bitext-customer-support-llm-chatbot-training-dataset`](https://huggingface.co/datasets/bitext/Bitext-customer-support-llm-chatbot-training-dataset) `train[2]`) | 223 / 226 | 55 / 55 | **-75.3%** | **-75.7%** | 9/9 |
| Order change SOP ([`bitext/Bitext-customer-support-llm-chatbot-training-dataset`](https://huggingface.co/datasets/bitext/Bitext-customer-support-llm-chatbot-training-dataset) `train[1001]`) | 225 / 230 | 54 / 54 | **-76.0%** | **-76.5%** | 9/9 |
| Cancellation fee check SOP ([`bitext/Bitext-customer-support-llm-chatbot-training-dataset`](https://huggingface.co/datasets/bitext/Bitext-customer-support-llm-chatbot-training-dataset) `train[3001]`) | 98 / 100 | 26 / 27 | **-73.5%** | **-73.0%** | 5/5 |
| Refund timeframe & lookup ([`bitext/Bitext-customer-support-llm-chatbot-training-dataset`](https://huggingface.co/datasets/bitext/Bitext-customer-support-llm-chatbot-training-dataset) `train[6000]`) | 221 / 225 | 34 / 34 | **-84.6%** | **-84.9%** | 5/5 |
| Recall return & refund SOP ([`NebulaByte/E-Commerce_Customer_Support_Conversations`](https://huggingface.co/datasets/NebulaByte/E-Commerce_Customer_Support_Conversations) `train[1]`) | 131 / 131 | 42 / 42 | **-67.9%** | **-67.9%** | 6/6 |
| Robotic arm recalibration chat ([`FunDialogues/customer-service-robot-support`](https://huggingface.co/datasets/FunDialogues/customer-service-robot-support) `train[0]`) | 60 / 61 | 18 / 18 | **-70.0%** | **-70.5%** | 5/5 |
| Delayed order live-chat reply ([`Kaludi/Customer-Support-Responses`](https://huggingface.co/datasets/Kaludi/Customer-Support-Responses) `train[0]`) | 28 / 29 | 9 / 9 | **-67.9%** | **-69.0%** | 4/4 |
| **Total (7 English SOP & live-chat cases)** | **986 / 1,002** | **238 / 239** | **-75.9%** | **-76.1%** | **43/43 (100%)** |

## How to verify

Measure, do not assume. With the [token counter](../07-language-and-tokenization/) on the `o200k_base`
encoding, one example prompt falls from 38 tokens to 11:

```text
Hey, could you please help me refactor this function? I think it might have some issues with how it handles
the authentication, and I'd really like it to be more efficient. Thanks!          -> 38 tokens
Refactor function. Fix auth handling. Make efficient.                              -> 11 tokens
```

Then compare **session credits** for a real task before and after, and check the output still meets your
acceptance criteria.

## Trade-offs

- Learning or debugging needs explanations: ask for them on demand (`Explain why this beats X`).
- Over-compressed prompts lose constraints. Keep exact technical terms and concrete details.
- The talk deck this page draws from reports large output savings for these instructions (tens of percent,
  higher for yes/no style). We could not reproduce those figures independently; treat them as directional and
  measure on your tasks.

## Further reading (papers)

Ordered by venue tier, then citations. Findings are hypotheses to verify on your own workload; see the full list in [Papers](../../papers/).

- [LongLLMLingua: Accelerating and Enhancing LLMs in Long Context Scenarios via Prompt Compression](https://arxiv.org/abs/2310.06839) — ACL 2023, 658 citations. Question-aware compression for long contexts, aiming to cut cost while keeping or improving accuracy.
- [LLMLingua-2: Data Distillation for Efficient and Faithful Task-Agnostic Prompt Compression](https://arxiv.org/abs/2403.12968) — ACL 2024, 487 citations. Distills a faster, task-agnostic prompt compressor that generalizes across tasks.
- [LLMLingua: Compressing Prompts for Accelerated Inference of Large Language Models](https://arxiv.org/abs/2310.05736) — EMNLP 2023, 418 citations. Uses a small language model to drop low-information tokens from a prompt, with a controllable budget.
- [Compressing Context to Enhance Inference Efficiency of Large Language Models](https://arxiv.org/abs/2310.06201) — EMNLP 2023, 272 citations. Prunes redundant lexical units by self-information to shorten inputs before inference.
- [Let Me Speak Freely? A Study On The Impact Of Format Restrictions On Large Language Model Performance.](https://arxiv.org/abs/2408.02442) — EMNLP 2024, 142 citations. Strict output-format restrictions can lower reasoning performance.
- [Chain of Draft: Thinking Faster by Writing Less](https://arxiv.org/abs/2502.18600) — preprint 2025, 324 citations. Prompts the model to write minimal draft steps instead of verbose chain-of-thought.

## Sources

- Token Efficiency and Context Engineering — developer talk deck (June 2026), prompt-compression and
  output-control sections (figures as reported, not independently verified)
- [Optimize AI credit usage in VS Code](https://code.visualstudio.com/docs/agents/guides/optimize-usage)
- [`examples/formula_prompt_en.py`](https://github.com/dotnetpower/agent-tokenomics/blob/main/examples/formula_prompt_en.py), [`examples/formula_prompt.py`](https://github.com/dotnetpower/agent-tokenomics/blob/main/examples/formula_prompt.py) — reproducible English BPE formula measurement and prompt converter
- [caveman](https://github.com/JuliusBrussee/caveman), [cavewoman](https://github.com/danielle34/cavewoman)
