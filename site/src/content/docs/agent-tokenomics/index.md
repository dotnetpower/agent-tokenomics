---
created: 2026-11-03
updated: 2026-11-19
dateSource: git
title: "Agent Tokenomics (Token Economics)"
description: "LLM agent tokenomics and token economics troubleshooting guide: Diagnose, Isolate, Prove, Improve, and verify quality with tokprof and golden datasets."
track: agent
verified: 2026-11-03
---

Track B covers **Agent Tokenomics (LLM token economics)** for engineers who **build or operate** LLM agents. Treat high token cost like a production
incident: do not guess, follow a procedure.

New here? Start with [0. Concepts](00-concepts/) — how an agent spends tokens — then follow the procedure:

```text
Diagnose → Isolate → Prove → Improve → Result
   └────────── repeat with the next biggest cause ──────────┘
```

| Stage | Question | Output |
|-------|----------|--------|
| [0. Concepts](00-concepts/) | How does an agent spend tokens? | A shared mental model |
| [1. Diagnose](01-diagnose/) | Where do tokens go today? | A measured baseline |
| [2. Isolate](02-isolate/) | Which cause is responsible? | One ranked hypothesis |
| [3. Prove](03-prove/) | Is that cause real and how big? | A reproduced, quantified waste |
| [4. Improve](04-improve/) | What targeted fix removes it? | One change, one experiment |
| [5. Result](05-result/) | Did cost fall without hurting quality? | Keep / revert decision |
| [6. Golden dataset and rubric evaluation](06-golden-dataset-and-rubric/) | How do you grade single-turn and multi-turn quality (keywords + intent)? | Excel (CSV) dataset & hybrid rubric |

For an evidence-light discussion across measurement, optimization, evaluation, and release, use the [practical checklist below](#practical-checklist).

Plus [tokprof: Install & Usage](tokprof/), a [case study of TokenEconomizer](case-tokeneconomizer/), the [Coding Agent Token, Cache & Quota Anatomy (Copilot · Codex · Claude)](../copilot-tokenomics/coding-agent-quotas-and-caching/), the [ranked paper list](../papers/) behind the fixes, and a runnable
[measurement script](https://github.com/dotnetpower/agent-tokenomics/tree/main/examples).

<figure class="at-doc-figure">
  <img src="https://code.visualstudio.com/assets/docs/agents/concepts/agent-loop.png" alt="An agent repeatedly uses tools and their results to advance a task" loading="lazy" />
  <figcaption>The agent loop: instructions, tools, conversation history, and tool results shape successive calls. Stages 0–5 diagnose and optimize their cost. Sources: <a href="https://code.visualstudio.com/docs/agents/concepts/agents">VS Code Docs — Agents Overview</a> and <a href="https://github.com/microsoft/TokenEconomizer">microsoft/TokenEconomizer</a>.</figcaption>
</figure>

## What you need

- A **request log**: one record per model call with input / cached / output tokens, tool definitions sent,
  tool calls and errors, and which user prompt (turn) and session it belongs to. Most agent frameworks can
  emit this; otherwise wrap your model client.
- A **task set** you can re-run (5–20 representative tasks) with an automatic success check.
- Discipline: change one variable at a time.

## Rule zero

A token reduction is only a win if **task success does not drop**. Always report cost *and* a quality metric.

## Practical checklist

> **Reference date: 2026-11-19.** This independently written, generic **synthetic checklist** is based only on this repository's public procedures and linked billing guide. It contains no customer case, actual measurements, or promised savings. Provider policies, rates, supported controls, and telemetry can change; recheck the applicable original documentation before making an operating decision.

- Agree on the task boundary, success criteria, and evidence-sharing scope. Ask the open question, then select relevant follow-ups. No specific product, Copilot setting, or observability platform is required.
- Separate **observed**, **not applicable** (outside this workflow), **unconfirmed** (insufficient evidence), and **unsupported** (the system does not offer the control or field). Record the reason; none automatically means zero usage or poor maturity.
- The last column suggests a minimum starting point, not a submission requirement. Prefer an existing aggregate, a bounded redacted excerpt, or a walkthrough over a full production export. Minimize raw text collection; remove personal data, credentials, internal names, and original identifiers before sharing. Use local pseudonymous IDs where needed, agree access and retention, and do not publish private evidence or links.
- Keep agreed criteria, observations, and hypotheses separate. Choose one evidenced gap and one reversible change; define quality guardrails and keep/revert conditions before testing. If evidence is insufficient, measure first rather than approving a release.

| Topic | Neutral open question | Checks | Plain explanation | Minimum evidence |
|---|---|---|---|---|
| 1. Retries and failure loops | What happens when the same error recurs, and how is the decision to retry, redirect, or stop made? | Repeated-error signatures; model-call, step, and retry caps with explicit request/session scope; stop and escalation behavior. Distinguish backoff delays from a retry-count limit. | Waiting longer between attempts does not bound how many attempts occur. | One redacted failure sequence with its stop reason and the relevant limit excerpt. |
| 2. Single-agent and multi-agent work | How do single-agent and multi-agent approaches compare on the same complete task, and what explains the difference? | Total calls and parent/child input/output; coordination, review, and rework; end-to-end time and success rate. Count each call once, not child calls plus a parent aggregate that already includes them. | Parallel work can finish sooner without using fewer tokens. | One matched-task comparison with call-level totals or an aggregate whose inclusion rules are stated. |
| 3. Prompt measurement and versions | How are request components assembled, counted, and versioned? What changes when history grows, the prefix changes, or tools load on demand? | Instructions, user input, history, retrieval, tool definitions/results, and response schemas; fixed/dynamic split; change history linked to evaluation. Label static estimates with tokenizer/model version, separate them from actual provider usage, and note tokenizer differences. Check the schemas actually sent, cache eligibility, prefix stability, and reported reuse rather than assuming all enabled tools load upfront. | A short prompt file does not describe the whole request; an estimate is not a usage receipt. | One component token map with counting method and prompt version linked to an evaluation record. |
| 4. Retrieval-augmented generation (RAG) | How is retrieved material selected for the final model request, and what must remain when reducing it? | Chunk count, length, overlap, duplicates, reranking, and tokens finally delivered; required evidence, access permissions, and source attribution. Check retrieval quality as well as payload size. | A smaller index does not necessarily mean less model input. | One anonymized retrieval summary showing selected chunks, final token count, and evidence/permission checks. |
| 5. Model and reasoning effort | How are model or effort choices compared, and how are output and reasoning budgets chosen for the task? | Same representative tasks and pass criteria; change one model/effort variable while controlling other settings, prompts, tools, retrieval, and cache conditions. Compare total tokens, actual metered cost, time, retries, and success. Check supported output/reasoning limits and failures from truncation or insufficient reasoning. | A cheaper unit rate does not prove fewer tokens or lower cost per successful task. | One controlled comparison with model/settings, quality results, and available usage/cost readings. |
| 6. Request tracing | How can one user request be followed through internal model calls, failures, retries, and final completion? | Request/session/step and parent-child IDs; model and prompt versions; input, cache-read/write where exposed, and output tokens; failure/retry links and final success. State field semantics and missing coverage. Minimize sensitive text. | One user request may trigger many calls. A field that is not exposed is not zero. | One redacted trace outline and a short field/coverage description, without raw prompts. |
| 7. Evaluation dataset and rubric | What examples and acceptance rules show that the answer is correct, including after a long conversation or summary? | Representative, exception, failure, and multi-turn cases; required facts, conditions, evidence, forbidden actions, and acceptable wording. For long-context/summary changes, test must-keep facts and task completion. Combine keyword/slot checks with semantic review or a calibrated rubric. | Keeping keywords or slots does not prove that the meaning or decision is correct. | A small reviewed dataset excerpt with pass criteria and a failed or borderline example; no full conversation archive. |
| 8. Deployment decision | What agreed conditions lead to release, hold, an exception, or rollback, and who makes that decision? | Predeclare quality, safety, cost, and latency thresholds; hold conditions; exception owner, scope, and expiry; rollback trigger and responsible role. | Lower average tokens alone do not authorize deployment. | An existing release checklist or decision record containing the agreed gates. |
| 9. Feedback loop | How does feedback become a prioritized change, a new evaluation case, and a keep/revert decision? | Classify by cause, prioritize impact, change one factor, add/review evaluation cases, rerun, and record the decision. Include unresolved feedback. | Automatic retraining alone is not a maturity measure; a traceable improvement cycle matters. | One anonymized feedback-to-decision record, with its evaluation link or unresolved gap. |
| 10. Regression and effect verification | How is an improvement distinguished from variation, and how are losses or failures handled? | Same fixed evaluation set and grading version; comparable model/settings/cache conditions, except the declared variable. Repeat runs and report uncertainty, failure/loss cases, and rollback. Use the same task inputs and report any unavoidable mismatch. | A small sample cannot establish quality equivalence, even when average cost falls. | One paired before/after report with repetition counts, uncertainty, failures, and keep/revert rationale. |
| 11. Operational outcomes | What actual improvement can you substantiate with minimal anonymized evidence, and where did it not help? | Usage by task type and token category, attributed cause, before/after results, and success rate. Cost per successful task = all in-scope attempt costs ÷ successful tasks; no successes makes it undefined, not zero. Include failures, summarization, delegation, coordination/review, and evaluation costs; identify tool/retrieval charges separately. Apply cache-read/write and output rates correctly; do not add cached input or reasoning again when already included. Keep measured charges separate from estimates or subscription quotas. | Ask for a real observed outcome, not a savings promise; count the entire workflow, not just its final answer. | One approved anonymized aggregate with period, task mix, successes/attempts, cost basis, and caveats. If unavailable, record unconfirmed. |

### Sources

- [1. Diagnose](01-diagnose/): request logging, component token maps, static-estimate limits, and safe sharing.
- [2. Isolate](02-isolate/) and [4. Improve](04-improve/): cause attribution, on-demand tools, loop guards, and context/cache trade-offs.
- [3. Prove](03-prove/) and [5. Result](05-result/): controlled comparisons, uncertainty, quality guardrails, and keep/revert decisions.
- [6. Golden dataset and rubric evaluation](06-golden-dataset-and-rubric/): single-turn/multi-turn cases and keyword-plus-intent evaluation. Its worked examples are not measurements for this checklist.
- [Coding Agent Token, Cache & Quota Anatomy](../copilot-tokenomics/coding-agent-quotas-and-caching/#total-tokens-vs-cost-vs-quota): usage-field normalization, cache/output pricing, and original provider links. Product-specific controls are optional examples, not requirements for general agents.
