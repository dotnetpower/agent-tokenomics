---
created: 2026-11-03
updated: 2026-11-19
dateSource: git
title: Language and tokenization
description: Why the same sentence costs different token counts in different languages, and what to do about it.
track: copilot
verified: 2026-11-19
sidebar:
  order: 7
---

> **Scope: public documentation reviewed on 2026-11-19.** Prices, policies, UI, and settings can change. Check the original source for your client/version/harness. This date does not claim a new installation, billing, or model-quality experiment.

## Problem

Language costs depend on the actual model tokenizer and task. A count from one sample or proxy encoding is not a universal language multiplier.

## Measure it yourself

Use the existing local examples:

```bash
python3 examples/count_tokens.py
python3 examples/formula_prompt.py --lang ko
```

Record script revision, tokenizer package/version, encoding, exact input, and counts. tiktoken encodings do not reproduce every model offered by Copilot. Prefer reported model usage or a supported provider counter when available; do not call an approximate count exact.

## Actions

- Write complex requirements in the language that preserves their meaning. Test English standing rules rather than making English mandatory.
- Keep code identifiers and canonical product/setting names unchanged. Follow repository localization conventions for comments and documentation.
- Include translation effort, retries, answer quality, and actual credits in any language-switch comparison.
- Do not infer hidden reasoning or billing savings from a shorter visible prompt.

<a id="hybrid-korean-bpe-formula-benchmark"></a>
### Hybrid Korean + BPE formula benchmark

The scripts demonstrate text compression on fixed inputs. Earlier prose mixed a 47-checkpoint evaluation with a 44/54 result without a comparable evaluation record. Those model-accuracy percentages are removed: do not infer a corrected denominator.

Keep the Korean action, negation, quantity, exceptions, and response-language requirement. Compare both texts against the same rubric and actual completed task.

### Hugging Face customer-support and counseling benchmark

The [Korean script](https://github.com/dotnetpower/agent-tokenomics/blob/main/examples/formula_prompt_ko.py) and [tests](https://github.com/dotnetpower/agent-tokenomics/blob/main/examples/test_formula_prompt.py) check recognized slots and token reduction. Slots may be restored or the original text retained; this is **not a model comprehension test**. Record dataset/revision/rows for reproduction and do not generalize slot retention into semantic accuracy. No new model-quality or live-dataset benchmark is claimed in this revision.

## How to verify

Hold task, model/effort, acceptance checks, and cache conditions comparable. Compare native-language, English, and concise native-language prompts in [the baseline record](../02-measure-usage/). Preserve failures and use **not exposed** for unavailable usage.

## Trade-offs

An English prompt that causes a misunderstood requirement can be more expensive than clear Korean. Token efficiency does not override clarity, accessibility, or repository language conventions.

## Further reading (papers)

Ordered by venue tier, then citations. Findings are hypotheses to verify on your own workload; see the full list in [Papers](../../papers/).

- [Neural Machine Translation of Rare Words with Subword Units](https://arxiv.org/abs/1508.07909) - ACL 2015, 9,243 citations. Introduced the subword (BPE) units that underlie most LLM tokenizers.
- [Language Model Tokenizers Introduce Unfairness Between Languages](https://arxiv.org/abs/2305.15425) - NeurIPS 2023, 319 citations. The same text tokenizes to very different lengths across languages.
- [Do All Languages Cost the Same? Tokenization in the Era of Commercial Language Models](https://arxiv.org/abs/2305.13707) - EMNLP 2023, 278 citations. Shows that tokenization differences make commercial API usage cost different amounts per language.

## Sources

- [code.visualstudio.com: harness](https://code.visualstudio.com/blogs/2026/11/18/copilot-harness)
- [github.com: tiktoken](https://github.com/openai/tiktoken)
- [code.visualstudio.com: models](https://code.visualstudio.com/docs/agent-customization/language-models)
- [docs.github.com: rates](https://docs.github.com/en/copilot/reference/copilot-billing/models-and-pricing)
