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
    run.add_argument("--no-hooks", action="store_true", help="do not load the in-process Python/Node hooks")
    run.add_argument("--no-gateway-env", action="store_true", help="do not redirect *_BASE_URL environment variables")

    sh = sub.add_parser("shell", help="open a shell whose child processes are profiled")
    sh.add_argument("--out")
    sh.add_argument("--component", action="append")
    sh.add_argument("--map", action="append")
    sh.add_argument("--no-tiktoken", action="store_true")
    sh.add_argument("--report", action="store_true")
    sh.add_argument("--no-hooks", action="store_true")
    sh.add_argument("--no-gateway-env", action="store_true")

    rep = sub.add_parser("report", help="summarize a recorded JSONL file")
    rep.add_argument("file")
    rep.add_argument("--format", choices=["text", "md", "json"], default="text")
    rep.add_argument("--gap", type=float, default=5.0, help="seconds of quiet that separate user requests")
    rep.add_argument("--prices", help="JSON price file: {model: {input, cached, output}} per 1M tokens")

    cmp_ = sub.add_parser("compare", help="compare two recorded runs (before, after) with a bootstrap interval")
    cmp_.add_argument("before")
    cmp_.add_argument("after")
    cmp_.add_argument("--gap", type=float, default=5.0)
    cmp_.add_argument("--json", action="store_true")

    exp = sub.add_parser("export", help="export records in the JSONL schema of examples/token_report.py")
    exp.add_argument("file")
    exp.add_argument("--out")
    exp.add_argument("--gap", type=float, default=5.0)

    for name, helptext in (("start", "start a background profiling session"), ("serve", "run the session gateway in the foreground")):
        sp = sub.add_parser(name, help=helptext)
        sp.add_argument("--out", help="JSONL output path")
        sp.add_argument("--state", help="session state file (default ~/.tokprof/session.json)")
        sp.add_argument("--map", action="append")
        sp.add_argument("--no-tiktoken", action="store_true")
    st = sub.add_parser("status", help="show the active session")
    st.add_argument("--state")
    en = sub.add_parser("env", help="print the environment that routes new processes through the session")
    en.add_argument("--state")
    en.add_argument("--shell", choices=["bash", "fish", "powershell", "cmd"])
    en.add_argument("--unset", action="store_true", help="print commands that restore the previous values")
    sp_stop = sub.add_parser("stop", help="stop the active session")
    sp_stop.add_argument("--state")
    sp_stop.add_argument("--force", action="store_true")

    sub.add_parser("doctor", help="check this environment")
    sub.add_parser("selftest", help="verify the pipeline end to end with a local mock LLM")
    return p


def _report(path: str, fmt: str, gap: float, prices: Optional[str] = None) -> int:
    from .report import render_json, render_markdown, render_text
    from .store import ReadStats, read_jsonl

    stats = ReadStats()
    records = list(read_jsonl(path, stats))
    render = {"text": render_text, "md": render_markdown, "json": render_json}[fmt]
    print(render(records, gap))
    if prices and records:
        import json as _json

        from .report import render_cost

        with open(prices, "r", encoding="utf-8") as fh:
            print("\n" + render_cost(records, _json.load(fh)))
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
            gateway_env=not args.no_gateway_env,
            hooks=not args.no_hooks,
        )
        if args.report and os.path.exists(out):
            print()
            _report(out, "text", 5.0)
        elif os.path.exists(out):
            print(f"tokprof: view the breakdown with: tokprof report {out}", file=sys.stderr)
        return code
    if args.cmd == "report":
        return _report(args.file, args.format, args.gap, args.prices)
    if args.cmd == "compare":
        import json as _json

        from .analyze import compare
        from .report import render_compare
        from .store import read_jsonl

        result = compare(list(read_jsonl(args.before)), list(read_jsonl(args.after)), args.gap)
        print(_json.dumps(result, indent=2) if args.json else render_compare(result))
        return 0
    if args.cmd == "export":
        import json as _json

        from .analyze import to_legacy
        from .store import read_jsonl

        rows = to_legacy(list(read_jsonl(args.file)), args.gap)
        text = "\n".join(_json.dumps(r, ensure_ascii=False) for r in rows) + ("\n" if rows else "")
        if args.out:
            with open(args.out, "w", encoding="utf-8") as fh:
                fh.write(text)
        else:
            sys.stdout.write(text)
        return 0
    if args.cmd in ("start", "serve", "status", "env", "stop"):
        from . import session

        state = session.state_path(getattr(args, "state", None))
        if args.cmd in ("start", "serve"):
            out = os.path.abspath(args.out or _default_out())
            fn = session.start if args.cmd == "start" else session.serve
            return fn(out, state, args.map, not args.no_tiktoken)
        if args.cmd == "status":
            return session.status(state)
        if args.cmd == "env":
            return session.env(state, args.shell, args.unset)
        return session.stop(state, args.force)
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
