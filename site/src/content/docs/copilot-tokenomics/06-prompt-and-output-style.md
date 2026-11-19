---
created: 2026-11-03
updated: 2026-11-19
dateSource: git
title: Prompt and output style
description: Shorter prompts, structured requests, and capped output - the levers on both sides of the request.
track: copilot
verified: 2026-11-19
sidebar:
  order: 6
---

> **Scope: public documentation reviewed on 2026-11-19.** Prices, policies, UI, and settings can change. Check the original source for your client/version/harness. This date does not claim a new installation, billing, or model-quality experiment.

## Problem

Shorter visible answers do not prove lower hidden-reasoning usage or lower total session cost. Preserve requirements, verification, and failure reporting while removing unnecessary narration.

## Actions

### 1. Cap the output (highest return)

Use a task-appropriate format, for example: "Return the patch summary, commands actually run, test counts, and unresolved blockers in at most five bullets." This is a prompt instruction, not a hard API token limit.

"Code only" can fit a small transformation but should not suppress test failures or safety warnings. JSON in a chat prompt is not a guaranteed provider-enforced schema. [Output and cache rates differ by model](https://docs.github.com/en/copilot/reference/copilot-billing/models-and-pricing); [reasoning controls](https://code.visualstudio.com/docs/agent-customization/language-models) are separate from prose length.

### 2. Trim the prompt

State the goal, authoritative inputs, constraints, and acceptance checks. Keep implementation, its tests, and directly related documentation in one coherent task when they share the same outcome. Split unrelated goals, not the conditions needed for completion.

### 3. Terse-style instructions

Prefer clear bullets over invented shorthand. Shorter wording can omit negation, exceptions, or dependencies. Evaluate interpretation, not just string length.

### 4. BPE-aligned formula notation

The local [formula measurement script](https://github.com/dotnetpower/agent-tokenomics/blob/main/examples/formula_prompt_en.py) compares prose and formula representations using specified tiktoken encodings:

```bash
python3 examples/formula_prompt.py --lang en
```

Treat its token counts as **encoding-specific text measurements**. ASCII operators are not always one token in every position or model. Retained strings/slots do not demonstrate semantic equivalence or successful agent behavior. Do not turn these measurements into a promised Copilot credit-saving percentage.

## How to verify

Run the same task with the original and concise prompt. Preserve all acceptance conditions; compare visible output, reported inclusive output/reasoning where available, retries, credits, and tests. Use [the baseline template](../02-measure-usage/).

## Trade-offs

Explanations are useful for learning and review. Restrict unnecessary narration, not evidence. Previous unlinked talk-deck savings figures are not used as support; encoding experiments do not establish end-to-end quality.

## Further reading (papers)

Ordered by venue tier, then citations. Findings are hypotheses to verify on your own workload; see the full list in [Papers](../../papers/).

- [LongLLMLingua: Accelerating and Enhancing LLMs in Long Context Scenarios via Prompt Compression](https://arxiv.org/abs/2310.06839) - ACL 2023, 658 citations. Question-aware compression for long contexts, aiming to cut cost while keeping or improving accuracy.
- [LLMLingua-2: Data Distillation for Efficient and Faithful Task-Agnostic Prompt Compression](https://arxiv.org/abs/2403.12968) - ACL 2024, 487 citations. Distills a faster, task-agnostic prompt compressor that generalizes across tasks.
- [LLMLingua: Compressing Prompts for Accelerated Inference of Large Language Models](https://arxiv.org/abs/2310.05736) - EMNLP 2023, 418 citations. Uses a small language model to drop low-information tokens from a prompt, with a controllable budget.
- [Compressing Context to Enhance Inference Efficiency of Large Language Models](https://arxiv.org/abs/2310.06201) - EMNLP 2023, 272 citations. Prunes redundant lexical units by self-information to shorten inputs before inference.
- [Let Me Speak Freely? A Study On The Impact Of Format Restrictions On Large Language Model Performance.](https://arxiv.org/abs/2408.02442) - EMNLP 2024, 142 citations. Strict output-format restrictions can lower reasoning performance.
- [Chain of Draft: Thinking Faster by Writing Less](https://arxiv.org/abs/2502.18600) - preprint 2025, 324 citations. Prompts the model to write minimal draft steps instead of verbose chain-of-thought.

## Sources

- [code.visualstudio.com: harness](https://code.visualstudio.com/blogs/2026/11/18/copilot-harness)
- [code.visualstudio.com: optimize](https://code.visualstudio.com/docs/agents/guides/optimize-usage)
- [docs.github.com: rates](https://docs.github.com/en/copilot/reference/copilot-billing/models-and-pricing)
- [code.visualstudio.com: models](https://code.visualstudio.com/docs/agent-customization/language-models)
- [github.com: tiktoken](https://github.com/openai/tiktoken)
