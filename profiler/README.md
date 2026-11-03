# tokprof

Token profiler for LLM agents. It breaks down the **input tokens of every model call** into system prompt,
conversation history, current user prompt, and tool-related content (definitions, calls, results), without
changing the agent's code and without installing anything into the agent's project.

> Status: early development (R1). Works today: the local gateway backend for processes started under
> `tokprof`, request/response parsing for OpenAI chat, OpenAI Responses, and Anthropic Messages (including
> streaming), reports, `doctor`, and `selftest`. Planned: runtime preload hooks, OS-level local capture,
> and a Linux eBPF tap for already-running processes. See the project plan in the repository docs.

## Install (isolated, no dependency on your agent's project)

```bash
uv tool install ./profiler            # or: pipx install ./profiler
uv tool install "./profiler[tiktoken]"  # better counting accuracy
```

## Use

```bash
tokprof run -- python my_agent.py      # any command, any language; no code changes
tokprof shell                          # a shell whose child processes are profiled
tokprof report .tokprof/run-*.jsonl    # breakdown, per-step map, findings
tokprof doctor                         # what works on this machine
tokprof selftest                       # verify the pipeline with a local mock LLM
```

`tokprof run` starts a gateway on `127.0.0.1` and sets `OPENAI_BASE_URL`, `ANTHROPIC_BASE_URL`, and
`AZURE_OPENAI_ENDPOINT` (when already set) **for that command only**. Existing values are kept as the upstream.
Use `--map ENV_VAR=UPSTREAM_URL` for other base-URL variables.

## What is recorded

Token counts, salted hashes, text lengths, section and tool names, usage reported by the provider, timings.
**Not recorded:** prompt or response text, headers, API keys. Files are created with mode `0600`.

## How the breakdown is computed

Providers report only totals. `tokprof` splits each request into segments, counts each locally, and shows the
difference from the reported input as **Unattributed** (message template overhead, images, hidden tokens).
Counts are estimates; install `tiktoken` for OpenAI-family accuracy. Large system prompts are split at
markdown headings and XML-style tags, and `--component NAME=@file` attributes a known static text by name.

## Limits (today)

- Only processes started through `tokprof run` / `tokprof shell` are captured, and only if they honor the base-URL
  environment variables (clients that hard-code `base_url` need the planned preload hook).
- The gateway opens a new upstream TLS connection per request and asks upstream for uncompressed responses.
- HTTP(S) proxy settings of the original environment are not yet honored for the upstream connection.

## Develop

```bash
uv venv && uv pip install -e ".[dev]"
pytest
tokprof selftest
```
