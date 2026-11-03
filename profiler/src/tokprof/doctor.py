"""Environment checks: which capture backends can work here, and what is missing."""

from __future__ import annotations

import os
import platform
import shutil
import socket
import sys


def _port_free() -> bool:
    try:
        with socket.socket() as s:
            s.bind(("127.0.0.1", 0))
        return True
    except OSError:
        return False


def _ebpf_status() -> tuple:
    if sys.platform != "linux":
        return False, "eBPF tap is Linux-only"
    notes = []
    ok = True
    if not os.path.exists("/sys/kernel/btf/vmlinux"):
        ok = False
        notes.append("no kernel BTF (/sys/kernel/btf/vmlinux)")
    try:
        import gzip

        with gzip.open("/proc/config.gz", "rt") as fh:
            cfg = fh.read()
        for opt in ("CONFIG_BPF_SYSCALL=y", "CONFIG_UPROBES=y"):
            if opt not in cfg:
                ok = False
                notes.append(f"kernel option missing: {opt}")
    except Exception:
        notes.append("kernel config not readable (/proc/config.gz); cannot confirm options")
    if hasattr(os, "geteuid") and os.geteuid() != 0:
        notes.append("not root: eBPF needs sudo or CAP_BPF/CAP_PERFMON")
    return ok, "; ".join(notes) or "ready"


def run_doctor() -> int:
    from . import __version__
    from .counting import get_counter

    rows = []

    def add(name: str, ok: bool, detail: str = "") -> None:
        rows.append((name, ok, detail))

    add("platform", True, f"{platform.system()} {platform.release()} ({platform.machine()})")
    add("python", sys.version_info >= (3, 9), platform.python_version())
    add("local port available", _port_free(), "gateway binds 127.0.0.1 on a free port")
    counter = get_counter("gpt-4o")
    add("token counter", True, counter.method + ("" if counter.method != "heuristic" else " (install tiktoken for better accuracy)"))
    add("node (for Node-based agents)", shutil.which("node") is not None, shutil.which("node") or "not found; not required")
    redirected = [v for v in ("OPENAI_BASE_URL", "ANTHROPIC_BASE_URL", "AZURE_OPENAI_ENDPOINT") if os.environ.get(v)]
    add("existing base-URL env vars", True, ", ".join(redirected) + " (kept as upstream)" if redirected else "none")
    ebpf_ok, ebpf_note = _ebpf_status()
    add("eBPF TLS tap (Linux, later release)", ebpf_ok, ebpf_note)

    print(f"tokprof {__version__} doctor")
    for name, ok, detail in rows:
        print(f"  [{'ok' if ok else '--'}] {name}: {detail}")
    print()
    print("Available capture backends:")
    print("  gateway (tokprof run / shell): yes, for processes started under tokprof that honor *_BASE_URL env vars")
    print(f"  eBPF tap for running processes: {'possible' if ebpf_ok else 'not available'} (not implemented yet)")
    return 0
