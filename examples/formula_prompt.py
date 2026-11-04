#!/usr/bin/env python3
"""Compare prose, Unicode math, and BPE-aligned formula prompts (English & Korean separated).

English examples and compressor live in `examples/formula_prompt_en.py`.
Korean examples and compressor live in `examples/formula_prompt_ko.py`.
This script provides a unified CLI (`--lang {all,en,ko}`) and dispatches by language.

Setup:
    python3 -m venv .venv && .venv/bin/pip install tiktoken
Usage:
    .venv/bin/python examples/formula_prompt.py
    .venv/bin/python examples/formula_prompt.py --lang en
    .venv/bin/python examples/formula_prompt.py --lang ko
    .venv/bin/python examples/formula_prompt_en.py
    .venv/bin/python examples/formula_prompt_ko.py
    .venv/bin/python examples/formula_prompt.py --compress "이슈 1945을 완료할수 있도록 남은 작업에 대한 계획을 세워서 모두 조치하고 완료되면 푸시 머지까지 해서 이슈를 닫을수 있도록 하자"
    .venv/bin/python examples/formula_prompt.py --hf-live --hf-rows 50
"""

from __future__ import annotations

import argparse
from pathlib import Path
import re
import sys
from typing import Any, Sequence

import tiktoken

sys.path.insert(0, str(Path(__file__).resolve().parent))
from formula_prompt_en import (  # noqa: E402
    ENGLISH_WORKFLOW_CASES,
    HF_ENGLISH_DIALOGUE_CASES,
    HF_ENGLISH_LIVE_SPECS,
    HF_ENGLISH_TURN_CASES,
    STYLE_COMPARISON,
    _format_tool_call,
    check_slot_retention,
    compress_dialogue_en,
    compress_prompt_en,
    ensure_slot_preservation,
    extract_numeric_and_id_tokens,
    fetch_hf_rows,
    print_tables_en,
    run_hf_live_benchmark_en,
)
from formula_prompt_ko import (  # noqa: E402
    HF_KOREAN_DIALOGUE_CASES,
    HF_KOREAN_LIVE_SPECS,
    HF_KOREAN_TURN_CASES,
    KOREAN_WORKFLOW_CASES,
    compress_dialogue_ko,
    compress_prompt_ko,
    print_tables_ko,
    run_hf_live_benchmark_ko,
)

# Combined lists for callers inspecting all cases across both languages
HF_COUNSELING_TURN_CASES: list[dict[str, Any]] = HF_KOREAN_TURN_CASES + HF_ENGLISH_TURN_CASES
HF_COUNSELING_DIALOGUE_CASES: list[dict[str, Any]] = HF_KOREAN_DIALOGUE_CASES + HF_ENGLISH_DIALOGUE_CASES


def compress_prompt(text: str, *, include_lang_tag: bool = True) -> str:
    """Dispatch prompt/turn compression to the Korean or English BPE compressor."""
    s = text.strip()
    if not s:
        return ""
    if re.search(r"[가-힣]", s):
        return compress_prompt_ko(s, include_lang_tag=include_lang_tag)
    return compress_prompt_en(s)


def compress_dialogue(
    dialogue: str | Sequence[dict[str, Any]],
    *,
    include_lang_tag: bool = True,
) -> str:
    """Dispatch multi-turn dialogue compression to the Korean or English BPE compressor."""
    if isinstance(dialogue, str):
        has_ko = bool(re.search(r"[가-힣]", dialogue))
    else:
        raw_concat = " ".join(str(m.get("content") or "") for m in dialogue)
        has_ko = bool(re.search(r"[가-힣]", raw_concat))
    if has_ko:
        return compress_dialogue_ko(dialogue, include_lang_tag=include_lang_tag)
    return compress_dialogue_en(dialogue)


def run_hf_live_benchmark(rows_per_dataset: int = 50, *, lang: str = "all") -> list[dict[str, Any]]:
    """Run live Hugging Face benchmarks separated into English and Korean sections."""
    results: list[dict[str, Any]] = []
    if lang in ("all", "en"):
        results.extend(run_hf_live_benchmark_en(rows_per_dataset))
    if lang in ("all", "ko"):
        results.extend(run_hf_live_benchmark_ko(rows_per_dataset))

    if lang == "all" and results:
        tot_rows = sum(r["rows"] for r in results)
        tot_p200 = sum(r["o200k_prose"] for r in results)
        tot_f200 = sum(r["o200k_formula"] for r in results)
        tot_pcl = sum(r["cl100k_prose"] for r in results)
        tot_fcl = sum(r["cl100k_formula"] for r in results)
        tot_kept = sum(r["slots_kept"] for r in results)
        tot_slots = sum(r["slots_total"] for r in results)
        d200 = 100.0 * (tot_f200 - tot_p200) / max(1, tot_p200)
        dcl = 100.0 * (tot_fcl - tot_pcl) / max(1, tot_pcl)
        print(f"\n=== [ALL] Combined English + Korean Live HF Summary ({tot_rows} rows) ===")
        print(
            f"{f'Combined ({len(HF_ENGLISH_LIVE_SPECS)} EN + {len(HF_KOREAN_LIVE_SPECS)} KO datasets)':<54} {tot_rows:>4} "
            f"{tot_p200:>5} -> {tot_f200:<5} ({d200:>+5.1f}%)   "
            f"{tot_pcl:>5} -> {tot_fcl:<5} ({dcl:>+5.1f}%)   "
            f"{f'{tot_kept}/{tot_slots}':>12}"
        )
    return results


def print_tables(*, lang: str = "all") -> None:
    """Print benchmark tables separated into English and Korean sections."""
    print(f"tiktoken {tiktoken.__version__}\n")
    if lang in ("all", "en"):
        print_tables_en()
    if lang == "all":
        print()
    if lang in ("all", "ko"):
        print_tables_ko()


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument(
        "--lang",
        choices=("all", "en", "ko"),
        default="all",
        help="run English examples only (en), Korean examples only (ko), or both separated (all)",
    )
    ap.add_argument("--compress", help="convert a prose prompt/turn into a BPE-aligned formula and count tokens")
    ap.add_argument("--compress-dialogue", help="compact a multi-turn counselor dialogue into a BPE formula")
    ap.add_argument("--hf-live", action="store_true", help="fetch live rows from Hugging Face Datasets Server and benchmark")
    ap.add_argument("--hf-rows", type=int, default=50, help="number of rows per Hugging Face dataset for --hf-live")
    args = ap.parse_args()

    if args.compress or args.compress_dialogue:
        enc_o200k = tiktoken.get_encoding("o200k_base")
        enc_cl100k = tiktoken.get_encoding("cl100k_base")
        raw = args.compress if args.compress else args.compress_dialogue
        if args.lang == "en":
            formula = compress_prompt_en(raw) if args.compress else compress_dialogue_en(raw)
        elif args.lang == "ko":
            formula = compress_prompt_ko(raw) if args.compress else compress_dialogue_ko(raw)
        else:
            formula = compress_prompt(raw) if args.compress else compress_dialogue(raw)
        in_200, out_200 = len(enc_o200k.encode(raw)), len(enc_o200k.encode(formula))
        in_cl, out_cl = len(enc_cl100k.encode(raw)), len(enc_cl100k.encode(formula))
        print(f"Input  (o200k={in_200}, cl100k={in_cl}): {raw}")
        print(
            f"Output (o200k={out_200} [{100*(out_200-in_200)/max(1,in_200):+.1f}%], "
            f"cl100k={out_cl} [{100*(out_cl-in_cl)/max(1,in_cl):+.1f}%]): {formula}"
        )
        return

    print_tables(lang=args.lang)
    if args.hf_live:
        run_hf_live_benchmark(args.hf_rows, lang=args.lang)


if __name__ == "__main__":
    main()
