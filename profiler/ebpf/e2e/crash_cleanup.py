#!/usr/bin/env python3
"""Check that the kernel removes the eBPF probes when the helper is killed with SIGKILL.

    python3 e2e/crash_cleanup.py --helper bin/tokprof-ebpf [--sudo]
"""
import argparse
import re
import subprocess
import sys
import time


def loaded(cmd):
    out = subprocess.run(cmd + ["status"], capture_output=True, text=True)
    m = re.search(r"loaded in the kernel: (\d+)", out.stdout)
    if not m:
        raise SystemExit(f"status failed: {out.stdout}{out.stderr}")
    return int(m.group(1))


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--helper", required=True)
    ap.add_argument("--sudo", action="store_true")
    a = ap.parse_args()
    base = (["sudo", "-n"] if a.sudo else []) + [a.helper]
    target = subprocess.Popen(["sleep", "60"])
    try:
        before = loaded(base)
        helper = subprocess.Popen(base + ["trace", "--pid", str(target.pid), "--crash-after", "3s"],
                                  stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        time.sleep(1.5)
        during = loaded(base)
        helper.wait(timeout=20)
        time.sleep(1.0)
        after = loaded(base)
    finally:
        target.terminate()
    print(f"programs before={before} during={during} after SIGKILL={after}")
    ok = before == 0 and during > 0 and after == 0
    print("PASS" if ok else "FAIL")
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
