---
title: Agent Tokenomics
description: A troubleshooting procedure for reducing the token cost of LLM agents.
track: agent
verified: 2026-11-03
---

Track B is for engineers who **build or operate** LLM agents. Treat high token cost like a production
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

Plus a [case study of TokenEconomizer](case-tokeneconomizer/), the [ranked paper list](../papers/) behind the fixes, and a runnable
[measurement script](https://github.com/dotnetpower/agent-tokenomics/tree/main/examples).

## What you need

- A **request log**: one record per model call with input / cached / output tokens, tool definitions sent,
  tool calls and errors, and which user prompt (turn) and session it belongs to. Most agent frameworks can
  emit this; otherwise wrap your model client.
- A **task set** you can re-run (5–20 representative tasks) with an automatic success check.
- Discipline: change one variable at a time.

## Rule zero

A token reduction is only a win if **task success does not drop**. Always report cost *and* a quality metric.
