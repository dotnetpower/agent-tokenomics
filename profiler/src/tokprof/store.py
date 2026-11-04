"""JSONL storage. Counts, hashes, and names only: no prompt text, headers, or credentials are written."""

from __future__ import annotations

import json
import os
import threading
from typing import Iterator, Optional


class JsonlWriter:
    def __init__(self, path: str):
        self.path = path
        self._lock = threading.Lock()
        self._closed = False
        parent = os.path.dirname(os.path.abspath(path))
        os.makedirs(parent, exist_ok=True)
        flags = os.O_WRONLY | os.O_CREAT | os.O_APPEND
        fd = os.open(path, flags, 0o600)
        self._fh = os.fdopen(fd, "a", encoding="utf-8", buffering=65536)

    def __enter__(self) -> "JsonlWriter":
        return self

    def __exit__(self, exc_type, exc, tb) -> None:
        self.close()

    def write(self, record: dict) -> None:
        line = json.dumps(record, ensure_ascii=False, separators=(",", ":"))
        with self._lock:
            if self._closed:
                return
            self._fh.write(f"{line}\n")
            self._fh.flush()

    def close(self) -> None:
        with self._lock:
            if self._closed:
                return
            self._closed = True
            try:
                self._fh.flush()
                self._fh.close()
            except Exception:
                pass


class ReadStats:
    def __init__(self) -> None:
        self.bad_lines = 0


def read_jsonl(path: str, stats: Optional[ReadStats] = None) -> Iterator[dict]:
    with open(path, "r", encoding="utf-8", errors="replace") as fh:
        for line in fh:
            line = line.lstrip("\ufeff").strip()
            if not line:
                continue
            if '"segments"' not in line:
                try:
                    json.loads(line)
                except ValueError:
                    if stats is not None:
                        stats.bad_lines += 1
                continue
            try:
                obj = json.loads(line)
            except ValueError:
                if stats is not None:
                    stats.bad_lines += 1
                continue
            if isinstance(obj, dict) and isinstance(obj.get("segments"), list):
                yield obj
