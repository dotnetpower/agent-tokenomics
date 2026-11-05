---
title: "tokprof: Install & Usage"
description: Install tokprof from PyPI and profile LLM agent token usage per session, turn, and call without changing agent code.
track: agent
verified: 2026-11-06
sidebar:
  order: 6
---

[`tokprof`](https://pypi.org/project/tokprof/) is a standalone native Rust single-binary profiler that decomposes the **input tokens of every LLM call** into system prompt, conversation history, current user prompt, and tool-related content (definitions, calls, results, response schemas) — without modifying agent code and without adding dependencies to your agent's project.

## 1. Install

`tokprof` is published on [PyPI](https://pypi.org/project/tokprof/) with prebuilt native wheels for Linux (`x86_64`, `aarch64`), macOS (`arm64`, `x86_64`), and Windows (`x64`). You do **not** need a Rust or Go toolchain installed.

```bash
# Install globally as an isolated CLI tool (recommended)
uv tool install tokprof
# or
pipx install tokprof

# Or install with pip in any Python environment
pip install tokprof

# Or run immediately without installing
uvx tokprof --help
```

### Prebuilt platform packages

| Platform | Wheel / Package | Included binaries |
|----------|-----------------|-------------------|
| Linux `x86_64` & `aarch64` | `manylinux2014` wheel | `tokprof` + static `tokprof-ebpf` helper |
| macOS Apple Silicon (`arm64`) & Intel (`x86_64`) | `macosx` wheel | `tokprof` |
| Windows `x64` (`amd64`) | `win_amd64` wheel | `tokprof.exe` |
| Source distribution (`sdist`) / Cargo | `tokprof-*.tar.gz` | Builds from Rust source (`cargo install --path ./profiler`) |

Verify your installation and check which capture modes your machine supports:

```bash
tokprof --version
tokprof doctor      # checks tokenizers, Linux eBPF kernel BTF/uprobes, mitmproxy, and Node
tokprof selftest    # runs a local mock LLM through the full capture & decomposition pipeline
```

For full CLI help and examples in English or Korean:

```bash
tokprof --help            # English (default)
tokprof --lang ko         # Korean (한국어 상세 도움말 및 예시)
```

---

## 2. Quick start workflows

### Workflow A — Profile already-running services & Docker containers (`start` / `status` / `stop`)

On Linux, `tokprof start` starts a background session and automatically attaches the eBPF probe to running host processes and Docker containers (when root or passwordless `sudo` is available):

```bash
# 1. Start background profiling (or filter with: tokprof start --comm my_agent)
tokprof start

# 2. Trigger the agent workflow, API endpoint, or test you want to measure
pytest tests/test_agent.py

# 3. Check captured call count while the session is active
tokprof status

# 4. Stop the session, print the terminal breakdown, and write .tokprof/latest.html
tokprof stop
```

> **Tip (non-Linux or without root/eBPF):** When eBPF is unavailable, `tokprof start` prints `eval "$(tokprof env)"`. Run that in the terminal where you launch your agent so child processes route through the local gateway.

### Workflow B — Profile a single command in one step (`tokprof -- <cmd>`)

On Linux, macOS, and Windows, pass any command after `--` to run it through the local gateway and in-process hooks, print the visual report on exit, and generate `.tokprof/latest.html`:

```bash
tokprof -- python my_agent.py
tokprof -- pytest tests/
tokprof -- node agent.js
```

### Workflow C — Interactive subshell (`tokprof shell`)

Open a subshell where every command you run is profiled into one session:

```bash
tokprof shell
# run multiple agent commands inside the subshell, then exit:
exit
```

---

## 3. Capture modes

| Mode | Command | OS | Requires restart? | How it works |
|------|---------|----|-------------------|--------------|
| **Gateway + In-process hooks** | `tokprof -- <cmd>`<br>`tokprof run -- <cmd>`<br>`tokprof shell` | Linux, macOS, Windows | Yes (starts child process) | Routes `OPENAI_BASE_URL`, `ANTHROPIC_BASE_URL`, `AZURE_OPENAI_ENDPOINT`, and pattern-matched `*_BASE_URL` / `*_ENDPOINT` variables through `127.0.0.1`, plus Python (`httpx`, `requests`, `urllib3`, `aiohttp`, `urllib.request`) and Node (`fetch`, `http`, `https`) hooks for clients that hard-code URLs. |
| **Linux eBPF tap** | `tokprof start`<br>`tokprof ebpf --comm <name>`<br>`tokprof ebpf --pid <PID>` | Linux (x86_64, ARM64) | **No** (attaches live) | Attaches uprobes to OpenSSL and Go `crypto/tls` plus syscall tracepoints for plain `http://` / `h2c` across host processes and Docker containers. Requires root or `CAP_BPF`. |
| **mitmproxy capture** | `tokprof mitm --local <proc>` | Windows, macOS, Linux | **No** (in `--local` mode) | Uses `mitmdump` with `tokprof`'s addon script. Run `tokprof mitm --trust-help` for per-process CA trust instructions. |

---

## 4. Reports, HTML dashboard, CSV, and before/after comparison

Every completed session writes a timestamped JSONL recording and updates `.tokprof/latest.html` (and `.tokprof/latest.csv` when requested).

```bash
# Pretty terminal breakdown of the most recent run in .tokprof/
tokprof report

# Print full, untruncated prompt and response texts in the terminal
tokprof report --full-prompts

# Generate the self-contained HTML dashboard (.tokprof/latest.html) and open it
tokprof report --html --open

# Export per-session, per-turn, and per-call breakdown + prompt texts to CSV
tokprof report --format csv --out run.csv
tokprof report --csv

# Live-refreshing terminal dashboard while an agent is running
tokprof report --watch

# Add cost estimates using your own per-1M-token price table
tokprof report --prices prices.json

# Compare two runs with a 95% bootstrap confidence interval (Stages 3 & 5)
tokprof compare before.jsonl after.jsonl
```

### What the HTML and CLI reports show

1. **Input token breakdown (total):** System prompt (split by markdown headings and XML tags), tool definitions, tool results, tool calls, conversation history (user and assistant turns), current user prompt (including structured JSON payloads with `utterance`, `context`/`history`, and `capabilities`), response schema, and unattributed template overhead.
2. **Per-Session & Per-Turn lineage:** How fixed prefix tokens, history accumulation, cache hits, and fan-out (`calls/turn`) evolve across turns.
3. **Call & Turn Prompt Dossier:** Inspect the exact secret-redacted system prompt, history messages, user prompt, tool schemas, and assistant output for each turn and individual call.
4. **Automated diagnostic findings:** Flags unused tool definitions, repeated identical tool calls or HTTP failure loops, prompt cache-busting (dynamic values in the system prompt), and steep history growth, linking each finding to the matching Track B stage.

---

## 5. Private / on-prem endpoints and privacy

### Private DNS, vLLM, Ollama, TGI, LiteLLM, and custom CAs

`tokprof` scores request body shapes, URL paths, host/port hints, and response usage fields rather than matching a hardcoded domain list.

```bash
# Check why a specific URL and request body is (or is not) classified as an LLM call
tokprof explain http://vllm.internal:8000/v1/chat/completions --body request.json

# Force or exclude specific hosts/paths and trust an internal CA certificate
tokprof start --llm-host 'gpu*.corp' --llm-path '/internal/ask$' --upstream-ca corp-ca.pem
tokprof run --exclude-host telemetry.corp -- python my_agent.py
```

### Privacy & redaction

- **Never recorded:** HTTP headers (`Authorization`, `x-api-key`) or API keys.
- **File permissions:** All `.tokprof/` files are written with mode `0600`.
- **Omit prompt text:** Pass `--no-content` (`tokprof start --no-content` or `tokprof run --no-content -- ...`) to record only token counts, byte lengths, and salted SHA-256 hashes without storing prompt or response text.

---

## 6. CLI command reference

| Command | Description |
|---------|-------------|
| `tokprof start [--comm NAME] [--no-content]` | Start a background profiling session (auto-attaches eBPF on Linux). |
| `tokprof status` | Show the active background session and recorded LLM call count. |
| `tokprof stop` | Stop the active session, print the terminal report, and write `.tokprof/latest.html`. |
| `tokprof -- <cmd> [args...]` | Profile a single command and write the terminal + HTML report. |
| `tokprof profile [--comm NAME \| -- <cmd>]` | Foreground profiling of running processes or a child command. |
| `tokprof report [FILE] [--format FMT] [--html] [--csv]` | Render a recorded JSONL run (`pretty`, `text`, `html`, `csv`, `md`, `json`). |
| `tokprof compare <before.jsonl> <after.jsonl>` | Compare two runs with a 95% bootstrap confidence interval. |
| `tokprof run -- <cmd> [args...]` | Run a command through the local gateway and in-process hooks. |
| `tokprof shell` | Open an interactive shell where child processes are profiled. |
| `tokprof ebpf (--pid PID \| --comm NAME \| --all)` | Attach Linux eBPF probes to already-running processes. |
| `tokprof mitm [--local PROC \| --trust-help]` | Capture traffic via `mitmproxy` on Windows, macOS, or Linux. |
| `tokprof explain <url> [--body FILE]` | Explain how a URL and request body are scored for LLM detection. |
| `tokprof export <in.jsonl> --out <out.jsonl>` | Convert a recording to the compact `examples/token_report.py` schema. |
| `tokprof doctor` | Check local tokenizers, kernel eBPF support, and helper binaries. |
| `tokprof selftest` | Run an end-to-end self-test against a local mock LLM server. |

## Sources

- PyPI package: [`tokprof` on PyPI](https://pypi.org/project/tokprof/) (verified 2026-11-06)
- Source code & architecture: [`profiler/` in `dotnetpower/agent-tokenomics`](https://github.com/dotnetpower/agent-tokenomics/tree/main/profiler)
