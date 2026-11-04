"""Token counting. Always local; never sends content anywhere."""

from __future__ import annotations

import math
import os
from functools import lru_cache
from typing import Optional


class HeuristicCounter:
    """Dependency-free estimate: about 4 ASCII characters per token, one token per non-ASCII character."""

    method = "heuristic"

    def count(self, text: str) -> int:
        if not text:
            return 0
        ascii_chars = sum(1 for ch in text if ord(ch) < 128)
        other = len(text) - ascii_chars
        return int(math.ceil(ascii_chars / 4.0)) + other


class TiktokenCounter:
    def __init__(self, encoding_name: str, approx: bool):
        import tiktoken

        self._enc = tiktoken.get_encoding(encoding_name)
        self.method = f"tiktoken:{encoding_name}" + ("~approx" if approx else "")

    def count(self, text: str) -> int:
        if not text:
            return 0
        return len(self._enc.encode(text, disallowed_special=()))


class HFTokenizerCounter:
    """Exact local counting with a HuggingFace `tokenizer.json` file or pretrained tokenizer."""

    def __init__(self, spec: str):
        from tokenizers import Tokenizer

        if os.path.isfile(spec):
            self._tok = Tokenizer.from_file(spec)
            label = os.path.basename(spec)
        else:
            self._tok = Tokenizer.from_pretrained(spec)
            label = spec
        self.method = f"hf:{label}"

    def count(self, text: str) -> int:
        if not text:
            return 0
        return len(self._tok.encode(text, add_special_tokens=False).ids)


_OPENAI_PREFIXES = ("gpt-", "o1", "o3", "o4", "chatgpt", "text-embedding", "ft:gpt")
_TIKTOKEN_ENCODINGS = ("o200k_base", "cl100k_base", "p50k_base", "r50k_base")


def _encoding_for(model: Optional[str]) -> tuple[str, bool]:
    """Return (encoding name, approx). approx is True when the model is not an OpenAI family."""
    m = (model or "").lower()
    if m.startswith(("gpt-3.5", "gpt-4-", "gpt-4")) and not m.startswith(("gpt-4o", "gpt-4.1", "gpt-4.5")):
        return "cl100k_base", False
    if m.startswith(_OPENAI_PREFIXES):
        return "o200k_base", False
    return "o200k_base", True


@lru_cache(maxsize=32)
def _tiktoken_counter(encoding_name: str, approx: bool):
    try:
        return TiktokenCounter(encoding_name, approx)
    except Exception:
        return None


@lru_cache(maxsize=16)
def _hf_counter(spec: str):
    try:
        return HFTokenizerCounter(spec)
    except Exception:
        return None


def get_counter(model: Optional[str] = None, prefer_tiktoken: bool = True):
    """Best available counter for a model. Falls back to the heuristic when tiktoken is not usable."""
    custom = (os.environ.get("TOKPROF_TOKENIZER") or "").strip()
    if custom:
        if custom in _TIKTOKEN_ENCODINGS:
            c = _tiktoken_counter(custom, False)
            if c is not None:
                return c
        hc = _hf_counter(custom)
        if hc is not None:
            return hc
    if prefer_tiktoken:
        name, approx = _encoding_for(model)
        counter = _tiktoken_counter(name, approx)
        if counter is not None:
            return counter
    return HeuristicCounter()
