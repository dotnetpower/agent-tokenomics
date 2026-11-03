#!/usr/bin/env python3
"""Fetch bibliographic metadata for examples/papers.json from Semantic Scholar.

Writes examples/papers-meta.json (title, year, venue, citation count, retrieval date).
Citation counts change; re-run to refresh, then run build_papers.py.

Usage:
    python3 fetch_paper_meta.py            # optional: export S2_API_KEY for higher rate limits
"""

from __future__ import annotations

import datetime
import json
import os
import sys
import time
import urllib.error
import urllib.request
from pathlib import Path

HERE = Path(__file__).parent
URL = (
    "https://api.semanticscholar.org/graph/v1/paper/batch"
    "?fields=title,year,venue,citationCount,externalIds"
)


def fetch(ids: list[str]) -> list[dict | None]:
    body = json.dumps({"ids": [f"ARXIV:{i}" for i in ids]}).encode()
    headers = {"Content-Type": "application/json"}
    if key := os.environ.get("S2_API_KEY"):
        headers["x-api-key"] = key
    for attempt in range(6):
        try:
            req = urllib.request.Request(URL, data=body, headers=headers)
            return json.load(urllib.request.urlopen(req, timeout=90))
        except urllib.error.HTTPError as exc:
            if exc.code != 429:
                raise
            wait = 30 * (attempt + 1)
            print(f"rate limited, retrying in {wait}s", file=sys.stderr)
            time.sleep(wait)
    sys.exit("Semantic Scholar rate limit; try again later or set S2_API_KEY")


def main() -> None:
    papers = json.loads((HERE / "papers.json").read_text())
    ids = [p["id"] for p in papers]
    rows = fetch(ids)
    meta = {}
    for pid, row in zip(ids, rows):
        if row is None:
            sys.exit(f"not found on Semantic Scholar: arXiv:{pid}")
        meta[pid] = {
            "title": row["title"],
            "year": row["year"],
            "venue": row.get("venue") or "",
            "citations": row["citationCount"],
        }
    out = {"retrieved": datetime.date.today().isoformat(), "source": "Semantic Scholar Graph API", "papers": meta}
    (HERE / "papers-meta.json").write_text(json.dumps(out, indent=1, ensure_ascii=False) + "\n")
    print(f"wrote {len(meta)} records")


if __name__ == "__main__":
    main()
