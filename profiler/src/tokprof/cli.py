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


def _find_latest_jsonl(directory: str = ".tokprof") -> Optional[str]:
    candidates = []
    if os.path.isdir(directory):
        for name in os.listdir(directory):
            if name.endswith(".jsonl"):
                p = os.path.join(directory, name)
                if os.path.isfile(p):
                    candidates.append(p)
    if candidates:
        candidates.sort(key=lambda p: os.path.getmtime(p), reverse=True)
        return candidates[0]
    try:
        from . import session

        st = session._read_state(session.state_path())
        if st and st.get("out") and os.path.isfile(st["out"]):
            return st["out"]
    except Exception:
        pass
    return None


def _split_cmd(argv: list) -> tuple:
    if "--" in argv:
        i = argv.index("--")
        return argv[:i], argv[i + 1:]
    return argv, []


def _add_detect_args(sp) -> None:
    g = sp.add_argument_group("endpoint detection")
    g.add_argument("--llm-host", action="append", metavar="HOST[:PORT]", help="force these hosts to be treated as LLM endpoints (glob such as *.corp, or a CIDR such as 10.0.0.0/8); repeatable")
    g.add_argument("--llm-path", action="append", metavar="REGEX", help="force requests whose path matches this regular expression to be treated as LLM calls; repeatable")
    g.add_argument("--exclude-host", action="append", metavar="HOST[:PORT]", help="never treat these hosts as LLM endpoints; repeatable")
    g.add_argument("--upstream-ca", metavar="FILE", help="extra CA bundle the gateway trusts for upstream TLS (private CA); also read from SSL_CERT_FILE, REQUESTS_CA_BUNDLE, NODE_EXTRA_CA_CERTS")
    g.add_argument("--insecure-upstream", action="store_true", help="do NOT verify upstream TLS certificates (last resort for private endpoints; prints a warning)")
    g.add_argument("--no-auto-env", action="store_true", help="do not redirect LLM base-URL variables other than OPENAI/ANTHROPIC/AZURE (for example VLLM_BASE_URL, OLLAMA_HOST)")
    g.add_argument("--record-hosts", action="store_true", help="store host names in the JSONL file (by default only a salted hash and a class such as private-dns)")
    g.add_argument("--tokenizer", metavar="SPEC", help="tokenizer for local segment counting: o200k_base, cl100k_base, a path to tokenizer.json, or a HuggingFace repo ID")


def _apply_detect_args(args) -> None:
    """Detection settings travel through environment variables so every capture path and the background session share them."""
    for attr, var in (("llm_host", "TOKPROF_LLM_HOSTS"), ("llm_path", "TOKPROF_LLM_PATHS"), ("exclude_host", "TOKPROF_EXCLUDE_HOSTS")):
        values = getattr(args, attr, None)
        if values:
            existing = os.environ.get(var, "")
            os.environ[var] = ",".join(x for x in [existing] + list(values) if x)
    if getattr(args, "upstream_ca", None):
        existing = os.environ.get("TOKPROF_UPSTREAM_CA", "")
        os.environ["TOKPROF_UPSTREAM_CA"] = os.pathsep.join(x for x in [existing, args.upstream_ca] if x)
    if getattr(args, "insecure_upstream", False):
        os.environ["TOKPROF_INSECURE_UPSTREAM"] = "1"
    if getattr(args, "no_auto_env", False):
        os.environ["TOKPROF_NO_AUTO_ENV"] = "1"
    if getattr(args, "record_hosts", False):
        os.environ["TOKPROF_RECORD_HOSTS"] = "1"
    if getattr(args, "tokenizer", None):
        os.environ["TOKPROF_TOKENIZER"] = args.tokenizer


def build_parser() -> argparse.ArgumentParser:
    p = argparse.ArgumentParser(prog="tokprof", description="Token profiler for LLM agents")
    p.add_argument("--version", action="version", version=f"tokprof {__version__}")
    sub = p.add_subparsers(dest="cmd")

    prof = sub.add_parser(
        "profile",
        help="one-step profile: run a command or attach to a running process, then print a visual report and save an HTML dashboard",
    )
    prof.add_argument("--pid", default="", help="comma-separated PIDs of already-running processes to watch")
    prof.add_argument("--comm", default="", help="comma-separated process names to watch")
    prof.add_argument("--all", action="store_true", help="watch every process on this machine (Linux eBPF)")
    prof.add_argument("--duration", help="stop after this long (for example 30s or 5m)")
    prof.add_argument("--out", help="JSONL output path (default .tokprof/run-<time>.jsonl)")
    prof.add_argument("--html", nargs="?", const="", default="", metavar="FILE", help="HTML dashboard path (default: <out>.html and .tokprof/latest.html)")
    prof.add_argument("--no-html", action="store_true", help="do not write the HTML dashboard")
    prof.add_argument("--open", action="store_true", help="open the HTML dashboard in a browser when finished")
    prof.add_argument("--prices", help="JSON price file per 1M tokens")
    prof.add_argument("--component", action="append", metavar="NAME=@FILE")
    prof.add_argument("--map", action="append", metavar="ENV=UPSTREAM")
    prof.add_argument("--no-tiktoken", action="store_true")
    prof.add_argument("--no-hooks", action="store_true")
    prof.add_argument("--no-gateway-env", action="store_true")
    prof.add_argument("command", nargs=argparse.REMAINDER, help="command to run (when not watching --pid/--comm/--all)")
    _add_detect_args(prof)

    run = sub.add_parser("run", help="run a command with LLM base URLs routed through the local gateway")
    run.add_argument("--out", help="JSONL output path (default .tokprof/run-<time>.jsonl)")
    run.add_argument("--component", action="append", metavar="NAME=@FILE", help="name a known static text so it is attributed separately")
    run.add_argument("--map", action="append", metavar="ENV=UPSTREAM", help="redirect another base-URL env var to this upstream URL")
    run.add_argument("--no-tiktoken", action="store_true", help="use the dependency-free heuristic counter")
    run.add_argument("--report", action="store_true", help="print a report when the command exits")
    run.add_argument("--html", nargs="?", const="", metavar="FILE", help="write a self-contained HTML dashboard when the command exits")
    run.add_argument("--open", action="store_true", help="open the HTML dashboard in a browser")
    run.add_argument("--no-hooks", action="store_true", help="do not load the in-process Python/Node hooks")
    run.add_argument("--no-gateway-env", action="store_true", help="do not redirect *_BASE_URL environment variables")
    run.add_argument("command", nargs=argparse.REMAINDER, help="command to run (with or without --)")

    _add_detect_args(run)

    sh = sub.add_parser("shell", help="open a shell whose child processes are profiled")
    sh.add_argument("--out")
    sh.add_argument("--component", action="append")
    sh.add_argument("--map", action="append")
    sh.add_argument("--no-tiktoken", action="store_true")
    sh.add_argument("--report", action="store_true")
    sh.add_argument("--html", nargs="?", const="", metavar="FILE")
    sh.add_argument("--open", action="store_true")
    sh.add_argument("--no-hooks", action="store_true")
    sh.add_argument("--no-gateway-env", action="store_true")

    eb = sub.add_parser("ebpf", help="Linux: capture LLM calls of running processes with eBPF, without changing or restarting them")
    eb.add_argument("--pid", default="", help="comma-separated PIDs to watch")
    eb.add_argument("--comm", default="", help="comma-separated substrings matched against process name, path and command line")
    eb.add_argument("--all", action="store_true", help="watch every process (records all LLM calls on this machine)")
    eb.add_argument("--out")
    eb.add_argument("--component", action="append", metavar="NAME=@FILE")
    eb.add_argument("--helper", help="path to the tokprof-ebpf binary")
    eb.add_argument("--duration", help="stop after this long, for example 5m")
    eb.add_argument("--no-sudo", action="store_true", help="do not elevate; use when already root or the helper has capabilities")
    eb.add_argument("--no-plain", action="store_true", help="only observe TLS libraries; skip the syscall probes that see cleartext http:// traffic")
    eb.add_argument("--no-tiktoken", action="store_true")
    eb.add_argument("--report", action="store_true")
    eb.add_argument("--html", nargs="?", const="", metavar="FILE")
    eb.add_argument("--open", action="store_true")

    mm = sub.add_parser("mitm", help="capture LLM calls of selected processes through mitmproxy (Windows, macOS, or any OS)")
    mm.add_argument("--local", default="", help="comma-separated process names or PIDs for mitmproxy local capture")
    mm.add_argument("--mode", default="", help="any mitmproxy mode instead, for example regular@8080")
    mm.add_argument("--mitmdump", help="path to mitmdump")
    mm.add_argument("--duration", type=float, help="stop after this many seconds")
    mm.add_argument("--out")
    mm.add_argument("--component", action="append", metavar="NAME=@FILE")
    mm.add_argument("--no-tiktoken", action="store_true")
    mm.add_argument("--report", action="store_true")
    mm.add_argument("--html", nargs="?", const="", metavar="FILE")
    mm.add_argument("--open", action="store_true")
    mm.add_argument("--trust-help", action="store_true", help="explain how processes can trust the mitmproxy CA, then exit")

    _add_detect_args(sh)
    _add_detect_args(eb)
    _add_detect_args(mm)

    ex = sub.add_parser("explain", help="show how tokprof would classify one request, and why")
    ex.add_argument("url", help="full request URL, for example http://vllm.internal:8000/v1/chat/completions")
    ex.add_argument("--body", help="JSON file with the request body (default: read stdin when piped)")
    ex.add_argument("--response", help="file with the response body, to settle an ambiguous request")
    ex.add_argument("--sse", action="store_true", help="the response file is a server-sent-event stream")
    _add_detect_args(ex)

    rep = sub.add_parser("report", help="summarize a recorded JSONL file (defaults to the latest run in .tokprof/)")
    rep.add_argument("file", nargs="?", help="JSONL file to summarize (default: most recent .tokprof/run-*.jsonl)")
    rep.add_argument("--format", choices=["text", "pretty", "md", "json", "html"], default="text")
    rep.add_argument("--out", help="write the formatted report to this file instead of stdout")
    rep.add_argument("--html", nargs="?", const="", metavar="FILE", help="also write a self-contained HTML dashboard (default: <file>.html)")
    rep.add_argument("--open", action="store_true", help="open the HTML dashboard in a browser")
    rep.add_argument("--watch", action="store_true", help="refresh the report in the terminal as new calls arrive")
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

    for name, helptext in (("start", "start a background profiling session (auto-attaches Linux eBPF to running processes & Docker)"), ("serve", "run the session gateway in the foreground")):
        sp = sub.add_parser(name, help=helptext)
        sp.add_argument("--out", help="JSONL output path")
        sp.add_argument("--state", help="session state file (default ~/.tokprof/session.json)")
        sp.add_argument("--map", action="append")
        sp.add_argument("--pid", default="", help="comma-separated PIDs to watch (default: all processes & Docker containers)")
        sp.add_argument("--comm", default="", help="comma-separated process/container/cwd substrings to watch (for example: fdai)")
        sp.add_argument("--ebpf", action="store_true", help="enable the Linux eBPF tap in the background session")
        sp.add_argument("--no-ebpf", action="store_true", help="disable the Linux eBPF tap (gateway only)")
        sp.add_argument("--no-tiktoken", action="store_true")
        _add_detect_args(sp)
    st = sub.add_parser("status", help="show the active session")
    st.add_argument("--state")
    en = sub.add_parser("env", help="print the environment that routes new processes through the session")
    en.add_argument("--state")
    en.add_argument("--shell", choices=["bash", "fish", "powershell", "cmd"])
    en.add_argument("--unset", action="store_true", help="print commands that restore the previous values")
    sp_stop = sub.add_parser("stop", help="stop the active session and emit the token profile report")
    sp_stop.add_argument("--state")
    sp_stop.add_argument("--force", action="store_true")
    sp_stop.add_argument("--report", action="store_true", help="print a report after stopping")
    sp_stop.add_argument("--no-report", action="store_true", help="do not print a report after stopping")
    sp_stop.add_argument("--html", nargs="?", const="", metavar="FILE", help="write an HTML dashboard after stopping")
    sp_stop.add_argument("--no-html", action="store_true", help="do not write an HTML dashboard")
    sp_stop.add_argument("--open", action="store_true", help="open the HTML dashboard in a browser")

    sub.add_parser("doctor", help="check this environment")
    sub.add_parser("selftest", help="verify the pipeline end to end with a local mock LLM")
    return p


def _load_prices(prices: Optional[str]) -> Optional[dict]:
    if not prices:
        return None
    import json as _json

    with open(prices, "r", encoding="utf-8") as fh:
        return _json.load(fh)


def _write_html_dashboard(
    jsonl_path: str,
    html_arg: Optional[str] = "",
    *,
    gap: float = 5.0,
    prices: Optional[str] = None,
    open_browser: bool = False,
) -> Optional[str]:
    from .report import render_html
    from .store import read_jsonl

    if not os.path.exists(jsonl_path):
        return None
    records = list(read_jsonl(jsonl_path))
    if not records:
        return None
    stem = jsonl_path[:-6] if jsonl_path.endswith(".jsonl") else jsonl_path
    dest = html_arg if html_arg else (stem + ".html")
    parent = os.path.dirname(os.path.abspath(dest))
    os.makedirs(parent, exist_ok=True)
    content = render_html(records, gap, prices=_load_prices(prices))
    with open(dest, "w", encoding="utf-8") as fh:
        fh.write(content)
    if not html_arg and os.path.basename(os.path.dirname(os.path.abspath(jsonl_path))) == ".tokprof":
        latest = os.path.join(os.path.dirname(os.path.abspath(jsonl_path)), "latest.html")
        try:
            with open(latest, "w", encoding="utf-8") as fh:
                fh.write(content)
        except OSError:
            pass
    print(f"tokprof: HTML dashboard written to {dest}", file=sys.stderr)
    if open_browser:
        try:
            import webbrowser

            webbrowser.open("file://" + os.path.abspath(dest))
        except Exception:
            pass
    return dest


def _finish_run(out: str, args, *, default_report: bool = False, pretty: bool = False) -> None:
    if not os.path.exists(out):
        return
    want_report = getattr(args, "report", False) or default_report
    html_arg = getattr(args, "html", None)
    want_html = (html_arg is not None) and not getattr(args, "no_html", False)
    if want_report:
        print()
        _report(out, "pretty" if pretty else "text", 5.0, getattr(args, "prices", None))
    if want_html:
        _write_html_dashboard(out, html_arg, prices=getattr(args, "prices", None), open_browser=getattr(args, "open", False))
    if not want_report and not want_html:
        print(f"tokprof: view the breakdown with: tokprof report   (or: tokprof report --html)", file=sys.stderr)


def _watch_report(path: str, gap: float, prices: Optional[str] = None) -> int:
    try:
        while True:
            sys.stdout.write("\033[2J\033[H")
            _report(path, "pretty", gap, prices)
            print(f"\nWatching {path} (Ctrl+C to stop)...", file=sys.stderr)
            time.sleep(1.0)
    except KeyboardInterrupt:
        return 0


def _report(
    path: Optional[str],
    fmt: str,
    gap: float,
    prices: Optional[str] = None,
    *,
    out_file: Optional[str] = None,
    html_arg: Optional[str] = None,
    open_browser: bool = False,
    watch: bool = False,
) -> int:
    from .report import render_html, render_json, render_markdown, render_pretty, render_text
    from .store import ReadStats, read_jsonl

    if not path:
        path = _find_latest_jsonl()
        if not path:
            print("tokprof: no recorded JSONL file found in .tokprof/; pass a file path or run `tokprof profile -- <cmd>` first", file=sys.stderr)
            return 2
    if watch:
        return _watch_report(path, gap, prices)

    stats = ReadStats()
    records = list(read_jsonl(path, stats))
    price_map = _load_prices(prices)
    if fmt == "html":
        text = render_html(records, gap, prices=price_map)
    elif fmt == "pretty":
        text = render_pretty(records, gap, prices=price_map)
    else:
        render = {"text": render_text, "md": render_markdown, "json": render_json}[fmt]
        text = render(records, gap)
    if out_file:
        os.makedirs(os.path.dirname(os.path.abspath(out_file)), exist_ok=True)
        with open(out_file, "w", encoding="utf-8") as fh:
            fh.write(text + ("\n" if not text.endswith("\n") else ""))
    else:
        print(text)
        if price_map and records and fmt in ("text", "md"):
            from .report import render_cost

            print("\n" + render_cost(records, price_map))
    if html_arg is not None:
        _write_html_dashboard(path, html_arg, gap=gap, prices=prices, open_browser=open_browser)
    elif open_browser and fmt == "html" and out_file:
        try:
            import webbrowser

            webbrowser.open("file://" + os.path.abspath(out_file))
        except Exception:
            pass
    if stats.bad_lines:
        print(f"\n({stats.bad_lines} unreadable line(s) skipped)", file=sys.stderr)
    return 0


def main(argv: Optional[list] = None) -> int:
    argv = list(sys.argv[1:] if argv is None else argv)
    pre, cmd_tail = _split_cmd(argv)
    if not pre and cmd_tail:
        pre = ["profile"]
    parser = build_parser()
    args = parser.parse_args(pre)
    _apply_detect_args(args)

    if args.cmd == "profile":
        cmd = cmd_tail or list(getattr(args, "command", None) or [])
        if cmd and cmd[0] == "--":
            cmd = cmd[1:]
        out = args.out or _default_out()
        if args.pid or args.comm or args.all:
            if sys.platform.startswith("linux"):
                from .ebpf import run_ebpf

                code = run_ebpf(
                    out, pids=args.pid, comms=args.comm, everything=args.all, duration=args.duration,
                    components=_components(args.component), use_tiktoken=not args.no_tiktoken,
                )
            else:
                from .mitm import run_mitm

                local_spec = ",".join(x for x in (args.pid, args.comm) if x)
                dur = float(args.duration.rstrip("s")) if (args.duration and args.duration.rstrip("s").replace(".", "", 1).isdigit()) else None
                code = run_mitm(
                    out, local=local_spec, components=_components(args.component),
                    use_tiktoken=not args.no_tiktoken, duration=dur,
                )
        else:
            if not cmd:
                parser.error("profile needs a command (for example: tokprof profile -- python agent.py) or --pid/--comm")
            from .runner import run_profiled

            code = run_profiled(
                cmd, out,
                components=_components(args.component),
                extra_maps=args.map,
                use_tiktoken=not args.no_tiktoken,
                gateway_env=not args.no_gateway_env,
                hooks=not args.no_hooks,
            )
        _finish_run(out, args, default_report=True, pretty=True)
        return code

    if args.cmd in ("run", "shell"):
        from .runner import default_shell, run_profiled

        inline_cmd = list(getattr(args, "command", None) or [])
        if inline_cmd and inline_cmd[0] == "--":
            inline_cmd = inline_cmd[1:]
        eff_tail = cmd_tail or inline_cmd
        if args.cmd == "run" and not eff_tail:
            parser.error("run needs a command after --, for example: tokprof run -- python agent.py")
        cmd = eff_tail if args.cmd == "run" else (eff_tail or default_shell())
        out = args.out or _default_out()
        code = run_profiled(
            cmd, out,
            components=_components(args.component),
            extra_maps=args.map,
            use_tiktoken=not args.no_tiktoken,
            gateway_env=not args.no_gateway_env,
            hooks=not args.no_hooks,
        )
        _finish_run(out, args)
        return code
    if args.cmd == "ebpf":
        from .ebpf import run_ebpf

        out = args.out or _default_out()
        code = run_ebpf(
            out, pids=args.pid, comms=args.comm, everything=args.all, helper=args.helper, duration=args.duration,
            components=_components(args.component), use_tiktoken=not args.no_tiktoken, use_sudo=not args.no_sudo, plain=not args.no_plain,
        )
        _finish_run(out, args)
        return code
    if args.cmd == "mitm":
        from .mitm import TRUST_HELP, run_mitm

        if args.trust_help:
            print(TRUST_HELP)
            return 0
        out = args.out or _default_out()
        code = run_mitm(
            out, local=args.local, mode=args.mode, mitmdump=args.mitmdump, components=_components(args.component),
            use_tiktoken=not args.no_tiktoken, extra=cmd_tail, duration=args.duration,
        )
        _finish_run(out, args)
        return code
    if args.cmd == "report":
        return _report(
            args.file, args.format, args.gap, args.prices,
            out_file=args.out, html_arg=args.html, open_browser=args.open, watch=args.watch,
        )
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
            use_ebpf = True if args.ebpf else (False if args.no_ebpf else (args.state is None and sys.platform.startswith("linux")))
            fn = session.start if args.cmd == "start" else session.serve
            return fn(out, state, args.map, not args.no_tiktoken, use_ebpf=use_ebpf, pids=args.pid, comms=args.comm)
        if args.cmd == "status":
            return session.status(state)
        if args.cmd == "env":
            return session.env(state, args.shell, args.unset)
        prev = session._read_state(state)
        rc = session.stop(state, args.force)
        if rc == 0 and prev and prev.get("out") and not getattr(args, "no_report", False):
            auto_report = bool(prev.get("ebpf")) or args.state is None
            if args.html is None and auto_report and not getattr(args, "no_html", False):
                args.html = ""
            _finish_run(prev["out"], args, default_report=auto_report, pretty=True)
        return rc
    if args.cmd == "explain":
        import json as _json

        from .detect import explain

        raw = None
        if args.body:
            with open(args.body, encoding="utf-8") as fh:
                raw = fh.read()
        elif not sys.stdin.isatty():
            raw = sys.stdin.read()
        try:
            body = _json.loads(raw) if raw else None
        except ValueError:
            print("tokprof: the request body is not valid JSON", file=sys.stderr)
            return 2
        response = None
        if args.response:
            with open(args.response, encoding="utf-8") as fh:
                response = fh.read()
        print(explain(args.url, body, response=response, sse=args.sse))
        return 0
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
