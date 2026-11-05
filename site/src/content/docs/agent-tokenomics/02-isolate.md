---
title: "2. Isolate the problem"
description: Attribute cost to one cause category by changing one variable at a time.
track: agent
stage: isolate
verified: 2026-11-03
sidebar:
  order: 2
---

**Goal:** turn "it's expensive" into one specific, testable cause.

## Cause categories

| Category | Signal in the diagnosis | Isolating experiment |
|----------|------------------------|----------------------|
| **Re-learning repo facts** | Same discovery calls and the same corrected command at the start of many sessions | Count identical early tool calls across sessions |
| **Unused tools** | Tool definitions never called, yet sent every request | Compute `definition tokens × requests` per unused tool |
| **Failure loops** | Same error signature repeated in a turn | Count repeats per turn; read the transcript |
| **Bloated tool output** | A few tools dominate tool-output tokens | Rank tools by output tokens; inspect the largest results |
| **History growth** | Steep input growth with little new information, and conversation text (not tool results) is the larger share | Replay a session with history truncated or summarized; check that cache hit % does not collapse |
| **Prefix churn (cache misses)** | Low cache hit % | Diff consecutive requests; find the first divergence |
| **Over-large model / effort** | Easy steps use the biggest model | Re-run those steps with a smaller model |
| **Retries and rework** | Many tool calls per accepted change | Inspect turns that ended in revert/redo |
| **Context rot** | Quality drops in long sessions before the window is full; instructions given early are ignored | Re-run the task with a fresh session carrying a short summary; compare success |
| **Tool-count × step-count multiplier** | Large tool catalog *and* many steps per task | Compute `schema tokens × steps`; re-run with a minimal tool set |
| **Large static documents × call count** | One reference document (rules, cheat sheet, policy) is a big share of a step's fixed tokens, and the step is called more than once | Rank components by `tokens × calls`; re-run with the document attached only when relevant and compare |
| **Redundant pipeline steps (fan-out)** | Many LLM calls per user request; some are small checks or classifiers | Trace one request end to end; for each step ask what changes if it is skipped, merged, or done with a rule |
| **Unbounded per-session state** | Input grows turn after turn from internal messages, reports, or tool results that are never trimmed | Search the code for truncation or windowing; replay a long session with the state capped |
| **Short, uncacheable prompts** | Several steps have fixed prompts below the provider's minimum cacheable length | Compare each step's fixed tokens with the minimum; check `cached_tokens` is 0 for them |
| **Over-broad prompt / instructions** | Agent explores widely, or instructions repeat what the code already says | Replace with a scoped prompt and a trimmed instruction file; compare steps and success |

## Method

1. Rank categories by **estimated tokens at stake** (from the baseline), not by how plausible they feel.
2. For the top one, design an experiment that changes **only that variable** (e.g., remove the unused tools,
   nothing else).
3. Write the hypothesis in one sentence with a number:
   *"Unused tool definitions cost ~14,000 tokens across 7 requests (19% of input)."*

<figure class="at-doc-figure">
<div class="at-diag-stack">
<div class="at-diag-row"><span>Unused tools</span><div class="at-diag-bar"><span class="at-diag-seg seg-tool" style="width: 100%"></span></div><span>2,000 tok/call</span></div>
<div class="at-diag-row"><span>run_tests ×3</span><div class="at-diag-bar"><span class="at-diag-seg seg-miss" style="width: 100%"></span></div><span>28,800 input tok</span></div>
<div class="at-diag-row"><span>read_file output</span><div class="at-diag-bar"><span class="at-diag-seg seg-hist" style="width: 100%"></span></div><span>4,200 output tok</span></div>
</div>
<figcaption>The <a href="https://github.com/dotnetpower/agent-tokenomics/blob/main/examples/sample-requests.jsonl">sample</a> s1 shows definitions per call, total input across the three error calls, and one <code>read_file</code> result; the log does not establish whether it was filtered. Bar lengths are not comparable because units and scopes differ; not all 28,800 tokens are avoidable. The causes can overlap, so do not sum them.</figcaption>
</figure>

## Interpretation

- Rule out **tool output** before blaming history: if tool results dominate the growth, truncating or
  paginating them is cheaper and safer than summarizing the conversation.
- If two causes overlap (e.g., a failure loop also inflates history), attribute tokens to the **root**
  (the loop) and handle that first.
- If no category explains more than ~10% of tokens, the baseline may be fine; check whether the *task*
  genuinely needs that much context before optimizing further.

## Further reading (papers)

Ordered by venue tier, then citations. Findings are hypotheses to verify on your own workload; see the full list in [Papers](../../papers/).

- [SWE-agent: Agent-Computer Interfaces Enable Automated Software Engineering](https://arxiv.org/abs/2405.15793) — NeurIPS 2024, 2,357 citations. Shows that interface design, including concise and bounded observations, changes agent success.
- [Reducing Cost of LLM Agents with Trajectory Reduction](https://arxiv.org/abs/2509.23586) — PACMSE 2025, 68 citations. Reduces agent cost by trimming waste out of the trajectory.
- [Agentless: Demystifying LLM-based Software Engineering Agents](https://arxiv.org/abs/2407.01489) — preprint 2024, 566 citations. A fixed localize-repair-validate pipeline competes with open-ended agent loops on SWE-bench Lite at low cost.
- [The Complexity Trap: Simple Observation Masking Is as Efficient as LLM Summarization for Agent Context Management](https://arxiv.org/abs/2508.21433) — preprint 2025, 66 citations. Simple observation masking is as efficient as LLM summarization for agent context management.
- [Efficient Agents: Building Effective Agents While Reducing Cost](https://arxiv.org/abs/2508.02694) — preprint 2025, 40 citations. Systematic study of the efficiency-effectiveness trade-off in agent system design.

## Next

Reproduce and quantify it in [3. Prove](../03-prove/).

