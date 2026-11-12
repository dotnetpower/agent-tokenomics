#!/usr/bin/env python3
"""Turn a `tokprof report --csv` export into a draft Golden Dataset CSV.

Step 1 (baseline): one row per user turn is built from the captured prompts and
answers; `must_include_all` is pre-filled with number-like tokens found in the
baseline answer as *suggestions* for a human to confirm.

    python3 examples/tokprof_to_golden.py baseline-run.csv -o draft.csv

Step 2 (candidate): after changing the agent, record the same scenario again and
copy the new answer and token counts into the `candidate_*` columns by `case_id`.

    python3 examples/tokprof_to_golden.py candidate-run.csv --merge draft.csv -o golden.csv

Standard library only. Review every row for personal data before sharing.
"""

from __future__ import annotations

import argparse
import csv
import re
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from golden_dataset_tool import CSV_COLUMNS  # noqa: E402

NUMBER_LIKE = re.compile(r"\d[\d,.\-]{2,}\d")


def _int(val: str) -> int:
    try:
        return int(float(val or 0))
    except ValueError:
        return 0


def read_turns(path: Path) -> list[dict[str, str | int]]:
    """Collapse tokprof `call` rows into one record per (session, turn)."""
    with path.open("r", encoding="utf-8-sig", newline="") as f:
        calls = [r for r in csv.DictReader(f) if r.get("row_type") == "call"]

    grouped: dict[tuple[str, str], list[dict[str, str]]] = {}
    for row in calls:
        grouped.setdefault((row["session"], row["turn"]), []).append(row)

    turns: list[dict[str, str | int]] = []
    for (session, turn), rows in grouped.items():
        rows.sort(key=lambda r: _int(r["call_index"]))
        first = rows[0]
        answers = [r["assistant_response"] for r in rows if r.get("assistant_response")]
        turns.append(
            {
                "session": session,
                "turn": int(turn),
                "history": first.get("conversation_history", ""),
                "user_input": first.get("user_prompt", ""),
                "response": answers[-1] if answers else "",
                # Cost of the whole turn: every call in the agent loop is billed.
                "tokens": sum(_int(r["input_total"]) + _int(r["output_tokens"]) for r in rows),
            }
        )
    turns.sort(key=lambda t: (str(t["session"]), int(t["turn"])))
    return turns


def case_id(turn: dict[str, str | int]) -> str:
    return f"{turn['session']}-T{turn['turn']}"


def build_draft(turns: list[dict[str, str | int]]) -> list[dict[str, str]]:
    rows = []
    for t in turns:
        suggested = ";".join(dict.fromkeys(NUMBER_LIKE.findall(str(t["response"]))))
        row = {col: "" for col in CSV_COLUMNS}
        row.update(
            case_id=case_id(t),
            session_id=str(t["session"]),
            turn_index=str(t["turn"]),
            slice="tokprof_capture",
            match_mode="hybrid",
            prior_turns_context=str(t["history"]),
            user_input=str(t["user_input"]),
            must_include_all=suggested,
            expected_intent="TODO: one sentence describing what a correct answer must convey",
            baseline_response=str(t["response"]),
            baseline_tokens=str(t["tokens"]),
        )
        rows.append(row)
    return rows


def merge_candidate(dataset: list[dict[str, str]], turns: list[dict[str, str | int]]) -> list[str]:
    """Fill candidate_* columns in place; return case_ids that had no match."""
    by_id = {case_id(t): t for t in turns}
    missing = []
    for row in dataset:
        t = by_id.get(row["case_id"])
        if t is None:
            missing.append(row["case_id"])
            continue
        row["candidate_response"] = str(t["response"])
        row["candidate_tokens"] = str(t["tokens"])
    return missing


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("tokprof_csv", type=Path, help="CSV from `tokprof report --csv`")
    ap.add_argument("-o", "--out", type=Path, required=True, help="Output CSV path")
    ap.add_argument("--merge", type=Path, help="Existing draft/golden CSV to fill candidate_* columns in")
    args = ap.parse_args()

    turns = read_turns(args.tokprof_csv)
    if args.merge:
        with args.merge.open("r", encoding="utf-8-sig", newline="") as f:
            rows = list(csv.DictReader(f))
        missing = merge_candidate(rows, turns)
        for cid in missing:
            print(f"warning: no candidate turn for {cid}", file=sys.stderr)
    else:
        rows = build_draft(turns)

    with args.out.open("w", encoding="utf-8-sig", newline="") as f:
        writer = csv.DictWriter(f, fieldnames=CSV_COLUMNS)
        writer.writeheader()
        writer.writerows(rows)
    print(f"wrote {len(rows)} row(s) -> {args.out}")


if __name__ == "__main__":
    main()
