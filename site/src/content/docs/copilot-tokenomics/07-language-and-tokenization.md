---
title: Language and tokenization
description: Why the same sentence costs different token counts in different languages, and what to do about it.
track: copilot
verified: 2026-11-03
sidebar:
  order: 7
---

## Problem

"Is it cheaper to write my prompts in my native language, or to translate them?" Token counts depend on the
tokenizer, and tokenizers are trained mostly on English text. The same meaning usually needs **more tokens in
non-English languages**, so the answer is rarely "native is cheaper".

## Measure it yourself

[`examples/count_tokens.py`](https://github.com/dotnetpower/agent-tokenomics/blob/main/examples/count_tokens.py)
counts one sentence ("I met a huge dog") in several languages with [tiktoken](https://github.com/openai/tiktoken).
Results from our run (tiktoken 0.14.0, 2026-11-03):

| Language | Characters | o200k_base tokens | × English | cl100k_base tokens | × English |
|----------|-----------:|------------------:|----------:|-------------------:|----------:|
| English | 16 | 5 | 1.0 | 5 | 1.0 |
| Spanish | 24 | 7 | 1.4 | 8 | 1.6 |
| Korean | 13 | 8 | 1.6 | 13 | 2.6 |
| Chinese | 8 | 8 | 1.6 | 12 | 2.4 |
| Japanese | 9 | 8 | 1.6 | 11 | 2.2 |
| Russian | 26 | 7 | 1.4 | 14 | 2.8 |
| Hebrew | 13 | 7 | 1.4 | 16 | 3.2 |

Two lessons:

1. **Fewer characters does not mean fewer tokens.** Chinese, Japanese, and Korean use few characters, but
   each character can cost one or more tokens.
2. **The gap depends on the tokenizer.** Newer vocabularies (`o200k_base`) narrow it a lot compared with older
   ones (`cl100k_base`). Copilot can route to several providers, so run the script with an encoding close to
   your model, or use that provider's token-counting API for exact numbers.

## Actions

- Write **instructions files and standing rules in English**. They are sent on every request, so a persistent
  multiplier there matters most.
- **Do not translate a prompt just to save tokens** unless you measured it: translation can cost more than it saves
  and may change meaning.
- **For non-English workflows (e.g. Korean), keep native domain nouns and replace grammatical endings with BPE formula operators.**
  In agglutinative languages such as Korean, polite verb endings, auxiliary verbs, and particles (`~을 완료할수 있도록`, `~에 대한 계획을 세워서`, `~한 다음`) account for a large share of tokens. Keeping the core Korean domain nouns and action stems intact while replacing connectors with spaced ASCII formula operators (` ->`, ` =>`, ` >=`, ` <=`, ` &`) and a `[ko]` reply-language tag cuts input tokens by **22–39% on `o200k_base`** (**31–49% on `cl100k_base`**) without losing domain precision or Korean replies.
- Keep **code, identifiers, comments, and commit messages in English** regardless of the language you chat in.
- Non-Latin text can sometimes be transliterated to Latin script for fewer tokens, but this hurts readability;
  measure before adopting it.
- Ask the model to *reply* in the language you read best only when needed; replies are output tokens, which are
  priced higher.

### Hybrid Korean + BPE formula benchmark

Measured with [`examples/formula_prompt.py`](https://github.com/dotnetpower/agent-tokenomics/blob/main/examples/formula_prompt.py) (`tiktoken` 0.14.0, verified 2026-11-04):

| Workflow pattern | Korean prose (`o200k` / `cl100k`) | Hybrid Korean + BPE formula (`[ko] :: ...`) | `o200k_base` delta | `cl100k_base` delta |
|------------------|----------------------------------:|---------------------------------------------|-------------------:|--------------------:|
| Issue completion pipeline | 44 / 72 | `[ko] :: #1945 남은작업 계획수립 -> 모두조치 -> push & merge -> 이슈닫기` | **27 (-38.6%)** | **37 (-48.6%)** |
| Bulk gap discovery & loop | 46 / 74 | `[ko] :: 보강점 >=100 발굴 -> 모두조치 -> loop 10x -> 수치표 보고 -> commit & push & merge` | **31 (-32.6%)** | **38 (-48.6%)** |
| Bounded hardening loop | 54 / 91 | `[ko] :: 모두적용 -> loop(비평 >=10 하드닝, until 심각도 <= Low) -> 개선점 >=100 발굴 -> 모두보강 -> push & merge -> 보고` | **42 (-22.2%)** | **57 (-37.4%)** |
| Optimization audit & list | 31 / 51 | `[ko] :: 절감액 기반 최적화 >=30 발굴 -> 계획수립 -> 목록 보고` | **22 (-29.0%)** | **35 (-31.4%)** |
| **Total (4 prompts)** | **175 / 288** | — | **122 (-30.3%)** | **167 (-42.0%)** |

Why keep core Korean nouns instead of replacing everything with English shorthand? In an evaluation across 8 real Korean Copilot requests (47 constraint checkpoints), naive English keyword replacement (for example turning `"보강점 100개 이상 찾아서"` into `gaps >=100`) caused the model to misread imperative commands ("find at least 100 gaps") as conditional guards ("if gaps are at least 100"), dropping constraint accuracy to **81.5% (44/54)**. Keeping the Korean action noun (`보강점 >=100 발굴`) with BPE operators achieved **100.0% (47/47)** constraint accuracy and **100% Korean replies**.

## Trade-offs

Clarity and correctness beat a small token saving. If you think more precisely in Korean (or any language),
write that way for complex requirements and keep the *standing* files (instructions, tool descriptions) in
English.

## Further reading (papers)

Ordered by venue tier, then citations. Findings are hypotheses to verify on your own workload; see the full list in [Papers](../../papers/).

- [Neural Machine Translation of Rare Words with Subword Units](https://arxiv.org/abs/1508.07909) — ACL 2015, 9,243 citations. Introduced the subword (BPE) units that underlie most LLM tokenizers.
- [Language Model Tokenizers Introduce Unfairness Between Languages](https://arxiv.org/abs/2305.15425) — NeurIPS 2023, 319 citations. The same text tokenizes to very different lengths across languages.
- [Do All Languages Cost the Same? Tokenization in the Era of Commercial Language Models](https://arxiv.org/abs/2305.13707) — EMNLP 2023, 278 citations. Shows that tokenization differences make commercial API usage cost different amounts per language.

## Sources

- [`examples/count_tokens.py`](https://github.com/dotnetpower/agent-tokenomics/blob/main/examples/count_tokens.py) — reproducible measurement
- [`examples/formula_prompt.py`](https://github.com/dotnetpower/agent-tokenomics/blob/main/examples/formula_prompt.py) — reproducible Korean + BPE formula measurement and converter
- [tiktoken](https://github.com/openai/tiktoken)
- Token Efficiency and Context Engineering — developer talk deck (June 2026): reports the same direction (non-English
  text costs more tokens) with a different tokenizer; our table is our own measurement.
