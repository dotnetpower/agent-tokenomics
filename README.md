# agent-tokenomics

Practical ways to save LLM tokens, in two tracks. Inspired by
[microsoft/TokenEconomizer](https://github.com/microsoft/TokenEconomizer).

- **Track A – Copilot Tokenomics:** reduce token / AI-credit use of GitHub Copilot in VS Code.
- **Track B – Agent Tokenomics:** a troubleshooting procedure for LLM agents:
  Diagnose → Isolate → Prove → Improve → Result.

Docs site (Astro Starlight, GitHub Pages): <https://dotnetpower.github.io/agent-tokenomics/>

## Repository layout

| Path | Content |
|------|---------|
| `site/` | Astro Starlight docs site. English pages in `site/src/content/docs/`; Korean under `ko/`. Korean README: [README.ko.md](README.ko.md). |
| `examples/` | `token_report.py` — summarize a JSONL request log (overhead, cache hit, unused tools, failure loops); `sample-requests.jsonl`; `count_tokens.py` — token counts of one sentence across languages (needs `tiktoken`); `formula_prompt_en.py`, `formula_prompt_ko.py`, `formula_prompt.py` + `test_formula_prompt.py` — separated English and Korean BPE-aligned formula prompts and Hugging Face customer-support/counseling dialogue compaction (needs `tiktoken`); `prompt_budget.py` + `sample-pipeline.json` — static per-step token map for multi-call pipelines (synthetic example); `papers.json` + `papers-ko.json` + `fetch_paper_meta.py` + `build_papers.py` — generate the ranked Papers page (English and Korean). |
| `profiler/` | [`tokprof`](https://pypi.org/project/tokprof/): a zero-code Rust token profiler (`pip install tokprof` / `uvx tokprof`) that breaks input tokens down by system prompt, history, user prompt, and tool-related content per session, turn, and call. |
| `.github/copilot-instructions.md` | Authoring rules for this repository. |

## Develop

```bash
npm --prefix site ci
npm --prefix site run dev      # http://localhost:4321
npm --prefix site run build
python3 examples/token_report.py examples/sample-requests.jsonl
```

The site logo is a currency-style **T** with two diagonal bars, connecting tokens and cost. The header logo and favicon share [`site/public/favicon.svg`](site/public/favicon.svg); edit this SVG to keep them in sync.

## License

See [LICENSE](LICENSE).