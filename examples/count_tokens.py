#!/usr/bin/env python3
"""Count tokens of the same sentence in several languages.

Reproduces the "language and tokenization" table in the docs. Token counts depend on the
tokenizer, so run it with the encoding that matches your model.

Setup:
    python3 -m venv .venv && .venv/bin/pip install tiktoken
Usage:
    .venv/bin/python count_tokens.py [--encoding o200k_base] [--text "your text" ...]
"""

from __future__ import annotations

import argparse

import tiktoken

SENTENCES = {
    "English": "I met a huge dog",
    "Spanish": "Conocí a un perro enorme",
    "Korean": "나는 커다란 개를 만났다",
    "Chinese": "我遇見了一隻大狗",
    "Japanese": "大きな犬に出会った",
    "Russian": "Я встретил огромную собаку",
    "Hebrew": "פגשתי כלב ענק",
}


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--encoding", default="o200k_base")
    ap.add_argument("--text", action="append", help="extra text to count (repeatable)")
    args = ap.parse_args()

    enc = tiktoken.get_encoding(args.encoding)
    rows = dict(SENTENCES)
    for i, t in enumerate(args.text or [], 1):
        rows[f"custom {i}"] = t

    base = len(enc.encode(SENTENCES["English"]))
    print(f"encoding={args.encoding} (tiktoken {tiktoken.__version__})")
    print(f"{'language':<10} {'chars':>5} {'bytes':>5} {'tokens':>6} {'x English':>9}")
    for name, text in rows.items():
        n = len(enc.encode(text))
        print(f"{name:<10} {len(text):>5} {len(text.encode()):>5} {n:>6} {n / base:>8.1f}x")


if __name__ == "__main__":
    main()
