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
tokprof start                          # start background profiling (on Linux, auto-attaches eBPF to running processes & Docker)
tokprof start --comm fdai              # scope background profiling to matching processes, working directories, or Docker containers
tokprof status                         # check active session and recorded call count
tokprof stop                           # stop session, print visual terminal report, and write .tokprof/latest.html
tokprof -- python my_agent.py          # one-step command: run, print visual report, and write .tokprof/latest.html
tokprof profile --comm python,node     # attach to already-running processes in the foreground
tokprof report                         # summarize the latest run in .tokprof/ (no path needed)
tokprof report --format pretty --html  # rich terminal view + self-contained HTML dashboard (.tokprof/latest.html)
tokprof report --watch                 # live-updating terminal dashboard as calls arrive
tokprof run -- python my_agent.py      # run a command with LLM base URLs routed through the local gateway
tokprof shell                          # a shell whose child processes are profiled
tokprof report run.jsonl --prices prices.json   # add a cost estimate (per 1M tokens, your prices)
tokprof compare before.jsonl after.jsonl        # per-request change with a 95% bootstrap interval
tokprof export run.jsonl --out legacy.jsonl     # schema of examples/token_report.py
tokprof doctor                         # what works on this machine
tokprof selftest                       # verify the pipeline with a local mock LLM
```

`tokprof run` starts a gateway on `127.0.0.1` and sets `OPENAI_BASE_URL`, `ANTHROPIC_BASE_URL`, and
`AZURE_OPENAI_ENDPOINT` (when already set) **for that command only**. Existing values are kept as the upstream.
Use `--map ENV_VAR=UPSTREAM_URL` for other base-URL variables.

## What is recorded

Token counts, salted hashes, text lengths, section and tool names, usage reported by the provider, timings, and
secret-redacted prompt/response segment texts (so you can inspect the exact system prompt, conversation history,
current user prompt, tools/schema, and model output per session and per turn; pass `--no-content` to store only
counts and hashes). **Never recorded:** HTTP headers or API keys. Files are created with mode `0600`.

## How the breakdown is computed

Providers report only totals. `tokprof` splits each request into segments, counts each locally, and shows the
difference from the reported input as **Unattributed** (message template overhead, images, hidden tokens).
Counts are estimates; install `tiktoken` for OpenAI-family accuracy. Large system prompts are split at
markdown headings and XML-style tags, and `--component NAME=@file` attributes a known static text by name.

## Capture modes

- **Gateway:** child processes get `OPENAI_BASE_URL`, `ANTHROPIC_BASE_URL` (and `AZURE_OPENAI_ENDPOINT` when set) pointing at
  `127.0.0.1`. Works for any language whose client honors those variables. Disable with `--no-gateway-env`.
- **In-process hooks:** `PYTHONPATH` gets a `sitecustomize` that wraps `httpx`, `requests`, `urllib3`, `aiohttp` and `urllib.request`; `NODE_OPTIONS` gets a `--require` shim
  that wraps global `fetch`, `http.request` and `https.request` (covering `axios` and custom agents). Both copy request and response bodies to the local gateway and never modify traffic. Requests that
  already go through the gateway are not recorded twice. Disable with `--no-hooks`. Python's `-I` (isolated) mode ignores
  `PYTHONPATH`, so hooks do not load there.
- The gateway honors `HTTPS_PROXY` / `NO_PROXY` for its upstream connection (HTTP proxies via CONNECT).

## How an LLM call is recognized (any host, any path)

A fixed list of URL paths misses most real deployments, so `tokprof` scores independent evidence and keeps the reasons.
The same decision is made for every capture mode (gateway, hooks, eBPF, mitmproxy); the hooks and helpers only pre-filter.

| Evidence | Examples | Weight |
|----------|----------|--------|
| **Body shape** (independent of host and path) | `messages` list of role/content objects, Gemini `contents`/`parts`, a Responses-style `input` with `instructions`/`previous_response_id`, … | strong (60) |
| Ambiguous body | a bare `prompt`, `inputs` (TGI), `input` | weak (25-35): needs a second signal |
| **Path hint** | `/chat/completions`, `/responses`, `/messages`, `/generate`, `/api/chat`, `/api/generate`, `:generateContent`; prefixes, trailing slash, Azure `/deployments/<name>/` are fine | +25 |
| **Host / port hint** (never enough alone) | known provider domains, loopback, RFC 1918, `.internal`, `.svc.cluster.local`, `privatelink`, bare service names, `vllm`/`ollama` in the name, ports 8000/11434/8080/4000/… | +5 to +15 |
| **Response** | usage counters, `choices`, `candidates`, content blocks, Ollama/llama.cpp/TGI counters; JSON, SSE or NDJSON | decides ambiguous bodies |
| Overrides | `--llm-host`, `--llm-path`, `--exclude-host` | force / suppress |

A call is recorded at score >= 50. An ambiguous body (score 25-49) becomes a **candidate**: it is recorded only when its response
looks like a model answer, and an endpoint that proved itself is trusted for later calls to the same host, port and path. Embeddings,
moderation, rerank, tokenization, image/audio generation, files and batches are never recorded.

Understood request shapes: OpenAI Chat, OpenAI Responses, Anthropic Messages, AWS Bedrock Converse (`/converse`, `/converse-stream`, `/invoke`, `/invoke-with-response-stream`), Cohere Chat v1 & v2 (`/v1/chat`, `/v2/chat`), text completions (`prompt` / `inputs`: vLLM `/v1/completions`,
TGI `/generate`, llama.cpp, Ollama `/api/generate`; a prompt already rendered with ChatML, Llama 2/3, Mistral, Gemma, DeepSeek, Phi-3/4, or Cohere Command-R templates is split
into system / history / current turns, otherwise it is reported as one **raw prompt**), Ollama `/api/chat`, Gemini `contents`.
Response usage formats: OpenAI, Anthropic, Gemini `usageMetadata`, AWS Bedrock (`usage.inputTokens`, `metadata.usage`, `amazon-bedrock-invocationMetrics`), Cohere (`meta.tokens`/`billed_units`, `usage.tokens`/`billed_units`), Ollama `prompt_eval_count`/`eval_count`, llama.cpp `tokens_evaluated`,
TGI `details.prefill`; stream formats SSE, NDJSON, JSON arrays, and AWS EventStream (`application/vnd.amazon.eventstream` binary framing). The model name is taken from the URL when the body has none
(Azure deployments, Gemini, Bedrock). Pass `--tokenizer FILE_OR_NAME` (or `TOKPROF_TOKENIZER`) to count segments with a local HuggingFace `tokenizer.json` or pretrained model tokenizer.

```bash
tokprof explain http://vllm.llm.svc.cluster.local:8000/v1/chat/completions --body request.json   # why would this be (not) recorded?
tokprof run --llm-host 'gpu*.corp' --llm-host 10.20.0.0/16 --llm-path '/internal/ask$' -- python agent.py
tokprof run --exclude-host telemetry.corp -- python agent.py
```

At the end of a run the detected endpoints are listed (host name, class such as `private-dns`, API, call count, and why); the JSONL file keeps
only the class and a salted hash of `host:port` unless you pass `--record-hosts`. JSON POSTs that looked LLM-like but whose responses did not
are counted and reported with the flag that forces them if they are model endpoints after all.

### Private and on-prem endpoints

- **Base-URL variables are found by pattern**, not only `OPENAI_BASE_URL`: names containing `OPENAI`, `LLM`, `VLLM`, `OLLAMA`, `LITELLM`, `GEMINI`,
  `MISTRAL`, `INFERENCE`, ... and ending in `_BASE_URL`, `_API_BASE`, `_API_URL`, `_ENDPOINT`, `_URL` (and `OLLAMA_HOST`) are routed through the
  gateway too, and the start-up line lists them. Names that mention downloads, databases, telemetry and similar are skipped.
  Disable with `--no-auto-env`; add anything else with `--map NAME=URL`.
- **Private CA:** the gateway makes the upstream TLS connection for the client, so it trusts `SSL_CERT_FILE`, `REQUESTS_CA_BUNDLE`,
  `CURL_CA_BUNDLE`, `NODE_EXTRA_CA_CERTS` and `--upstream-ca FILE`. A refused certificate produces a 502 that says so. `--insecure-upstream`
  disables verification (prints a warning); use it only as a last resort.
- DNS is resolved by the gateway exactly as the client would (system resolver, `/etc/hosts`, split-horizon, `privatelink` zones); `HTTPS_PROXY`
  and `NO_PROXY` are honored.
- Hooks and eBPF see the URL the client really used, so any host name works without configuration.

## Limits (today)

- Outside the eBPF tap, only processes started through `tokprof` (`run`, `shell`, or the environment from `tokprof env`) are captured.
  Already-running processes need the Linux eBPF tap or `tokprof mitm` (see below).
- Under `tokprof run`, clients that neither honor the base-URL variables nor use `httpx`, `requests`, `urllib3`, `aiohttp`, `urllib.request`,
  Node `fetch` or Node `http`/`https` (for example raw `http.client` or compiled non-Python/Node binaries that hard-code their endpoint)
  are not captured by the gateway or hooks. `tokprof ebpf` (Linux) and `tokprof mitm` see them at the network level.
- Non-seekable streaming request bodies (for example a generator passed as `data=`) are not captured by in-process hooks.
- The gateway opens a new upstream TLS connection per request and asks upstream for uncompressed responses.

## Develop

```bash
uv venv && uv pip install -e ".[dev]"
pytest
tokprof selftest
```

## Linux eBPF tap (running processes, no restart, no code change)

`tokprof ebpf` watches processes that are **already running** and records their LLM calls like any other capture. The agent is not
modified, restarted or reconfigured. Two probe sets cover both kinds of traffic:

- **HTTPS:** uprobes on OpenSSL-compatible TLS libraries (`SSL_write`/`SSL_read` and the `_ex` variants) and Go's `crypto/tls`
  (`crypto/tls.(*Conn).Write` and `crypto/tls.(*Conn).Read`, using entry + `RET`-offset uprobes rather than `uretprobe` so goroutine stack
  growth is never disturbed) read the plaintext.
- **Plain `http://`** (for example an on-prem vLLM on `http://gpu-box:8000` or cleartext HTTP/2 `h2c`): no TLS library is involved, so the
  `write`, `writev`, `sendto`, `sendmsg`, `read`, `readv`, `recvfrom`, `recvmsg` and `close` syscalls of the selected processes are observed
  via raw tracepoints. A socket is copied only after a write that starts with `POST ` or the HTTP/2 `PRI *` preface has marked it, and HTTP/1.1
  sockets are unmarked when the exchange is complete. Disable with `--no-plain`.

The helper rebuilds HTTP/1.1 and HTTP/2 (both TLS and `h2c`) exchanges, decodes `gzip`, `deflate`, `br` and `zstd` bodies, and keeps the ones that look like LLM calls.

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
- **Works with:** plain HTTP/1.1 and `h2c` from any language whose runtime uses those syscalls (verified: system and `uv` Python, Node.js `fetch` and `http2`,
  Go `net/http`, curl; JSON, SSE and a 144 KB body), OpenSSL-compatible libraries that export `SSL_write`/`SSL_read` (system Python, curl, Node.js) or keep
  them in `.symtab` (`uv`-managed Python), and both unstripped and stripped (`-ldflags="-s -w"`, via `.gopclntab`) Go binaries using `crypto/tls` (`go-tls`). Processes in another mount namespace (such as Docker/Kubernetes containers) are resolved through `/proc/<pid>/root`, and forked child processes of watched PIDs are automatically watched in the kernel. Verified on Linux x86-64 (kernel 6.18) and arm64 (CI runner, kernel 7.0).
- **HTTP/2:** followed (HPACK, interleaved streams, SSE, both TLS and `h2c`) for connections that were opened **after** the tap attached, because the
  header-compression state of an existing connection is unknown. A connection already in progress is counted as `non_http`.
- **Does not capture:** Java TLS, Rust with rustls (their plain HTTP is captured), the *server* side of a connection
  (attach to the clients), more than 8 `iovec` buffers per vector syscall,
  and very short-lived TLS processes that finish before the helper attaches (use `tokprof -- <cmd>` for those).
- **Overhead (measured, WSL2):** 3 × 3,000 keep-alive HTTPS POSTs with 4 KiB bodies: mean 0.378 ms without the tap, 0.373 ms with
  it, 100% of calls captured, no kernel drops (`python3 ebpf/e2e/bench.py --helper ebpf/bin/tokprof-ebpf --sudo`). The plain-HTTP syscall probes run on
  every `read`/`write` of every process while the helper is attached: a 64-byte `write` to `/dev/null` went from 119 ns to 146 ns in a process that is not
  watched, and to about 256 ns with `--all` (`--no-plain` removes this cost). The probes are raw tracepoints: per-syscall perf tracepoints were
  skipped at random on a busy machine (a per-CPU re-entrancy guard), which lost individual writes.
- **Killed helper:** `python3 ebpf/e2e/crash_cleanup.py --helper ebpf/bin/tokprof-ebpf --sudo` kills the helper with SIGKILL and checks with
  `tokprof-ebpf status` that the kernel holds no tokprof programs afterwards (measured: 16 while running, 0 after).
- **Verify:** `python3 ebpf/e2e/run_e2e.py --helper ebpf/bin/tokprof-ebpf --sudo` starts a local HTTPS server and clients (system Python,
  uv Python, Node `fetch`, curl, Go; HTTPS and plain HTTP; HTTP/1.1 and HTTP/2; JSON, SSE, a 144 KB body), attaches, and compares every captured exchange with what was sent
  (add `--tokprof` to go through `tokprof ebpf`).

Rebuilding the BPF object after editing `ebpf/bpf/tls.c` needs clang and libbpf headers: `cd ebpf/internal/tap && go generate ./...`.

The helper can also be used directly: `tokprof-ebpf trace --pid N` prints one JSON object per exchange on stdout. Its pre-filter forwards only
POSTs whose JSON body has a `messages`/`contents` list or a `prompt`/`input`/`inputs` key; the decision is made by the profiler (see above).

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
