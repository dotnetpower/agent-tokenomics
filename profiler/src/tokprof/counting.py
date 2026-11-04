"""Token counting. Always local; never sends content anywhere."""

from __future__ import annotations

import os
from functools import lru_cache
from typing import Optional

_MAX_CACHE_TEXT_LEN = 65_536


class HeuristicCounter:
    """Dependency-free estimate: about 4 ASCII characters per token, one token per non-ASCII character."""

    method = "heuristic"

    def count(self, text: str) -> int:
        if not text:
            return 0
        if not isinstance(text, str):
            text = str(text)
        if text.isascii():
            return (len(text) + 3) >> 2
        ascii_chars = len(text.encode("ascii", "ignore"))
        other = len(text) - ascii_chars
        return ((ascii_chars + 3) >> 2) + other


_HEURISTIC_SINGLETON = HeuristicCounter()


class TiktokenCounter:
    def __init__(self, encoding_name: str, approx: bool):
        import tiktoken

        self._enc = tiktoken.get_encoding(encoding_name)
        self.method = f"tiktoken:{encoding_name}" + ("~approx" if approx else "")
        self._cached_len = lru_cache(maxsize=2048)(self._encode_len)

    def _encode_len(self, text: str) -> int:
        try:
            return len(self._enc.encode(text, disallowed_special=()))
        except UnicodeEncodeError:
            clean = text.encode("utf-8", "replace").decode("utf-8")
            return len(self._enc.encode(clean, disallowed_special=()))

    def count(self, text: str) -> int:
        if not text:
            return 0
        if not isinstance(text, str):
            text = str(text)
        if len(text) <= _MAX_CACHE_TEXT_LEN:
            return self._cached_len(text)
        return self._encode_len(text)


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
        self._cached_len = lru_cache(maxsize=2048)(self._encode_len)

    def _encode_len(self, text: str) -> int:
        try:
            return len(self._tok.encode(text, add_special_tokens=False).ids)
        except Exception:
            clean = text.encode("utf-8", "replace").decode("utf-8")
            return len(self._tok.encode(clean, add_special_tokens=False).ids)

    def count(self, text: str) -> int:
        if not text:
            return 0
        if not isinstance(text, str):
            text = str(text)
        if len(text) <= _MAX_CACHE_TEXT_LEN:
            return self._cached_len(text)
        return self._encode_len(text)


_OPENAI_PREFIXES = ("gpt-", "o1", "o3", "o4", "chatgpt", "text-embedding", "ft:gpt", "computer-use", "codex-")
_TIKTOKEN_ENCODINGS = ("o200k_base", "cl100k_base", "p50k_base", "r50k_base")


@lru_cache(maxsize=256)
def _encoding_for(model: Optional[str]) -> tuple[str, bool]:
    """Return (encoding name, approx). approx is True when the model is not an OpenAI family."""
    m = (model or "").lower().strip()
    if "/" in m:
        m = m.rsplit("/", 1)[-1]
    if m.startswith(("gpt-3.5", "gpt-4-", "gpt-4")) and not m.startswith(("gpt-4o", "gpt-4.1", "gpt-4.5", "gpt-5")):
        return "cl100k_base", False
    if m.startswith(_OPENAI_PREFIXES) or "gpt-4o" in m or "gpt-4.1" in m or "gpt-5" in m:
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
    custom = os.environ.get("TOKPROF_TOKENIZER")
    if custom:
        custom = custom.strip()
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
    return _HEURISTIC_SINGLETON
