---
title: "5. Result"
description: Re-measure against the baseline and decide keep or revert.
track: agent
stage: result
verified: 2026-11-03
sidebar:
  order: 5
---

**Goal:** an honest before/after comparison and a recorded decision.

## Checks

Run the *same* task set, repetitions, and environment as the baseline, now with the fix.

| Metric | Baseline | With fix | Change | Acceptable? |
|--------|----------|----------|--------|-------------|
| Mean cost per task | | | | |
| Input / cached / output tokens | | | | |
| **Success rate (guardrail)** | | | | must not drop beyond noise |
| Median latency | | | | |
| Cost of the fix itself (extra model calls, maintenance) | | | | include it (e.g., the summarization call) |
| Cache hit % | | | | a compaction that wipes the cache can erase its own savings |
| Capacity and latency (when those are the goal) | | | | tokens per minute per request, utilization, p95 latency, time to first token |
| Decision accuracy of changed components (routers, classifiers) | | | | from the stage-3 component eval; per class, not only average |
| Recall of must-keep facts (lossy fixes only) | | | | from the stage-3 recall check |

Count **every** cost the fix introduces (e.g., a judge model, a summarizer), as TokenEconomizer does in
its reports.

<figure class="at-doc-figure">
<div class="at-diag-stack">
<div class="at-diag-row"><span>Baseline</span><div class="at-diag-bar"><span class="at-diag-seg seg-miss" style="width: 100%"></span></div><span>39,200 input tok</span></div>
<div class="at-diag-row"><span>Illustrative fix</span><div class="at-diag-bar"><span class="at-diag-seg seg-hit" style="width: 38.8%"></span></div><span>~15,200 input tok</span></div>
</div>
<figcaption>In the <a href="https://github.com/dotnetpower/agent-tokenomics/blob/main/examples/sample-requests.jsonl">sample</a>, s1 turn 1 has 39,200 input tokens over four calls. The after value assumes two calls of 7,200 and 8,000 input tokens after tool pruning and a loop guard; it is not a measured result. A held pass rate is also an assumption: repeat the same tasks to verify quality and cost. Test the two changes separately.</figcaption>
</figure>

## Decision

- **Keep** if cost fell and success held. Record the change, the numbers, and the date.
- **Revert** if success dropped or the saving is within noise.
- **Iterate:** go back to stage 2 with the next-largest cause. Savings usually come from several small
  fixes, not one.

## Report template

```text
Change:        <what was done>
Cause proven:  <link to stage-3 record>
Cost / task:   <before> -> <after>  (<delta %>)
Success:       <before> -> <after>
Side costs:    <included>
Decision:      keep | revert
Next cause:    <from stage 2 ranking>
```

## Keep it healthy

Re-run the diagnosis periodically: new tools, longer instructions, or model changes can silently bring
the waste back.

### Continuous drift guard

After keeping a fix, run a fixed representative task set in CI or a weekly job when tools or instructions change. Record the baseline and assert that `first_call_input` (first-call input tokens, a fixed-overhead proxy) and `unused_tool_defs` (definitions unused across the selected task set) stay within agreed thresholds. Compare the same model, tools, and tasks; investigate intentional changes rather than silently updating the baseline. An unused tool on a small set is only a candidate for review, not proof it is unnecessary. Track task success alongside cost so the check does not reward a quality regression.

## Further reading (papers)

Ordered by venue tier, then citations. Findings are hypotheses to verify on your own workload; see the full list in [Papers](../../papers/).

- [AI Agents That Matter](https://arxiv.org/abs/2407.01502) — TMLR 2024, 289 citations. Argues agent evaluations must control for cost, not just accuracy.
- [Cost-of-Pass: An Economic Framework for Evaluating Language Models](https://arxiv.org/abs/2504.13359) — ICLR 2025, 56 citations. Defines the expected cost to obtain a correct answer and compares models on it.
- [τ-bench: A Benchmark for Tool-Agent-User Interaction in Real-World Domains](https://arxiv.org/abs/2406.12045) — preprint 2024, 1,669 citations. Benchmark that introduces pass^k to measure agent reliability across repeated runs.
- [Adding Error Bars to Evals: A Statistical Approach to Language Model Evaluations](https://arxiv.org/abs/2411.00640) — preprint 2024, 194 citations. Statistical guidance for reporting uncertainty in LLM evaluations.
