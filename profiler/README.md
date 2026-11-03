# tokprof

Token profiler for LLM agents. It breaks down the **input tokens of every model call** into system prompt,
conversation history, current user prompt, and tool-related content (definitions, calls, results), without
changing the agent's code and without installing anything into the agent's project.

> Status: early development. Works today for processes started under `tokprof`: a local gateway (base-URL
> environment variables) plus in-process hooks (Python `httpx`, Node `fetch`) for clients that hard-code their
> endpoint; OpenAI chat, OpenAI Responses, and Anthropic Messages (including streaming); reports, cost estimate,
> before/after comparison with a bootstrap interval; background sessions; `doctor` and `selftest`.
> Also: a Linux eBPF tap (`tokprof ebpf`) that captures LLM calls of **already-running** processes without restarting
> or changing them, and `tokprof mitm` (mitmproxy local capture) for Windows and macOS.

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
- **In-process hooks:** `PYTHONPATH` gets a `sitecustomize` that wraps `httpx`, `requests` and `aiohttp`; `NODE_OPTIONS` gets a `--require` shim
  that wraps global `fetch`. Both copy request and response bodies to the local gateway and never modify traffic. Requests that
  already go through the gateway are not recorded twice. Disable with `--no-hooks`. Python's `-I` (isolated) mode ignores
  `PYTHONPATH`, so hooks do not load there.
- The gateway honors `HTTPS_PROXY` / `NO_PROXY` for its upstream connection (HTTP proxies via CONNECT).

## Limits (today)

- Outside the eBPF tap, only processes started through `tokprof` (`run`, `shell`, or the environment from `tokprof env`) are captured.
  Already-running processes need the Linux eBPF tap or `tokprof mitm` (see below).
- Under `tokprof run`, clients that neither honor the base-URL variables nor use `httpx`, `requests`, `aiohttp` or Node `fetch`
  (for example `axios`, `urllib3` used directly) are not captured by the gateway or hooks. `tokprof ebpf` (Linux) and `tokprof mitm`
  see them at the network level.
- `aiohttp` bodies passed as `data=` are captured only when they are `bytes`/`str`; `requests` bodies only when they are `bytes`/`str`.
- The gateway opens a new upstream TLS connection per request and asks upstream for uncompressed responses.
- `tokprof start`/`stop` is POSIX-only for now.

## Develop

```bash
uv venv && uv pip install -e ".[dev]"
pytest
tokprof selftest
```

## Linux eBPF tap (running processes, no restart, no code change)

`tokprof ebpf` attaches uprobes to the TLS library of processes that are **already running** and reads the plaintext of
`SSL_write`/`SSL_read` (and the `_ex` variants), rebuilds HTTP/1.1 exchanges, keeps the ones that look like LLM calls, and records
them like any other capture. The agent is not modified, restarted or reconfigured.

```bash
cd ebpf && go build -o bin/tokprof-ebpf ./cmd/tokprof-ebpf     # Go 1.25+; the compiled BPF object is committed, clang is not needed
tokprof doctor                                                    # checks kernel BTF/uprobes and finds the helper
tokprof ebpf --pid 1234 --report                                  # by PID
tokprof ebpf --comm node,python --out run.jsonl                   # by name; new matching processes are attached automatically
```

- **Privileges:** loading eBPF needs root or `CAP_BPF` + `CAP_PERFMON` + `CAP_SYS_RESOURCE`. `tokprof ebpf` runs the helper through
  `sudo` (a prompt on a terminal, `sudo -n` otherwise). `tokprof-ebpf inspect` needs none.
- **Scope is explicit:** you must pass `--pid`, `--comm` or `--all`. `--all` records every LLM call it can parse on the machine.
- **Privacy:** the helper emits only the request path, host, and the JSON bodies of POSTs that look like LLM calls. Query strings and
  all headers other than the few used for framing (so no `Authorization`) are dropped inside the helper; the profiler then stores only
  counts, hashes and names as usual.
- **Cleanup:** probes are file descriptors owned by the helper process, so the kernel removes them when the helper exits, even if it
  is killed. Closing the stdin of `tokprof-ebpf trace --exit-on-stdin-eof` (what `tokprof ebpf` does) makes it detach and exit cleanly.
- **Works with:** OpenSSL-compatible libraries that export `SSL_write`/`SSL_read` (system Python, curl, Node.js) and statically linked
  builds that keep those symbols in `.symtab` (for example `uv`-managed Python). Verified on Linux x86-64, kernel 6.18. An arm64 object is built from the same program and checked by CI on an arm64 runner.
- **HTTP/2:** followed (HPACK, interleaved streams, SSE) for connections that were opened **after** the tap attached, because the
  header-compression state of an existing connection is unknown. A connection already in progress is counted as `non_http`.
- **Does not capture:** Go `crypto/tls`, Java, Rust with rustls, processes in another mount namespace,
  responses compressed with `br`/`zstd` (the response is then estimated locally), and very short-lived processes that finish before the
  helper attaches (it rescans every second; use `tokprof run` for those). 
- **Overhead (measured, WSL2):** 3 × 3,000 keep-alive HTTPS POSTs with 4 KiB bodies: mean 0.378 ms without the tap, 0.373 ms with
  it, 100% of calls captured, no kernel drops. Reproduce with `python3 ebpf/e2e/bench.py --helper ebpf/bin/tokprof-ebpf --sudo`.
- **Killed helper:** `python3 ebpf/e2e/crash_cleanup.py --helper ebpf/bin/tokprof-ebpf --sudo` kills the helper with SIGKILL and checks with
  `tokprof-ebpf status` that the kernel holds no tokprof programs afterwards (measured: 5 while running, 0 after).
- **Verify:** `python3 ebpf/e2e/run_e2e.py --helper ebpf/bin/tokprof-ebpf --sudo` starts a local HTTPS server and clients (system Python,
  uv Python, Node `fetch`, curl; HTTP/1.1 and HTTP/2; JSON, SSE, a 144 KB body), attaches, and compares every captured exchange with what was sent
  (add `--tokprof` to go through `tokprof ebpf`).

Rebuilding the BPF object after editing `ebpf/bpf/tls.c` needs clang and libbpf headers: `cd ebpf/internal/tap && go generate ./...`.

The helper can also be used directly: `tokprof-ebpf trace --pid N` prints one JSON object per exchange on stdout.

## Windows, macOS and other systems: `tokprof mitm`

There is no eBPF outside Linux, so `tokprof mitm` uses [mitmproxy](https://mitmproxy.org/)'s **local capture**, which redirects the
traffic of the processes you name at the operating-system level (WinDivert on Windows, a Network Extension on macOS). The agent is
not changed. Install mitmproxy outside your project, then:

```bash
uv tool install mitmproxy                     # or: pipx install mitmproxy
tokprof mitm --trust-help                     # how a process can trust the mitmproxy CA (nothing is installed for you)
tokprof mitm --local node,python --report     # administrator rights on Windows; one-time extension approval on macOS
tokprof mitm --mode regular@8080              # any OS: explicit proxy mode, point the agent's HTTPS_PROXY at it
tokprof mitm --mode regular@8080 --duration 300   # stop by itself after 5 minutes
```

- A small mitmproxy addon (`boot/mitm_addon.py`, standard library only) forwards only the request path, request body and response body
  to the profiler. Responses are streamed through untouched, so streaming clients keep their latency.
- TLS interception needs the process to trust the mitmproxy CA. Prefer per-process variables (`NODE_EXTRA_CA_CERTS`,
  `REQUESTS_CA_BUNDLE`, `SSL_CERT_FILE`) over a system-wide install.
- **Verified:** the addon and the command with real mitmproxy 12.3 in regular proxy mode (plain HTTP and TLS, gzip responses, SSE
  pass-through) on Linux, and in CI on Windows, macOS and Linux (regular mode). **Not verified:** `--local` capture itself on Windows
  and macOS, which needs administrator rights or a system-extension approval that CI cannot give; it is mitmproxy's own feature.
