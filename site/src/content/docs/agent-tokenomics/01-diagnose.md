---
title: "1. Diagnose the current state"
description: Measure where tokens go before changing anything.
track: agent
stage: diagnose
verified: 2026-11-03
sidebar:
  order: 1
---

**Goal:** a baseline you trust. Do not optimize yet.

## Symptoms

- The bill or token count is higher than expected, or grows faster than usage.
- Sessions slow down as they get longer.
- Similar tasks have very different costs.

## Collect the request log

You need one record per model call (fields: see the [index](../)). Three ways to get it:

1. **Provider usage fields** — log the `usage` object each API response returns (input, cached, output, reasoning tokens).
2. **Tracing SDK / OpenTelemetry** — instrument the agent so each model call and tool call becomes a span.
3. **Gateway / proxy** — route model traffic through a gateway that logs every request.

These platforms can do 2 and 3 for agents you build or operate. Pick one; do not stack several on the same traffic.

| Platform | Style | Good for | Links |
|----------|-------|----------|-------|
| **Arize Phoenix** | Tracing (OpenTelemetry / OpenInference); self-hostable (check its license) | Step-by-step traces of model, retrieval, and tool calls; evals and prompt experiments in one place | [docs](https://arize.com/docs/phoenix), [repo](https://github.com/Arize-ai/phoenix) |
| **Langfuse** | Tracing SDKs and integrations; open source, self-hostable or cloud | Per-generation usage and cost by type (input, output, cached…), cost dashboards, alerts, Metrics API | [token & cost tracking](https://langfuse.com/docs/observability/features/token-and-cost-tracking), [repo](https://github.com/langfuse/langfuse) |
| **LangSmith** | Tracing (LangChain's hosted platform) | Token and cost breakdown per trace (input incl. cache reads, output incl. reasoning), project stats and dashboards | [cost tracking](https://docs.langchain.com/langsmith/cost-tracking) |
| **Helicone** | Gateway / proxy (OpenAI-compatible) | Logging every request with minimal code change; fallbacks across providers | [quickstart](https://docs.helicone.ai/getting-started/quick-start), [repo](https://github.com/Helicone/helicone) |
| **Portkey** | Gateway (unified API) plus observability | One interface to many models with monitoring, controls, and guardrails | [docs](https://portkey.ai/docs/introduction/what-is-portkey), [repo](https://github.com/Portkey-AI/gateway) |

How to choose:

- **Tracing** shows the *structure* of a run (which step, which tool, how nested) — best for isolating
  loops and bloated tool output. It needs instrumentation in your code or framework.
- **Gateways** are quickest to add (change a base URL) and see every call, but they see only the model traffic,
  not what happened between calls, and they sit in the path of your prompts.
- Check each product's current license, hosting options, pricing, data-retention, and whether it records the
  **cached-token and reasoning-token** fields you need. Features change; verify before committing.
- Prompts and tool results often contain private code or data: configure **redaction / sampling** and review
  the data-handling terms before sending production traffic.
- Tools that ship model price tables compute cost for you, but their prices can lag. Treat dollar figures as
  estimates and keep **token counts** as the primary metric.

To use [`token_report.py`](https://github.com/dotnetpower/agent-tokenomics/tree/main/examples) with any of them,
export or query the platform's data and map it to the JSONL fields (`session`, `turn`, `input_tokens`,
`cached_input_tokens`, `output_tokens`, `tool_defs`, `tool_calls`).

Using GitHub Copilot in VS Code instead? These do not instrument it; use
[Agent Debug Logs and the Cache Explorer](../../copilot-tokenomics/02-measure-usage/).

## Checks

Run the example script on your log:

```bash
python examples/token_report.py requests.jsonl
```

Record these for the baseline:

| Metric | How to get it | Why it matters |
|--------|---------------|----------------|
| Input / cached / output tokens per request | Provider usage fields | Input usually dominates; cached input is cheaper |
| **Cache hit %** | cached ÷ input | Low hit % with a stable prefix suggests prefix churn |
| **Fixed overhead** | Input tokens of the *first* request in a session | System prompt + instructions + tool definitions, paid every request |
| **LLM calls per user request (fan-out)** | Count model calls that share a request or trace id | One question may trigger many calls; each carries its own prompt |
| **Per-step token map** | Per step: model, calls, fixed tokens per call, dynamic tokens | Shows which step and which component dominates (see below) |
| **Coverage** | Sum of logged tokens vs the provider's billed or monitored tokens for the same period | A gap means un-instrumented calls (sub-services, embeddings, background jobs) |
| **Tool-definition tokens** | Sum of tool schemas sent | Paid on every request, used or not |
| **Context growth curve** | Input tokens by request index | Steep growth = history or tool output accumulating |
| **Tool output tokens by tool** | Tokens each tool result adds | Finds bloated results (big file reads, verbose logs) |
| **History vs tool-output share** | Split the growth in each request into *conversation text* and *tool results* | Decides whether summarizing history can help at all; tool results are often the larger part |
| **Retries / errors per turn** | Tool-call error signatures | Repeated errors = wasted loops |
| Cost per successful task | Cost ÷ passed tasks | The number you ultimately want to lower |

## Per-step token map

For agents that run several model calls per user request, build one table per request type:

| Step | Model | Calls | Fixed tokens / call | Dynamic tokens / call | Total | Share |
|------|-------|------:|--------------------:|----------------------:|------:|------:|
| *(one row per step)* | | | | | | |

Rank **components** (system prompt, tool schemas, output schema, reference documents) by `tokens × calls`. The top
one or two usually explain most of the waste.

### No production access? Estimate statically

If you cannot read production usage yet, count the fixed prompt parts from the code with a tokenizer.
[`examples/prompt_budget.py`](https://github.com/dotnetpower/agent-tokenomics/blob/main/examples/prompt_budget.py)
takes a small JSON description of the steps (a prompt file or a token count, plus calls per request) and prints
the map:

```bash
python3 examples/prompt_budget.py examples/sample-pipeline.json
```

Treat the result as a **lower bound**: it excludes everything that arrives per request (user data, retrieved
passages, conversation history, tool results), so real usage is higher. Use it to rank candidates; confirm with
real measurements as soon as you can, and say plainly in any report that the figures are estimates. Also flag
steps whose fixed prompt is below your provider's minimum cacheable length.

## Interpretation

| Observation | Likely direction (verify in stage 2) |
|-------------|--------------------------------------|
| High overhead on request #1 | Large instructions or tool catalog |
| Steep growth between requests | Large tool outputs or unpruned history (check the history vs tool-output split before choosing a fix) |
| Low cache hit % | Prefix changes between requests |
| Same error signature ≥ 3 in a turn | Failure loop |
| High output tokens | Verbose answers or excessive reasoning effort |
| One reference document is a large share of a step, or the step is called more than once | Static prompt bloat × call count |
| Many LLM calls per user request, several of them small classifiers or checks | Redundant pipeline steps (fan-out) |
| Logged tokens are well below billed tokens | Un-instrumented calls: add them to the log before trusting any conclusion |
| A per-session list (messages, reports, tool results) only ever grows | Unbounded state: find the truncation logic; if none, add it |

## Sharing measurements safely

Diagnosis often involves a vendor, a platform team, or a customer. Share what is needed and no more:

- **Safe by default:** aggregated token counts, prompt sizes, call counts, hit rates, error signatures.
- **Ask first:** prompt text, tool results, retrieved passages, user or customer data, internal names.
- Redact identifiers and sample or aggregate before export; remember that log and trace platforms store whatever
  you send them.
- Agree up front who decides what: the platform or vendor team proposes structure and settings; the domain
  owner decides which facts must survive a summary; the security owner approves retention or data-handling changes.

## Worked example

On the sample log, `s1` shows 9,200 tokens of overhead on its first request, of which two tool definitions
(`notebook_edit`, `browser_open`) are never called. Turn 1 in `s1` has three identical
`pytest: command not found` errors.

For a pipeline, run the static estimate on the synthetic
[`sample-pipeline.json`](https://github.com/dotnetpower/agent-tokenomics/blob/main/examples/sample-pipeline.json)
(invented numbers for illustration): 8 LLM calls per request and about 78,800 fixed tokens. One reference
document attached to the answer step (called twice) accounts for about half of all fixed tokens, and the
guardrail step is under 1,024 tokens, so it likely cannot be cached. That single table already says where to look first.

## Next

Pick the **largest** suspicious signal and go to [2. Isolate](../02-isolate/).
