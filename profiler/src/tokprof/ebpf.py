"""Zero-change capture on Linux through the ``tokprof-ebpf`` helper.

The helper attaches eBPF probes to the TLS library of running processes, rebuilds the
HTTP/1.1 exchanges and prints one JSON object per LLM call. This module launches it
(with sudo when needed) and records those calls exactly like proxied traffic.
"""
import json
import os
import secrets
import shutil
import subprocess
import sys
from typing import Optional

from .gateway import Gateway
from .store import JsonlWriter

HELPER_NAME = "tokprof-ebpf"


def find_helper(explicit: Optional[str] = None) -> Optional[str]:
    candidates = [explicit, os.environ.get("TOKPROF_EBPF_HELPER"), shutil.which(HELPER_NAME)]
    here = os.path.dirname(os.path.abspath(__file__))
    candidates.append(os.path.normpath(os.path.join(here, "..", "..", "ebpf", "bin", HELPER_NAME)))  # source checkout
    for c in candidates:
        if c and os.path.isfile(c) and os.access(c, os.X_OK):
            return os.path.abspath(c)
    return None


def helper_command(helper: str, pids: str, comms: str, everything: bool, duration: Optional[str], use_sudo: bool, plain: bool = True) -> list:
    cmd = [helper, "trace", "--exit-on-stdin-eof"]
    if not plain:
        cmd.append("--no-plain")
    if pids:
        cmd += ["--pid", pids]
    if comms:
        cmd += ["--comm", comms]
    if everything:
        cmd.append("--all")
    if duration:
        cmd += ["--duration", duration]
    if use_sudo and hasattr(os, "geteuid") and os.geteuid() != 0 and shutil.which("sudo"):
        # Prompt on the terminal when there is one; never hang waiting for a password otherwise.
        cmd = ["sudo"] + ([] if sys.stderr.isatty() else ["-n"]) + cmd
    return cmd


def _ingest_lines(stream, gw: Gateway) -> int:
    n = 0
    for line in stream:
        line = line.strip()
        if not line:
            continue
        try:
            msg = json.loads(line)
        except ValueError:
            continue
        gw.ingest_message(msg)
        n += 1
    return n


def run_ebpf(
    out_path: str,
    *,
    pids: str = "",
    comms: str = "",
    everything: bool = False,
    helper: Optional[str] = None,
    duration: Optional[str] = None,
    components: Optional[dict] = None,
    use_tiktoken: bool = True,
    use_sudo: bool = True,
    plain: bool = True,
    quiet: bool = False,
) -> int:
    if not sys.platform.startswith("linux"):
        print("tokprof: eBPF capture is Linux-only. On this OS use `tokprof run` or `tokprof start` (gateway and hooks).", file=sys.stderr)
        return 2
    if not (pids or comms or everything):
        print("tokprof: choose what to watch: --pid, --comm or --all", file=sys.stderr)
        return 2
    exe = find_helper(helper)
    if not exe:
        print(f"tokprof: {HELPER_NAME} not found. Build it with: (cd profiler/ebpf && go build -o bin/{HELPER_NAME} ./cmd/{HELPER_NAME}), "
              "or pass --helper PATH.", file=sys.stderr)
        return 2

    writer = JsonlWriter(out_path)
    gw = Gateway([], writer.write, salt=secrets.token_bytes(16), components=components, run_id=secrets.token_hex(4), use_tiktoken=use_tiktoken)
    cmd = helper_command(exe, pids, comms, everything, duration, use_sudo, plain)
    if not quiet:
        print(f"tokprof: profiling to {out_path} via eBPF; press Ctrl+C to stop", file=sys.stderr)
    try:
        proc = subprocess.Popen(cmd, stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True, encoding="utf-8", errors="replace")
    except OSError as exc:
        print(f"tokprof: cannot start the helper: {exc}", file=sys.stderr)
        writer.close()
        return 127
    try:
        _ingest_lines(proc.stdout, gw)
    except KeyboardInterrupt:
        pass
    code = 1
    try:
        # Closing stdin asks the helper to detach its probes and flush pending exchanges.
        try:
            proc.stdin.close()
        except OSError:
            pass
        _ingest_lines(proc.stdout, gw)
        code = proc.wait(timeout=15)
    except (subprocess.TimeoutExpired, KeyboardInterrupt):
        proc.kill()
    finally:
        writer.close()
    if not quiet:
        s = gw.stats
        print(f"tokprof: {s['recorded']} LLM call(s) recorded, {s['record_errors']} recording error(s)", file=sys.stderr)
        summary = gw.detector.render_summary()
        if summary:
            print(summary, file=sys.stderr)
    return code
