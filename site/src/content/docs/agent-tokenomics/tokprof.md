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
| **Azure Application Insights / Log Analytics** | `tokprof pull --workspace <ID>`<br>`tokprof pull --app-insights <ID>`<br>`tokprof import <file>` | Any OS & **Azure Cloud Shell** | **No** (reads telemetry) | Queries `AppDependencies` + `AppTraces` (OpenTelemetry GenAI, OpenInference, APIM, or `[TOKPROF_CHUNK]` logs), reassembles prompts/tools, and runs full 7-segment attribution or usage-only fallback. |

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

## 6. Profiling agents already deployed on Azure (Cloud Shell & Application Insights)

When an agent is already running on Azure, host-level eBPF (`CAP_BPF`) is only available on Linux VMs and privileged AKS nodes. For managed PaaS and serverless services, `tokprof` provides `tokprof pull`, `tokprof import`, and a lossless chunked log bridge.

### Feasibility matrix by Azure hosting target

| Azure hosting target | `tokprof ebpf` | `tokprof run` / hooks | `tokprof pull` / `import` (App Insights) | Recommended method |
|----------------------|----------------|-----------------------|------------------------------------------|--------------------|
| **Azure VM / VMSS (Linux)** | **Yes** | **Yes** | **Yes** | SSH/Bastion + `sudo tokprof start` (zero restart) |
| **AKS (Kubernetes)** | **Conditional** (needs `CAP_BPF` + `/sys/kernel/btf`) | **Yes** | **Yes** | Privileged debug container/DaemonSet, sidecar gateway, or `tokprof pull` |
| **Azure App Service / Container Apps (ACA)** | **No** (sandboxed kernel) | **Yes** (startup command or `PYTHONPATH` / `NODE_OPTIONS`) | **Yes** | `tokprof pull` from Application Insights, or `TOKPROF_STDOUT_LOG=1` hook |
| **Azure Functions** | **No** | **Hooks only** (`PYTHONPATH` / `NODE_OPTIONS`) | **Yes** | `tokprof pull` from Application Insights or `TOKPROF_STDOUT_LOG=1` |
| **Azure AI Foundry Hosted Agent** | **No** (managed runtime) | **No** | **Yes** | `tokprof pull` against the Foundry project's Application Insights / Log Analytics workspace |

### Running `tokprof` in Azure Cloud Shell

Because `tokprof` ships as a static `manylinux2014_x86_64` binary wheel on PyPI, it installs in Azure Cloud Shell in seconds without a Rust or Go toolchain and persists across sessions in `$HOME`:

```bash
# Install in Azure Cloud Shell
pip install --user tokprof
export PATH="$HOME/.local/bin:$PATH"

# Pull the last 6 hours of LLM calls from Log Analytics or Application Insights
# (uses your active Cloud Shell `az` login automatically)
tokprof pull --workspace <LOG_ANALYTICS_WORKSPACE_ID> --since 6h
tokprof pull --app-insights <APP_INSIGHTS_APP_ID> --since 2h --conv <OPERATION_ID>

# Print the underlying KQL query to run in Azure Portal Logs
tokprof pull --print-kql --since 24h

# Import an exported JSON, NDJSON, or container log file offline
tokprof import appinsights_export.json --report
```

When `tokprof` detects Azure Cloud Shell (`CLOUD_SHELL=true` or `ACC_CLOUD`), its HTML report server automatically binds to Cloud Shell Web Preview ports (`8080`, `8000–8090`, or `8888`) instead of `18765`, and prints instructions to open **[Web Preview] → [Configure] → Port 8080** or run `download .tokprof/latest.html`.

### Application Insights collection tiers & lossless bridge

| Telemetry mode | Segment breakdown (`system`, `tool_def`, `history_*`, `user_prompt`) | How to enable |
|----------------|----------------------------------------------------------------------|---------------|
| **Tier 1 — Full OTel GenAI Content Recording** | **Yes (7 segments + `sysfp`)** | Set `AZURE_TRACING_GEN_AI_CONTENT_RECORDING_ENABLED=true` or `OTEL_INSTRUMENTATION_GENAI_CAPTURE_MESSAGE_CONTENT=true` on the agent so `AppTraces` records `gen_ai.*.message` and `gen_ai.choice` events. |
| **Tier 2 — Usage-Only Fallback (default OTel)** | **No (totals, cost, latency, and turn curve only)** | Works with default `gen_ai.usage.*` attributes; `tokprof` marks calls as `usage_only` and adds a diagnostic banner explaining how to enable content recording. |
| **Tier 3 — Lossless Chunked Bridge (>32 KB prompts)** | **Yes (100% lossless, bypasses 32 KB App Insights limit)** | Set `TOKPROF_STDOUT_LOG=1` with `tokprof`'s Python/Node hooks (`PYTHONPATH` / `NODE_OPTIONS`). Hooks emit gzip+base64 `[TOKPROF_CHUNK]` lines (20 KB each) to container logs, which `tokprof pull` / `import` reassembles automatically. |

### Production operational safeguards: RBAC, LAW ingestion lag, and Cloud Shell 30m timeout

| Operational issue | Built-in `tokprof` safeguard | CLI flag / usage |
|-------------------|------------------------------|------------------|
| **No workspace RBAC (resource-only permission)** | Resource-centric query via `/v1/subscriptions/.../query` when the user only has `Monitoring Reader` on the Application Insights resource | `tokprof pull --resource-id /subscriptions/<SUB>/.../components/<APP>` |
| **Table-level RBAC blocking `AppTraces`** | Automatically detects `InsufficientAccessError` / `403` on `AppTraces` and falls back to an `AppDependencies`-only query (`usage_only` mode) | Automatic on `tokprof pull` |
| **Subscription / tenant mismatch or missing RBAC** | Classifies `401` / `403` / `404` (`InvalidAuthenticationTokenTenant`, `InsufficientAccessError`) and prints exact `az account set` / `az role assignment create` commands | Automatic on `tokprof pull` |
| **Log Analytics ingestion latency (2–15m) & split arrival** | Tracks `IngestionTime - TimeGenerated` lag, polls until late `AppTraces` events arrive, and upgrades `usage_only` spans to full 7-segment `tier1_full` in-place | `tokprof pull --workspace <ID> --wait-ingest 5m --poll-interval 15s` |
| **Azure Cloud Shell 20–30m session timeout** | Persists `.tokprof/pull-checkpoint.json` and recordings under `~/clouddrive/.tokprof` (Azure Files mount), caps waits at 25m, and resumes cleanly after disconnect | `tokprof pull --resume` |
| **HTTP 429 rate limiting & Sovereign Clouds** | Exponential backoff honoring `Retry-After` / `x-ms-retry-after-ms`, plus Sovereign Cloud endpoints (`api.loganalytics.us`, `api.loganalytics.azure.cn`) | `tokprof pull --cloud {public,usgov,china}` |
| **Multi-replica duplicate spans & Azure secrets** | Deduplicates spans by `(OperationId, SpanId)` across replicas/retries and redacts Azure `AccountKey=`, `SharedAccessKey=`, and SAS `sig=` tokens | Automatic |

#### Step-by-step production examples (RBAC, LAW latency, Cloud Shell timeout, Portal CSV)

```bash
# 1) Resource-centric RBAC: when you have Monitoring Reader on the App Insights resource
#    but NO permission on the shared Log Analytics Workspace
tokprof pull \
  --resource-id /subscriptions/00000000-0000-0000-0000-000000000000/resourceGroups/rg-agent/providers/Microsoft.Insights/components/my-agent-ai \
  --since 6h

# 2) Log Analytics ingestion latency (2–15m): poll until delayed AppTraces message events arrive
#    and automatically upgrade usage-only spans to full 7-segment breakdowns in-place
tokprof pull --workspace <WORKSPACE_ID> --since 30m --wait-ingest 5m --poll-interval 15s

# 3) Azure Cloud Shell 30m session timeout recovery: resume from ~/clouddrive/.tokprof/pull-checkpoint.json
#    using an incremental KQL timestamp watermark
tokprof pull --resume

# 4) Offline import from an Azure Portal CSV export (when CLI token access is blocked by Conditional Access)
tokprof import portal_query_export.csv --out .tokprof/portal.jsonl --report

# 5) Sovereign Cloud (Azure US Government or Azure China)
tokprof pull --workspace <WORKSPACE_ID> --cloud usgov --since 24h
```

### Handling oversized logs, controlling prompt inclusion, and verifying output

#### What happens when logs or prompts are very large?

`tokprof` applies four layered safeguards so oversized prompts or multi-hundred-thousand-row workspaces never break token attribution:

| Layer | Limit / Threshold | Behavior when exceeded | Impact on token counts & hashes |
|-------|-------------------|------------------------|---------------------------------|
| **1. Gateway & Hook Capture** | Gateway: **64 MB** (`MAX_BODY`)<br>Python/Node hooks: **32 MB** (`_CAP`) | Captures full request and streaming SSE response bodies up to 32–64 MB without dropping frames. | **100% exact** |
| **2. Per-Segment Stored Preview Cap** | **32,768 chars** per segment (`MAX_SEGMENT_TEXT_CHARS`) | Token count (`tokens`), character length (`chars`), and salted SHA-256 fingerprint (`hash`) are computed on the **entire original text first** before capping the stored preview string (`text`) at 32,768 chars with `... [truncated, N chars total]`. | **100% exact** (tokenization happens before preview truncation) |
| **3. App Insights 32 KB Property Limit** | **32 KB (32,768 B)** per `Properties` / `Message` column | • Single-pass `parse_or_repair_json` repairs JSON cut off mid-string/object and marks `... [truncated by App Insights]`.<br>• Setting `TOKPROF_STDOUT_LOG=1` enables the gzip+base64 `[TOKPROF_CHUNK]` bridge (20 KB per line), recovering **100 KB+ prompts with 0% loss**. | Unrepaired tail is attributed to `residual`; **100% exact** with chunk bridge |
| **4. Log Analytics Query Result Limit** | **500,000 rows** or **64 MB** (`ResponsePayloadTooLarge`) | • Requests `Accept-Encoding: gzip, deflate, br` (reducing transfer size by ~97.7%).<br>• On `ResponsePayloadTooLarge`, automatically retries with `\| take 20000` and falls back to `AppDependencies`-only.<br>• Supports explicit `--limit <ROWS>` and `--usage-only`. | Safe fallback prevents query failure |

#### Three modes for including or excluding prompts

You can choose whether prompt/response texts are fetched and stored at collection time (`pull` / `start` / `run`), import time (`import`), or report time (`report`):

| Mode | CLI flag / Env var | Prompt text stored on disk? | 7-segment breakdown & `sysfp` hash? | Measured artifact size (200 calls / 1,400 segments) |
|------|--------------------|-----------------------------|-------------------------------------|-----------------------------------------------------|
| **1. Full Prompts (`full-prompts`)** | *(default)*<br>Terminal untruncated view: `--full-prompts` | **Yes** (secrets auto-redacted) | **Yes (7 segments + `sysfp`)** | JSONL: `1.50 MB`<br>HTML: `1.90 MB`<br>CSV: `1.17 MB` |
| **2. Segment Counts Only (`no-content`)** | `--no-content` or `TOKPROF_NO_CONTENT=1` | **No (`0 chars`)** | **Yes (100% identical tokens & `sysfp` hashes)** | JSONL: `384 KB` (**-74.4%**)<br>HTML: `245 KB` (**-87.1%**)<br>CSV: `25.6 KB` (**-97.8%**) |
| **3. Totals Only (`usage-only`)** | `tokprof pull --usage-only`<br>`tokprof import --usage-only` | **No (`0 chars`, `AppTraces` not queried)** | **No** (total input/output, cost, latency, and turn curve only) | JSONL: `138 KB` (**-90.8%**)<br>HTML: `186 KB` (**-90.2%**)<br>CSV: `27.8 KB` (**-97.6%**) |

```bash
# Mode 1: Keep redacted prompt & response texts (default) and inspect full text in terminal
tokprof pull --workspace <WORKSPACE_ID> --since 6h
tokprof report --full-prompts

# Mode 2: Compute full 7-segment token breakdown & cache hashes, but omit all prompt/response text
tokprof pull --workspace <WORKSPACE_ID> --since 6h --no-content
tokprof import appinsights_export.json --no-content --out .tokprof/no-content.jsonl

# Strip prompt texts from an ALREADY recorded full JSONL file when generating shareable HTML/CSV/JSONL
tokprof report .tokprof/full.jsonl --no-content --html shareable.html --csv shareable.csv
tokprof import .tokprof/full.jsonl --no-content --out .tokprof/stripped.jsonl

# Mode 3: Skip AppTraces completely on huge workspaces (query only AppDependencies token totals + row cap)
tokprof pull --workspace <WORKSPACE_ID> --since 7d --usage-only --limit 50000
```

#### How to verify whether prompts were included, omitted, or truncated

1. **Automatic `content verification` summary on `stderr`:** Every `tokprof pull` and `tokprof import` run inspects the resulting records and prints an exact verification line:
   ```text
   # Default (full-prompts):
   tokprof: content verification — mode=full-prompts, segments_with_text=1400/1400, stored_prompt_chars=1000580, stored_response_chars=74890, 32k_preview_capped=0, appinsights_32kb_repaired=0

   # With --no-content (proves 0 prompt/response chars are stored while all 1,400 segments are preserved):
   tokprof: content verification — mode=no-content, segments_with_text=0/1400, stored_prompt_chars=0, stored_response_chars=0, 32k_preview_capped=0, appinsights_32kb_repaired=0

   # With --usage-only:
   tokprof: content verification — mode=usage-only, segments_with_text=0/0, stored_prompt_chars=0, stored_response_chars=0, 32k_preview_capped=0, appinsights_32kb_repaired=0
   ```
2. **Direct JSONL audit with `jq`:** Run this one-liner against any `.tokprof/*.jsonl` file to verify that no prompt text (`text` / `response_text`) exists or to count 32 KB truncations:
   ```bash
   jq -s '{
     calls: length,
     total_segments: ([.[].segments | length] | add),
     segments_with_text: ([.[].segments[] | select(.text != null)] | length),
     responses_with_text: ([.[] | select(.response_text != null)] | length),
     appinsights_32kb_repaired: ([.[] | select(.detect.truncated_repaired == true)] | length),
     preview_32k_capped: ([.[].segments[] | select(.text != null and (.text | contains("[truncated,")))] | length)
   }' .tokprof/no-content.jsonl
   ```

---

## 7. CLI command reference

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
| `tokprof pull (--workspace ID \| --app-insights ID \| --resource-id ARM_ID) [flags]` | Pull and decompose LLM calls from Azure Application Insights / Log Analytics (`--since`, `--wait-ingest`, `--resume`, `--no-content`, `--usage-only`, `--limit`, `--cloud`). |
| `tokprof import <file> [--no-content] [--usage-only] [--report]` | Convert exported App Insights JSON/CSV, OTel GenAI, APIM, or `[TOKPROF_CHUNK]` logs into JSONL. |
| `tokprof explain <url> [--body FILE]` | Explain how a URL and request body are scored for LLM detection. |
| `tokprof export <in.jsonl> --out <out.jsonl>` | Convert a recording to the compact `examples/token_report.py` schema. |
| `tokprof doctor` | Check local tokenizers, kernel eBPF support, and helper binaries. |
| `tokprof selftest` | Run an end-to-end self-test against a local mock LLM server. |

## Sources

- PyPI package: [`tokprof` on PyPI](https://pypi.org/project/tokprof/) (verified 2026-11-06)
- Source code & architecture: [`profiler/` in `dotnetpower/agent-tokenomics`](https://github.com/dotnetpower/agent-tokenomics/tree/main/profiler)
