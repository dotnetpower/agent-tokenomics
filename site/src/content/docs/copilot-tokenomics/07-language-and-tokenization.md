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
- Keep **code, identifiers, comments, and commit messages in English** regardless of the language you chat in.
- Non-Latin text can sometimes be transliterated to Latin script for fewer tokens, but this hurts readability;
  measure before adopting it.
- Ask the model to *reply* in the language you read best only when needed; replies are output tokens, which are
  priced higher.

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
- [tiktoken](https://github.com/openai/tiktoken)
- Token Efficiency and Context Engineering — developer talk deck (June 2026): reports the same direction (non-English
  text costs more tokens) with a different tokenizer; our table is our own measurement.
