---
title: "4. Improve"
description: Apply one targeted fix mapped to the proven cause.
track: agent
stage: improve
verified: 2026-11-03
sidebar:
  order: 4
---

**Goal:** the smallest change that removes the proven waste. One fix per experiment.

## Fix catalog

| Proven cause | Fix | Notes |
|--------------|-----|-------|
| Re-learning repo facts | Put short, confirmed repo facts at the start of the session (instructions file or injected "micro-skills"): correct test command, required env var, where generated files live | Keep facts few and verified; drop a fact when later sessions contradict it |
| Unused tools | **Prune** tool definitions; load tools on demand; give sub-agents narrow tool sets | Keep a one-step way to re-enable a pruned tool |
| Failure loops | Detect repeated identical errors in a turn; inject **one** redirect ("try a different approach"); if still stuck, tell the user how to stop | Never silently kill a turn; cap redirects per turn |
| Bloated tool output | Truncate or paginate results; search before reading whole files; return summaries; run bulk loops in a script and return only the result | Make truncation visible so the agent can ask for more |
| History growth | Compact/summarize older turns; start fresh sessions per task; isolate exploration in sub-agents | Summaries are lossy — see [Compaction without losing the cache](#compaction-without-losing-the-cache) |
| Prefix churn | Put stable content first (system prompt, instructions, tool definitions), volatile content last; do not edit the prefix mid-session | Check with a cache-diff view |
| Over-large model / effort | Route easy steps to a smaller model; lower reasoning effort by default | Gate with the stage-3 quality check |
| Retries and rework | Better success criteria in the prompt; validate cheaply before expensive steps | Often also improves quality |
| Context rot | Put critical rules at the start *and* restate key constraints near the end; compact or restart before the window is nearly full | Models attend less reliably to the middle of long contexts ([Lost in the Middle](https://arxiv.org/abs/2307.03172)) |
| Tool-count × step-count multiplier | Per-workspace tool sets; audit servers by usage; cap steps (`maxTurns`-style limit) and define a done-criterion | See [MCP audit](../../copilot-tokenomics/09-mcp-audit/) |
| Over-broad prompt / instructions | Keep only "landmine" facts; declarative rules; one task per prompt; a plan file for complex work | Context files often add cost without raising success ([arXiv:2602.11988](https://arxiv.org/abs/2602.11988)) |
| Long, stable knowledge in every prompt | Package as an on-demand **skill** instead of always-on text | Keep one source of truth |
| Verbose model output | Standing output limits ("code only", fixed formats); structured output | Output tokens usually cost more than input |
| Large static documents × call count | Attach a reference document only when the question needs it (rule or retrieval); split into sections and include the relevant ones; load on demand through a tool or skill; attach it once rather than on every call of a multi-call step | Gate the condition with the component-level eval from stage 3; measure `tokens × calls` |
| Redundant pipeline steps (fan-out) | Skip a step with a deterministic check; merge neighboring classification steps into one call; use a smaller model for guardrails and routing; drop steps whose output is rarely used | Each removed step must pass the stage-3 eval; keep the old path behind a flag until proven |
| Unbounded per-session state | Cap or window internal messages; keep only what later steps need (a short result, not the full report); summarize by the compaction rules below | Search for the missing truncation first: it is often a one-line fix |
| Short, uncacheable prompts | Merge the step into a neighbor, replace it with a rule, or accept it (it is small); never pad to reach the minimum | Check the provider's current minimum and `cached_tokens` |
| Bulk / polling tool loops | Run the loop in a script and return only the summary | Benchmark scripted vs interactive |

## Compaction without losing the cache

Compaction and prompt caching pull in opposite directions: rewriting the *beginning* of the history changes the
request prefix, so the provider can no longer reuse its cache and you pay full input price again.

- **Keep the prefix stable.** System prompt, instructions, and tool definitions first and unchanged; keep the
  history append-only between compactions.
- **Compact rarely, in one batch,** when a threshold is reached (for example, a fraction of the context window)
  or at a task boundary — not every turn.
- **Break-even check.** Compact only if
  `saved tokens × remaining turns × (1 − cached-input discount)` exceeds
  `summarization call cost + cache rebuild cost`. Short remaining sessions rarely pay back.
- **Structure the summary** by what the agent needs, not by chronology:

```text
Goal:            <one line>
Decisions:       <what was decided and why, briefly>
Constraints:     <rules, versions, "do not touch X">
Done / artifacts:<files changed, commands that worked, with paths>
Open items:      <unresolved questions, next step>
Failures to avoid: <error signatures already seen and what fixed them>
```

  A few lines of natural language can carry nuance the fields cannot; use the structure as the core and add
  prose only for what does not fit.
- **Summarize in the background, off the critical path:** trigger when the context passes a token threshold, run
  asynchronously after the answer, and use the result from the next turn on.
- **Mix summary and raw tail:** send the summary plus the last few raw turns, and cap the summary length. The
  summarizer's own prompt is usually short (often below the cacheable minimum), so its cost is mostly
  uncached input; make sure the savings on later calls outweigh it (break-even above).
- **Domain owners decide what must survive** (for example, the facts a human advisor would never want lost);
  engineers implement and test it. Review the list when the domain changes.
- **Offload instead of summarizing** when you can: save long raw output to a file and keep a one-line summary
  plus the path, so the agent can re-read it on demand (near-lossless).
- **Guard the summary:** do not summarize a summary repeatedly (errors compound); keep the original; redact
  secrets and personal data before they enter a summary.
- Validate with the [lossy-fix criteria](../03-prove/#when-the-fix-is-lossy-summaries-compaction).

## Rules

- **One fix at a time.** Combined changes make results unattributable.
- Define the **revert condition** in advance (e.g., success drops by more than your noise threshold).
- Prefer fixes that are **reversible and visible** (settings/config) over silent behavior changes.
- Black-box first: if you cannot modify the agent itself, act through its configuration, instructions,
  and tool settings.

## Evidence by cause

Published work behind each fix, ordered by venue tier then citations within a row (full list: [Papers](../../papers/)). Treat each as a hypothesis for *your* agent: prove it with [stage 3](../03-prove/).

| Cause | Papers |
|-------|--------|
| Re-learned repo facts | [Agentic Context Engineering](https://arxiv.org/abs/2510.04618) (ICLR 2025); [Agent Workflow Memory](https://arxiv.org/abs/2409.07429) (ICML 2024); [TokenEconomizer](https://arxiv.org/abs/2610.22732) (preprint 2026) |
| Unused tools / tool schema size | [MCP-Zero](https://arxiv.org/abs/2506.01056) (preprint 2025); [RAG-MCP](https://arxiv.org/abs/2505.03275) (preprint 2025); [Less is More (function calling)](https://arxiv.org/abs/2411.15399) (preprint 2024) |
| Failure loops, agent design | [SWE-agent](https://arxiv.org/abs/2405.15793) (NeurIPS 2024); [Agentless](https://arxiv.org/abs/2407.01489) (preprint 2024); [Efficient Agents](https://arxiv.org/abs/2508.02694) (preprint 2025) |
| Bloated tool output / trajectory | [SWE-agent](https://arxiv.org/abs/2405.15793) (NeurIPS 2024); [Trajectory Reduction](https://arxiv.org/abs/2509.23586) (PACMSE 2025); [The Complexity Trap (observation masking)](https://arxiv.org/abs/2508.21433) (preprint 2025) |
| History growth / memory | [Mem0](https://arxiv.org/abs/2504.19413) (ECAI 2025); [Recursively Summarizing](https://arxiv.org/abs/2308.15022) (Neurocomputing 2023); [MemGPT](https://arxiv.org/abs/2310.08560) (preprint 2023); [The Complexity Trap (observation masking)](https://arxiv.org/abs/2508.21433) (preprint 2025) |
| Context rot | [Lost in the Middle](https://arxiv.org/abs/2307.03172) (TACL 2023); [LLMs Get Lost in Multi-Turn Conversation](https://arxiv.org/abs/2505.06120) (ICLR 2025); [Same Task, More Tokens](https://arxiv.org/abs/2402.14848) (ACL 2024) |
| Prefix churn / cache misses | [SGLang](https://arxiv.org/abs/2312.07104) (NeurIPS 2023); [Prompt Cache](https://arxiv.org/abs/2311.04934) (MLSys 2023); [Don't Break the Cache](https://arxiv.org/abs/2601.06007) (preprint 2026) |
| Prompt and context compression | [LongLLMLingua](https://arxiv.org/abs/2310.06839) (ACL 2023); [LLMLingua-2](https://arxiv.org/abs/2403.12968) (ACL 2024); [LLMLingua](https://arxiv.org/abs/2310.05736) (EMNLP 2023) |
| Over-large model | [FrugalGPT](https://arxiv.org/abs/2305.05176) (TMLR 2023); [Hybrid LLM](https://arxiv.org/abs/2404.14618) (ICLR 2024); [RouteLLM](https://arxiv.org/abs/2406.18665) (preprint 2024) |
| Reasoning length / effort | [Token-Budget-Aware Reasoning](https://arxiv.org/abs/2412.18547) (ACL 2024); [L1 (length-controlled reasoning)](https://arxiv.org/abs/2503.04697) (preprint 2025); [Chain of Draft](https://arxiv.org/abs/2502.18600) (preprint 2025) |
| Redundant pipeline steps / unneeded model size | [FrugalGPT](https://arxiv.org/abs/2305.05176) (TMLR 2023); [AutoMix](https://arxiv.org/abs/2310.12963) (NeurIPS 2023); [Agentless](https://arxiv.org/abs/2407.01489) (preprint 2024) |
| Bulk calls | [Batch Prompting](https://arxiv.org/abs/2301.08721) (EMNLP 2023) |

## Next

Re-measure in [5. Result](../05-result/).
