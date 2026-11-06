---
title: "cctx: VS Code Copilot Token Saver & Accelerator"
description: "Install Code Context (cctx) from the VS Code Marketplace to cut Copilot lookup tokens, speed up coding tasks, and inspect live AIC/USD cost savings."
track: copilot
verified: 2026-11-06
sidebar:
  order: 11
---

[Code Context (`cctx`)](https://marketplace.visualstudio.com/items?itemName=dotnetpower.code-context-native) is a **VS Code Native Language Model Tools extension** (`dotnetpower.code-context-native`) backed by a shared local Rust trigram index. Installed directly from the VS Code Marketplace, it both **reduces Copilot token/AI Credit consumption** and **accelerates development speed** by replacing multi-step `grep` + full-file `view` loops with single-call, block-clamped context lookups — while surfacing live per-model token, cache-hit, and AIC/USD savings right inside VS Code.

## Problem

When GitHub Copilot Agent Mode explores a repository using built-in search and file-read tools, two compounding costs slow you down and drain AI Credits:

1. **Multi-turn exploration latency (slower development):** Finding a symbol, reading its definition, and checking nearby callers typically takes several `grep` → `view` → `grep` round trips. Each extra LLM turn adds network and prefill latency.
2. **History bloat (higher token cost):** Unclamped file reads (`100–200+` lines or whole documents) enter the conversation history and are **re-sent on every subsequent turn** of the session. Verbose prose agent instructions add another fixed per-turn tax.

---

## Action: Install in VS Code (Marketplace)

`cctx` works as a native VS Code extension (`VS Code 1.140+`) on **Linux, native Windows (`cctx.exe`), WSL, and SSH** — no MCP JSON configuration or proposed API flags are required for VS Code Copilot.

### 1. Install the VS Code extension from Marketplace

Install **[Code Context by dotnetpower](https://marketplace.visualstudio.com/items?itemName=dotnetpower.code-context-native)** from the VS Code Extensions view, or run:

```bash
code --install-extension dotnetpower.code-context-native
```

### 2. Install the local Rust backend (`cctx`)

The VS Code extension talks to a fast local Rust daemon (`cctx`) on your machine:

```bash
# Linux / WSL / macOS / SSH:
git clone https://github.com/dotnetpower/code-context.git
cd code-context && bash scripts/install.sh && ~/.local/bin/cctx ping

# Native Windows (PowerShell, no WSL required):
cargo build --locked --release
copy target\release\cctx.exe $HOME\.local\bin\cctx.exe
```

### 3. Open your project and run Getting Started

Run **Code Context: Getting Started** from the VS Code Command Palette (`Ctrl+Shift+P` / `⌘Shift+P`). On native Windows, `cctx.exe daemon` starts automatically in the background. The extension automatically attaches a compact, BPE-aligned instruction file (`applyTo: "**"`) so Copilot prefers the native tools without manual prompting:

| VS Code Native Tool | What Copilot uses it for | Why it is faster & cheaper |
|---|---|---|
| `#cctxNativeContext` | **Symbol / pattern + nearby code in 1 call** | Replaces separate `grep` + `view` turns; clamps snippets to function/block boundaries within `<8 KiB`. |
| `#cctxNativeGrep` | **Trigram-indexed literal / regex search** | Prioritizes symbol definitions (`max_matches_per_file`) instead of dumping noisy match lists. |
| `#cctxNativeFiles` | **Instant filename & path glob discovery** | Sub-millisecond in-memory path matching (`0.307 ms` warm median). |
| `#cctxNativeRead` | **Hash-verified line-range & anchor reads** | Reads exact line ranges or `#toc` / `#slide-N-M` / `#heading` anchors in `.md`, `.docx`, `.pptx`, and `.pdf` files. |

---

## Why it both saves cost and accelerates development

In the published [8-session, 40-round, 8,960-pair benchmark suite](https://github.com/dotnetpower/code-context/blob/main/docs/BENCHMARKS.md) on `aks-store-demo` and Copilot A/B coding trials (as reported by `dotnetpower/code-context`), `cctx` improves both **developer velocity** and **token efficiency** without reducing answer accuracy:

### 1. Development speed acceleration

- **15.20× faster warm code retrieval** across the mixed retrieval suite (`4.450 ms → 0.293 ms` per operation; 95% CI `14.53–15.84×`; `29.115 ms` cold sync median):
  - Literal search: **20.35× faster** (`5.885 ms → 0.289 ms`)
  - Search + context: **18.70× faster** (`5.935 ms → 0.317 ms`)
  - Filename discovery: **16.03× faster** (`4.923 ms → 0.307 ms`)
- **61.9% fewer tool calls** per coding task (`7.00 → 2.67` calls) and **26.7% fewer LLM round-trip turns** (`5.00 → 3.67` turns), eliminating back-and-forth search loops.
- **24.7% faster end-to-end task completion wall time** (`20.31 s → 15.30 s` at 100% answer accuracy).
- **Nonblocking cold cache:** If a workspace index is still warming up, `cctx` returns `CACHE_MISS` immediately and loads in the background so Copilot never stalls waiting for indexing.

### 2. Token and AI Credit savings

| Benchmark dimension | Baseline (`rg` + `view`) | With `cctx` in VS Code | Reported impact |
|---|---:|---:|---|
| **Rust 6-symbol lookup payload** | 15,028 tok (100L) / 25,133 tok (200L) | **4,031 tok** (6 calls vs 20) | **-73.2% to -84.0% tokens** (**10.3×** faster) |
| **Python 6-symbol lookup payload** | 13,997 tok (100L) / 24,609 tok (200L) | **2,141 tok** (6 calls vs 15) | **-84.7% to -91.3% tokens** (**13.16×** faster) |
| **Rich documents (`.pptx` / `.docx` / `.md`)** | Full-document read | Virtual `#toc` + `#slide-N-M` anchors | **-86.3% to -98.9% tokens** |
| **Auto-loaded agent instructions** | 2,048 tok prose | **756 tok** BPE-aligned formula spec | **-63.1% instruction overhead** |
| **Copilot A/B net session cost** | 6,360 payload tok / 4.76B nanoAIU | **3,177 payload tok / 3.55B nanoAIU** | **-50.0% payload tok · -25.4% AIU cost** |

---

## How to verify: Live usage & cost savings in VS Code

You do not need to guess whether `cctx` is being used or how much it saves. The extension provides two built-in usage views.

### 1. VS Code Status Bar badge & hover popover (instant, in-editor)

Look at the bottom status bar in VS Code. `cctx` displays live native call counts, active model, estimated tokens saved, and USD/AIC savings:

```text
$(database) CCTX N S:382 C:4 P:0 E:0 · Claude Sonnet 4.6 · ~142.8K tok (+$0.48)
```

Hovering over the status bar item opens a rich Markdown popover showing per-model spend, cache hit rate, and native tool breakdown (synthetic example):

| 💰 Cost Saved (@$0.001) | 💎 API Value (@$0.01) | 🔥 Tokens Saved | 🎯 Warm Hit Rate | ⚡ Avg Latency |
| :---: | :---: | :---: | :---: | :---: |
| **+$0.48** (`~480.0 AIC`) | **+$4.80** | **~142.8K tok** | **99.0%** (`382/386`) | **0.31 ms** |

| Operation | Calls (OK/Miss) | Delivered | Saved Tok | Avg ms |
| :--- | ---: | ---: | ---: | ---: |
| `cctxNativeContext` | 142 (140/2) | ~68.4K | ~94.2K | 0.32 ms |
| `cctxNativeRead` | 128 (127/1) | ~52.1K | ~31.5K | 0.26 ms |
| `cctxNativeGrep` | 74 (73/1) | ~16.8K | ~12.4K | 0.29 ms |
| `cctxNativeFiles` | 42 (42/0) | ~5.5K | ~4.7K | 0.30 ms |

### 2. Multi-session terminal dashboard (`cctx usage --hours 24`)

To inspect aggregate LLM spend, prompt-cache hit rate, eligible source adoption, and per-model/per-session savings across all your recent VS Code Copilot sessions, run `cctx usage --hours 24` (or `--hours 168` for 7 days) in the VS Code terminal (synthetic example):

```text
$ cctx usage --hours 24
╔═══════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════╗
║ ⚡ CODE CONTEXT (cctx) — MODEL, TOKEN & COST SAVINGS DASHBOARD (Window: 24.0h)                                            ║
╠═══════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════╣
║ 💰 COST SAVED (@$0.001/AIC) :  +$24.80    USD   (-23.5% of baseline bill · ~24800.0 AIC saved)                            ║
║ 💎 API LIST VALUE (@$0.01)  :  +$248.00   USD   (Frontier API list price: $2/$0.10/$10 per 1M)                            ║
║ 🔥 TOKENS SAVED             :  ~740.20M   tok   (Payload ~7.42M + 35x Cache ~259.7M + ~2040 Turns ~450.1M + BPE ~22.98M)  ║
║ 🧬 BPE FORMULA COMPRESSION  :  ██████████ 100.0%  (4/4 surfaces: ext·mcp·subagent·global · ~2100 tok/turn · +$12.30)      ║
║ 🎯 WARM CACHE HIT RATE      :  ██████████  97.2%  (4680 warm hits / 4815 CCTX lookups)                                    ║
║ 🚀 ELIGIBLE SOURCE ADOPTION :  ███████░░░  74.8%  (4815/6437 · excl. 920 /tmp & 680 pre-edit)                             ║
╚═══════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════╝

  1. LLM MODEL USAGE, CACHE HIT & COST SAVINGS
┌──────────────────────────┬────────────┬─────────────────┬───────────────┬───────────────┬───────────┬────────────────────────┬──────────────────────────────────────────┐
│ Model (Effort)           │ Sess/Turns │  Input (Cache%) │   Out+Rsn Tok │  CCTX (Warm%) │ Src Adopt │    LLM Spend (@$0.001) │        COST SAVED (@$0.001 / API @$0.01) │
├──────────────────────────┼────────────┼─────────────────┼───────────────┼───────────────┼───────────┼────────────────────────┼──────────────────────────────────────────┤
│ Claude Sonnet 4.6 (high) │    18/4120 │   1.84B (96.2%) │   4.12M+1.90M │  3410 (97.5%) │     76.4% │   $58.40 (58400.0 AIC) │   +$18.20 / API +$182.00 (~18200.0 AIC)  │
│ GPT-5.4 (medium)         │    24/1980 │ 640.00M (97.4%) │   2.10M+1.15M │  1405 (96.5%) │     71.2% │   $22.10 (22100.0 AIC) │     +$6.60 / API +$66.00 (~6600.0 AIC)   │
│ TOTAL (All Models)       │    42/6100 │   2.48B (96.5%) │   6.22M+3.05M │  4815 (97.2%) │     74.8% │   $80.50 (80500.0 AIC) │   +$24.80 / API +$248.00 (~24800.0 AIC)  │
└──────────────────────────┴────────────┴─────────────────┴───────────────┴───────────────┴───────────┴────────────────────────┴──────────────────────────────────────────┘
```

Combine this with VS Code's built-in [Agent Debug Logs](../02-measure-usage/) to compare a representative task before and after enabling `cctx`.

---

## Trade-offs and cautions

- **Tiny known-range file reads:** Because `cctx` communicates with a local daemon over IPC, reading a tiny known slice of a single file can be slightly slower in raw sub-millisecond latency (`0.251 ms` vs `0.083 ms` direct host read), though `#cctxNativeRead` adds SHA-256 verification and `<8 KiB` inline budget clamping.
- **Cold-start fallback:** A `CACHE_MISS` on the first lookup in a new workspace is normal — it is **not** an empty search result. Copilot falls back to standard host tools for that turn while the daemon finishes indexing in the background (`~29 ms` median on medium repos).
- **Security and exclusion boundaries:** Hidden configuration directories (`.github`, `.vscode`), `.gitignore` paths, `/tmp`, and secret files are intentionally excluded from the trigram cache and handled by VS Code's policy-aware host tools.

## Sources

- [Code Context by dotnetpower — VS Code Marketplace (`dotnetpower.code-context-native`)](https://marketplace.visualstudio.com/items?itemName=dotnetpower.code-context-native)
- [dotnetpower/code-context GitHub repository](https://github.com/dotnetpower/code-context)
- [Code Context Benchmarks & Token Efficiency Report](https://github.com/dotnetpower/code-context/blob/main/docs/BENCHMARKS.md)
- [VS Code Setup & Troubleshooting Guide](https://github.com/dotnetpower/code-context/blob/main/vscode-extension/docs/SETUP.md)
