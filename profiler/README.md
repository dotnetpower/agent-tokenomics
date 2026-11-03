# tokprof

Token profiler for LLM agents. It breaks down the **input tokens of every model call** into system prompt,
conversation history, current user prompt, and tool-related content (definitions, calls, results), without
changing the agent's code and without installing anything into the agent's project.

> Status: early development. Works today for processes started under `tokprof`: a local gateway (base-URL
> environment variables) plus in-process hooks (Python `httpx`, Node `fetch`) for clients that hard-code their
> endpoint; OpenAI chat, OpenAI Responses, and Anthropic Messages (including streaming); reports, cost estimate,
> before/after comparison with a bootstrap interval; background sessions; `doctor` and `selftest`.
> Planned: Linux eBPF tap for already-running processes, and OS-level local capture on Windows and macOS.

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
tokprof report run.jsonl --prices prices.json   # add a cost estimate (per 1M tokens, your prices)
tokprof compare before.jsonl after.jsonl        # per-request change with a 95% bootstrap interval
tokprof export run.jsonl --out legacy.jsonl     # schema of examples/token_report.py
tokprof start && eval "$(tokprof env)"          # background session for NEW processes in this shell
tokprof env --unset; tokprof stop               # restore the shell, then stop (refuses while calls are in flight)
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

## Capture modes

- **Gateway:** child processes get `OPENAI_BASE_URL`, `ANTHROPIC_BASE_URL` (and `AZURE_OPENAI_ENDPOINT` when set) pointing at
  `127.0.0.1`. Works for any language whose client honors those variables. Disable with `--no-gateway-env`.
- **In-process hooks:** `PYTHONPATH` gets a `sitecustomize` that wraps `httpx` transports; `NODE_OPTIONS` gets a `--require` shim
  that wraps global `fetch`. Both copy request and response bodies to the local gateway and never modify traffic. Requests that
  already go through the gateway are not recorded twice. Disable with `--no-hooks`. Python's `-I` (isolated) mode ignores
  `PYTHONPATH`, so hooks do not load there.
- The gateway honors `HTTPS_PROXY` / `NO_PROXY` for its upstream connection (HTTP proxies via CONNECT).

## Limits (today)

- Only processes started through `tokprof` (`run`, `shell`, or the environment from `tokprof env`) are captured. Already-running
  processes need the planned Linux eBPF tap (see `ebpf/`).
- Clients that neither honor the base-URL variables nor use `httpx` or Node `fetch` (for example `requests`, `aiohttp`, `axios`)
  are not captured yet.
- The gateway opens a new upstream TLS connection per request and asks upstream for uncompressed responses.
- `tokprof start`/`stop` is POSIX-only for now.

## Develop

```bash
uv venv && uv pip install -e ".[dev]"
pytest
tokprof selftest
```

## Linux eBPF helper (work in progress)

`ebpf/` holds `tokprof-ebpf`, a Go helper for observing TLS plaintext of already-running processes on Linux.
Today it ships one unprivileged subcommand:

```bash
cd ebpf && go build -o bin/tokprof-ebpf ./cmd/tokprof-ebpf
./bin/tokprof-ebpf inspect        # which running processes expose SSL_write/SSL_read we could attach to
```

It reports, per process, whether OpenSSL entry points are exported by a shared library, present only as local
symbols in a statically linked binary (as in `uv`-managed Python builds), or absent. Probes that attach to those
symbols are the next milestone and need `CAP_BPF`/`CAP_PERFMON`.
