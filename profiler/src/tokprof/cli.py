"""Command line interface."""

from __future__ import annotations

import argparse
import os
import sys
import time
from typing import Optional

from . import __version__


def _components(specs: list) -> dict:
    out: dict = {}
    for spec in specs or []:
        name, _, value = spec.partition("=")
        if not name or not value:
            raise SystemExit(f"--component expects NAME=@FILE or NAME=TEXT, got {spec!r}")
        if value.startswith("@"):
            with open(value[1:], "r", encoding="utf-8") as fh:
                value = fh.read()
        out[name] = value
    return out


def _default_out() -> str:
    return os.path.join(".tokprof", time.strftime("run-%Y%m%d-%H%M%S.jsonl"))


def _split_cmd(argv: list) -> tuple:
    if "--" in argv:
        i = argv.index("--")
        return argv[:i], argv[i + 1:]
    return argv, []


def build_parser() -> argparse.ArgumentParser:
    p = argparse.ArgumentParser(prog="tokprof", description="Token profiler for LLM agents")
    p.add_argument("--version", action="version", version=f"tokprof {__version__}")
    sub = p.add_subparsers(dest="cmd")

    run = sub.add_parser("run", help="run a command with LLM base URLs routed through the local gateway")
    run.add_argument("--out", help="JSONL output path (default .tokprof/run-<time>.jsonl)")
    run.add_argument("--component", action="append", metavar="NAME=@FILE", help="name a known static text so it is attributed separately")
    run.add_argument("--map", action="append", metavar="ENV=UPSTREAM", help="redirect another base-URL env var to this upstream URL")
    run.add_argument("--no-tiktoken", action="store_true", help="use the dependency-free heuristic counter")
    run.add_argument("--report", action="store_true", help="print a report when the command exits")

    sh = sub.add_parser("shell", help="open a shell whose child processes are profiled")
    sh.add_argument("--out")
    sh.add_argument("--component", action="append")
    sh.add_argument("--map", action="append")
    sh.add_argument("--no-tiktoken", action="store_true")
    sh.add_argument("--report", action="store_true")

    rep = sub.add_parser("report", help="summarize a recorded JSONL file")
    rep.add_argument("file")
    rep.add_argument("--format", choices=["text", "md", "json"], default="text")
    rep.add_argument("--gap", type=float, default=5.0, help="seconds of quiet that separate user requests")

    sub.add_parser("doctor", help="check this environment")
    sub.add_parser("selftest", help="verify the pipeline end to end with a local mock LLM")
    return p


def _report(path: str, fmt: str, gap: float) -> int:
    from .report import render_json, render_markdown, render_text
    from .store import ReadStats, read_jsonl

    stats = ReadStats()
    records = list(read_jsonl(path, stats))
    render = {"text": render_text, "md": render_markdown, "json": render_json}[fmt]
    print(render(records, gap))
    if stats.bad_lines:
        print(f"\n({stats.bad_lines} unreadable line(s) skipped)", file=sys.stderr)
    return 0


def main(argv: Optional[list] = None) -> int:
    argv = list(sys.argv[1:] if argv is None else argv)
    pre, cmd_tail = _split_cmd(argv)
    parser = build_parser()
    args = parser.parse_args(pre)

    if args.cmd in ("run", "shell"):
        from .runner import default_shell, run_profiled

        if args.cmd == "run" and not cmd_tail:
            parser.error("run needs a command after --, for example: tokprof run -- python agent.py")
        cmd = cmd_tail if args.cmd == "run" else (cmd_tail or default_shell())
        out = args.out or _default_out()
        code = run_profiled(
            cmd, out,
            components=_components(args.component),
            extra_maps=args.map,
            use_tiktoken=not args.no_tiktoken,
        )
        if args.report and os.path.exists(out):
            print()
            _report(out, "text", 5.0)
        elif os.path.exists(out):
            print(f"tokprof: view the breakdown with: tokprof report {out}", file=sys.stderr)
        return code
    if args.cmd == "report":
        return _report(args.file, args.format, args.gap)
    if args.cmd == "doctor":
        from .doctor import run_doctor

        return run_doctor()
    if args.cmd == "selftest":
        from .selftest import run_selftest

        return run_selftest()
    parser.print_help()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
