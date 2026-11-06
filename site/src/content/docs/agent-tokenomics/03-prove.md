---
title: "3. Prove the problem"
description: "LLM agent tokenomics Stage 3 (Prove): reproduce token waste on a frozen task set and quantify cost and confidence intervals across repeat runs."
track: agent
stage: prove
verified: 2026-11-03
sidebar:
  order: 3
---

**Goal:** a hypothesis is not accepted until a repeatable measurement supports it.

## Checks

1. **Freeze the setup:** model, version, temperature/effort, prompts, tool set, task list, and environment.
2. **Baseline runs:** run every task **at least 3 times** (outputs are nondeterministic). Record per task:
   total tokens by type, cost, success (pass/fail), duration.
3. **Targeted counter-run:** apply only the isolation change from stage 2 (e.g., disable the unused tools)
   and run again with the same task set and repetitions.
4. **Compare:** the waste attributed to the cause should appear as a drop in the specific metric
   (e.g., input tokens on request #1), not just in the total.

## Evidence standard

| Question | Evidence required |
|----------|-------------------|
| Is the cause real? | The metric moves when only that variable changes. |
| How big is it? | Mean and spread across repetitions, per task and overall. |
| Is it general? | It appears in more than one task / session / repository. |
| Is it safe to remove? | Success rate in the counter-run is not lower beyond noise. |

If the spread between repetitions is larger than the effect, run more repetitions or choose a bigger target.

### Cost measurements versus pass-rate guardrails

For a deterministic fixed-prefix reduction, such as a **synthetic** 2,000-token drop, 3–5 repeats per task can establish the size of the token-count change if the observed spread is small; variable effects require checking the spread and repeating as needed. This is not enough to establish success-rate parity.

For a 3–5 percentage-point change in Pass@1, use a substantially larger fixed task set and calculate a paired confidence interval or power analysis before deciding. A 20–50+ task subset can be an exploratory starting point, **not** a sufficient sample-size guarantee: small differences can require hundreds or more tasks depending on paired outcomes, desired power, and the equivalence margin. Report the uncertainty and leave the guardrail inconclusive when it is too wide ([Adding Error Bars to Evals](https://arxiv.org/abs/2411.00640)).

## When the fix is lossy (summaries, compaction)

Compressing history cannot *guarantee* the same result; it can only be **shown** equivalent on your tasks.
Define "equivalent" before running the experiment:

1. **Success parity** — use the same tasks and enough observations that a paired confidence interval fits within the predeclared equivalence margin; otherwise the result is inconclusive.
2. **Preserved facts** — list what must survive: decisions, constraints, file paths, error signatures,
   open TODOs. After compaction, ask questions whose answers are in that list and count correct recalls.
3. **No new rework** — re-asked questions and repeated explorations per task do not increase.
4. **Net cost** — saved input tokens, minus the cost of the summarization call and of any lost prompt-cache
   reuse (see [Improve](../04-improve/#compaction-without-losing-the-cache)), is positive.

Keep the original transcript so a failed recall can be traced to what the summary dropped.

## Component-level evals (routers, classifiers, guardrails)

In a pipeline, a small decision step (which route, which category, allowed or blocked) can silently change the
final answer. When the fix shrinks such a step's prompt or tool descriptions, prove it with a **component-level
eval set** in addition to end-to-end success:

1. Build a labeled set of real-shaped cases (hundreds is typical; sample from production with redaction).
2. Measure **decision accuracy** and a confusion table, before and after the change, on the same cases.
3. Set the minimum acceptable accuracy per class **before** running (the rare, high-cost classes matter more than
   the average).
4. Do not shrink tool descriptions or output schemas without this check: they interact with answer accuracy.
5. For static-document fixes, also report the `tokens × calls` before and after, not only the total.

## Multi-step reliability

Per-step errors compound: if each step succeeds with probability *p*, an *n*-step run succeeds with *pⁿ* (when
steps are independent). That is simple arithmetic, not a measurement of your agent, but it shows why long
loops are fragile:

| Steps | p = 0.95 | p = 0.99 |
|------:|---------:|---------:|
| 5 | 0.774 | 0.951 |
| 10 | 0.599 | 0.904 |
| 20 | 0.358 | 0.818 |
| 50 | 0.077 | 0.605 |

Consequences for your proof: report **cost per *successful* task**, not total tokens; and expect that fixes
which *shorten* the loop (clear done-criteria, pre-stated repo facts, fewer retries) can raise success as well
as cut cost.

<figure class="at-doc-figure">
<div class="at-diag-stack">
<div class="at-diag-row"><span>5 steps</span><div class="at-diag-bar"><span class="at-diag-seg seg-hit" style="width: 77%"></span></div><span>77%</span></div>
<div class="at-diag-row"><span>10 steps</span><div class="at-diag-bar"><span class="at-diag-seg seg-hist" style="width: 60%"></span></div><span>60%</span></div>
<div class="at-diag-row"><span>20 steps</span><div class="at-diag-bar"><span class="at-diag-seg seg-miss" style="width: 36%"></span></div><span>36%</span></div>
</div>
<figcaption>Rounded p<sup>n</sup> values assume independent steps with p = 0.95; these are not measurements of an actual agent.</figcaption>
</figure>

## Template

```text
Hypothesis:   <one sentence with a number>
Variable:     <the single thing changed>
Tasks / reps: <N tasks × R runs>
Baseline:     tokens=<mean±sd>  cost=<>  success=<x/N>
Counter-run:  tokens=<mean±sd>  cost=<>  success=<x/N>
Conclusion:   confirmed | rejected | inconclusive (why)
```

## Worked example

For the sample, the hypothesis "three identical `pytest: command not found` errors in one turn cause two avoidable extra requests (and their re-sent context)" is proven by running the same task with the right command
(`python -m pytest`) stated up front, and confirming the requests per turn drop while success stays equal.

## Further reading (papers)

Ordered by venue tier, then citations. Findings are hypotheses to verify on your own workload; see the full list in [Papers](../../papers/).

- [Evaluating Very Long-Term Conversational Memory of LLM Agents](https://arxiv.org/abs/2402.17753) — ACL 2024, 1,082 citations. Benchmark of very long-term conversational memory.
- [LongMemEval: Benchmarking Chat Assistants on Long-Term Interactive Memory](https://arxiv.org/abs/2410.10813) — ICLR 2024, 869 citations. Benchmark of long-term memory in chat assistants; useful for designing recall checks.
- [AI Agents That Matter](https://arxiv.org/abs/2407.01502) — TMLR 2024, 289 citations. Argues agent evaluations must control for cost, not just accuracy.
- [τ-bench: A Benchmark for Tool-Agent-User Interaction in Real-World Domains](https://arxiv.org/abs/2406.12045) — preprint 2024, 1,669 citations. Benchmark that introduces pass^k to measure agent reliability across repeated runs.
- [Adding Error Bars to Evals: A Statistical Approach to Language Model Evaluations](https://arxiv.org/abs/2411.00640) — preprint 2024, 194 citations. Statistical guidance for reporting uncertainty in LLM evaluations.
- [Non-Determinism of"Deterministic"LLM Settings](https://arxiv.org/abs/2408.04667) — preprint 2024, 159 citations. Shows outputs vary across runs even in settings meant to be deterministic.

## Next

Only a **confirmed** hypothesis moves on to [4. Improve](../04-improve/).

