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

Reproduced with [`examples/formula_prompt.py`](https://github.com/dotnetpower/agent-tokenomics/blob/main/examples/formula_prompt.py) (`tiktoken` 0.14.0, verified 2026-11-04):

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
- [`examples/formula_prompt.py`](https://github.com/dotnetpower/agent-tokenomics/blob/main/examples/formula_prompt.py) — reproducible BPE formula measurement and prompt converter
- [caveman](https://github.com/JuliusBrussee/caveman), [cavewoman](https://github.com/danielle34/cavewoman)
