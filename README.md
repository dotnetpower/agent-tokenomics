# agent-tokenomics — Tokenomics & Token Economics for LLMs & AI Agents

[![Deploy docs site to GitHub Pages](https://github.com/dotnetpower/agent-tokenomics/actions/workflows/pages.yml/badge.svg)](https://github.com/dotnetpower/agent-tokenomics/actions/workflows/pages.yml)
[![VS Code Marketplace - cctx](https://img.shields.io/badge/VS%20Code%20Marketplace-Code%20Context%20(cctx)-3b5bdb)](https://marketplace.visualstudio.com/items?itemName=dotnetpower.code-context-native)
[![PyPI - tokprof](https://img.shields.io/pypi/v/tokprof?label=PyPI%20tokprof&color=245e63)](https://pypi.org/project/tokprof/)
[![License: MIT](https://img.shields.io/github/license/dotnetpower/agent-tokenomics)](./LICENSE)
[![Docs (EN)](https://img.shields.io/badge/docs-English-3b5bdb)](https://dotnetpower.github.io/agent-tokenomics/)
[![Docs (KO)](https://img.shields.io/badge/docs-%ED%95%9C%EA%B5%AD%EC%96%B4-3b5bdb)](https://dotnetpower.github.io/agent-tokenomics/ko/)
[![llms.txt](https://img.shields.io/badge/GEO-llms.txt-20c997)](https://dotnetpower.github.io/agent-tokenomics/llms.txt)

**Spend fewer LLM tokens and accelerate development without making your AI tools worse.**

`agent-tokenomics` is a bilingual (English / [한국어](README.ko.md)) **tokenomics (token economics)** engineering guide and toolkit for measuring, diagnosing, and reducing LLM token consumption across two tracks, inspired by [microsoft/TokenEconomizer](https://github.com/microsoft/TokenEconomizer):

- **Track A – [Copilot Tokenomics](https://dotnetpower.github.io/agent-tokenomics/copilot-tokenomics/):** Practical levers to reduce token and AI Credit usage in everyday VS Code GitHub Copilot workflows, plus **[`Code Context (cctx)`](https://dotnetpower.github.io/agent-tokenomics/copilot-tokenomics/cctx/)** ([VS Code Marketplace](https://marketplace.visualstudio.com/items?itemName=dotnetpower.code-context-native)) to cut lookup tokens, accelerate coding tasks (`15.20×` faster retrieval, `-24.7%` task wall time, as reported), and view live AIC/USD savings in the status bar and `cctx usage`.
- **Track B – [Agent Tokenomics](https://dotnetpower.github.io/agent-tokenomics/agent-tokenomics/):** A 6-stage troubleshooting procedure for engineers who build or operate LLM agents (`Concepts → Diagnose → Isolate → Prove → Improve → Result → Golden Dataset Rubric`).
- **[`tokprof`](https://pypi.org/project/tokprof/) (`pip install tokprof`):** A standalone, zero-code native Rust token profiler that attributes every LLM call's input tokens to system prompt, conversation history, user prompt, and tool definitions/results, and renders an interactive HTML Edition Dossier.

> **Live documentation:** <https://dotnetpower.github.io/agent-tokenomics/> · **한국어 문서:** <https://dotnetpower.github.io/agent-tokenomics/ko/> · **LLM index:** [`llms.txt`](https://dotnetpower.github.io/agent-tokenomics/llms.txt) / [`llms-full.txt`](https://dotnetpower.github.io/agent-tokenomics/llms-full.txt)

---

## Pick your path

| Who you are | Start here |
|---|---|
| **Developer using GitHub Copilot in VS Code** | [Track A: Copilot Tokenomics](https://dotnetpower.github.io/agent-tokenomics/copilot-tokenomics/) → [10-minute Checklist](https://dotnetpower.github.io/agent-tokenomics/copilot-tokenomics/10-checklist/) → [`cctx` VS Code Extension](https://dotnetpower.github.io/agent-tokenomics/copilot-tokenomics/cctx/) |
| **Engineer building or operating an LLM agent** | [Track B: Agent Tokenomics](https://dotnetpower.github.io/agent-tokenomics/agent-tokenomics/) → [0. Concepts](https://dotnetpower.github.io/agent-tokenomics/agent-tokenomics/00-concepts/) → [1. Diagnose](https://dotnetpower.github.io/agent-tokenomics/agent-tokenomics/01-diagnose/) |
| **Profiling an existing agent with zero code changes** | [`tokprof` Install & Usage](https://dotnetpower.github.io/agent-tokenomics/agent-tokenomics/tokprof/) · [Interactive HTML Report Sample](https://dotnetpower.github.io/agent-tokenomics/demo/tokprof-sample.html) |
| **Evaluating answer quality after token optimization** | [6. Golden Dataset & Rubric Evaluation](https://dotnetpower.github.io/agent-tokenomics/agent-tokenomics/06-golden-dataset-and-rubric/) |
| **AI agent / LLM crawler** | [`llms.txt`](llms.txt) (structured index) · [`llms-full.txt`](https://dotnetpower.github.io/agent-tokenomics/llms-full.txt) (single-fetch full corpus) |

---

## Quickstart: `tokprof` (Zero-Code LLM Token Profiler)

[`tokprof`](https://pypi.org/project/tokprof/) ships prebuilt native wheels on PyPI for Linux (`x86_64`, `aarch64`), macOS (`arm64`, `x86_64`), and Windows (`x64`). No Rust or Go toolchain is required.

```bash
# 1. Install from PyPI (or run directly with uvx tokprof)
pip install tokprof
# or: uv tool install tokprof

# 2. Start background profiling (auto-attaches Linux eBPF to running processes & Docker)
tokprof start

# 3. Run your agent workload, check live call counts, then stop & generate the report
tokprof status
tokprof stop                           # prints terminal breakdown + writes .tokprof/latest.html

# Or profile a single command in one step:
tokprof -- python my_agent.py
```

See [profiler/README.md](profiler/README.md) and the [full `tokprof` documentation](https://dotnetpower.github.io/agent-tokenomics/agent-tokenomics/tokprof/).

---

## Documentation map

### Track A — VS Code GitHub Copilot (`copilot-tokenomics/`)

Every guide follows *Problem → Action → How to verify → Trade-offs*:

| # | Guide | Question it answers |
|---|---|---|
| 01 | [How Copilot usage is charged](https://dotnetpower.github.io/agent-tokenomics/copilot-tokenomics/01-how-usage-is-charged/) | How do AI Credits (token billing) and Premium Request multipliers work? |
| 02 | [Measure your usage](https://dotnetpower.github.io/agent-tokenomics/copilot-tokenomics/02-measure-usage/) | Which chats, turns, and requests cost the most in VS Code? |
| 03 | [Keep context lean](https://dotnetpower.github.io/agent-tokenomics/copilot-tokenomics/03-context-hygiene/) | How do new chats, `/fork`, `/compact`, and `#file` prevent history bloat? |
| 04 | [Instructions and tools](https://dotnetpower.github.io/agent-tokenomics/copilot-tokenomics/04-instructions-and-tools/) | How do I shrink always-on `copilot-instructions.md` and tool definitions? |
| 05 | [Models, planning, delegation](https://dotnetpower.github.io/agent-tokenomics/copilot-tokenomics/05-models-and-planning/) | When does a cheaper model, Plan mode, or subagent delegation lower total cost? |
| 06 | [Prompt and output style](https://dotnetpower.github.io/agent-tokenomics/copilot-tokenomics/06-prompt-and-output-style/) | How do structured / BPE-aligned formula prompts and output caps cut tokens? |
| 07 | [Language and tokenization](https://dotnetpower.github.io/agent-tokenomics/copilot-tokenomics/07-language-and-tokenization/) | Why do non-English prompts use more BPE tokens, and how do I measure it? |
| 08 | [Agent modes and budget guards](https://dotnetpower.github.io/agent-tokenomics/copilot-tokenomics/08-agent-modes-and-budget/) | How do Ask, Plan, and Agent modes differ, and how do I bound the agent loop? |
| 09 | [MCP audit](https://dotnetpower.github.io/agent-tokenomics/copilot-tokenomics/09-mcp-audit/) | Which MCP servers should stay enabled per workspace? |
| 10 | [Checklist](https://dotnetpower.github.io/agent-tokenomics/copilot-tokenomics/10-checklist/) | What is the one-page operational routine prioritized by impact? |
| VSC | [`cctx`: VS Code Token Saver & Accelerator](https://dotnetpower.github.io/agent-tokenomics/copilot-tokenomics/cctx/) | How do I install `cctx` from the VS Code Marketplace to cut lookup tokens, speed up coding (`15.20×` retrieval, `-24.7%` task time), and see live AIC/USD savings? |

### Track B — LLM Agent Token Troubleshooting (`agent-tokenomics/`)

| Stage | Guide | Output |
|---|---|---|
| 00 | [Concepts: how an agent spends tokens](https://dotnetpower.github.io/agent-tokenomics/agent-tokenomics/00-concepts/) | Mental model: stateless re-sending, agent loop, prompt caching, context rot, tool taxes |
| 01 | [Diagnose the current state](https://dotnetpower.github.io/agent-tokenomics/agent-tokenomics/01-diagnose/) | Measured baseline of input/output/cached tokens, fixed overhead, and turn growth |
| 02 | [Isolate the problem](https://dotnetpower.github.io/agent-tokenomics/agent-tokenomics/02-isolate/) | Single-variable attribution (unused tools, failure loops, bloated tool outputs, history) |
| 03 | [Prove the problem](https://dotnetpower.github.io/agent-tokenomics/agent-tokenomics/03-prove/) | Quantified waste reproduced on a frozen task set with repeat runs |
| 04 | [Improve](https://dotnetpower.github.io/agent-tokenomics/agent-tokenomics/04-improve/) | One targeted fix per experiment (micro-skills, tool pruning, loop guards, caching) |
| 05 | [Result](https://dotnetpower.github.io/agent-tokenomics/agent-tokenomics/05-result/) | Before/after cost, quality guardrail, and latency comparison (keep or revert) |
| 06 | [Golden dataset and rubric evaluation](https://dotnetpower.github.io/agent-tokenomics/agent-tokenomics/06-golden-dataset-and-rubric/) | Excel/CSV golden dataset and hybrid keyword + intent rubric for single/multi-turn evals |
| CLI | [`tokprof`: Install & Usage](https://dotnetpower.github.io/agent-tokenomics/agent-tokenomics/tokprof/) | Zero-code per-call token profiling via eBPF, local gateway, and in-process hooks |
| Case | [Case study: TokenEconomizer](https://dotnetpower.github.io/agent-tokenomics/agent-tokenomics/case-tokeneconomizer/) | How `microsoft/TokenEconomizer` (18.4–30.9% lower mean SWE-Bench cost) maps to Stages 1–5 |

### Reference & Research

- **[Resources](https://dotnetpower.github.io/agent-tokenomics/resources/):** Curated open-source tools, benchmarks, and official documentation.
- **[Papers](https://dotnetpower.github.io/agent-tokenomics/papers/):** 75 papers on prompt compression, caching, routing, and agent token efficiency ranked by venue tier and citations.
- **[Glossary](https://dotnetpower.github.io/agent-tokenomics/glossary/):** Bilingual (English ↔ Korean) terminology reference.

---

## Repository layout

| Path | Content |
|------|---------|
| `site/` | Astro Starlight documentation site (`https://dotnetpower.github.io/agent-tokenomics/`). English pages in `site/src/content/docs/`; Korean twins under `site/src/content/docs/ko/`. Korean README: [README.ko.md](README.ko.md). |
| `profiler/` | [`tokprof`](https://pypi.org/project/tokprof/): zero-code native Rust token profiler (`pip install tokprof` / `uvx tokprof`) with Linux eBPF TLS tap (`profiler/ebpf/`), local gateway, Python/Node hooks, and interactive HTML Edition Dossier. |
| `examples/` | Runnable measurement scripts and datasets: `token_report.py` + `sample-requests.jsonl`; `count_tokens.py`; `formula_prompt_en.py`, `formula_prompt_ko.py`, `formula_prompt.py` + `test_formula_prompt.py`; `prompt_budget.py` + `sample-pipeline.json`; `golden_dataset_tool.py` + `sample-golden-dataset.csv`/`.xlsx`; `papers.json` + `papers-ko.json` + `fetch_paper_meta.py` + `build_papers.py`. |
| `llms.txt` | Curated [llmstxt.org](https://llmstxt.org/) documentation index for generative engines and coding agents (paired with `site/public/llms-full.txt`). |
| `.github/copilot-instructions.md` | Authoring and bilingual parity rules for this repository. |

---

## Frequently Asked Questions

### What is tokenomics (token economics) in LLMs and AI agents?
In AI and large language models, **tokenomics (token economics)** is the engineering discipline of measuring, pricing, budgeting, and optimizing input, cached, output, and reasoning tokens across model calls and multi-turn agent loops without degrading answer quality.

### What is Rule Zero of `agent-tokenomics`?
Fewer tokens alone is not a valid win. Every optimization must be paired with a **quality guardrail** (pass rate on a fixed task set or a [Golden Dataset rubric](https://dotnetpower.github.io/agent-tokenomics/agent-tokenomics/06-golden-dataset-and-rubric/)) to verify that answer accuracy and multi-turn context retention do not degrade.

### How does `tokprof` capture LLM requests without changing agent code?
On Linux, `tokprof start` can attach an eBPF TLS uprobe tap to capture HTTPS traffic from already-running processes and Docker containers. Across Linux, macOS, and Windows, `tokprof` also provides a local reverse-proxy gateway (`OPENAI_BASE_URL`, `ANTHROPIC_BASE_URL`, `AZURE_OPENAI_ENDPOINT`) and automatic in-process hooks for Python (`httpx`, `requests`, `urllib3`, `aiohttp`) and Node.js (`fetch`, `http`/`https`).

---

## Develop

```bash
npm --prefix site ci
npm --prefix site run dev          # http://localhost:4321
npm --prefix site run check:pairs -- --strict
npm --prefix site run build        # builds dist/ + syncs llms.txt & llms-full.txt
npm --prefix site run check:seo    # verifies SEO meta, JSON-LD, robots.txt, sitemap, and llms.txt
python3 examples/token_report.py examples/sample-requests.jsonl
```

The site logo is a currency-style **T** with two diagonal bars, connecting tokens and cost. The header logo and favicon share [`site/public/favicon.svg`](site/public/favicon.svg); edit this SVG to keep them in sync.

## License

See [LICENSE](LICENSE).