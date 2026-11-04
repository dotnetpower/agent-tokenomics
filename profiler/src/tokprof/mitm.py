"""Capture LLM calls with mitmproxy, mainly for Windows and macOS where there is no eBPF.

mitmproxy is a separate tool (install it outside your project, for example ``uv tool install mitmproxy``).
Its ``--mode local`` redirects the traffic of selected processes at the operating-system level, so the
agent needs no change and no restart of its configuration; the TLS interception requires trusting the
mitmproxy CA, which tokprof never installs for you (see ``tokprof mitm --trust-help``).
"""
import os
import secrets
import shutil
import subprocess
import sys
from typing import Optional

from .gateway import Gateway, GatewayThread
from .store import JsonlWriter

ADDON = os.path.join(os.path.dirname(os.path.abspath(__file__)), "boot", "mitm_addon.py")

TRUST_HELP = """\
mitmproxy intercepts TLS with its own certificate authority, so the processes you watch must trust it.
tokprof does not install anything into your trust stores. Choose the narrowest option that works:

  Per process (preferred; nothing system-wide):
    Python (requests/httpx/aiohttp):  REQUESTS_CA_BUNDLE / SSL_CERT_FILE = ~/.mitmproxy/mitmproxy-ca-cert.pem
    Node.js:                          NODE_EXTRA_CA_CERTS = ~/.mitmproxy/mitmproxy-ca-cert.pem
    curl:                             --cacert ~/.mitmproxy/mitmproxy-ca-cert.pem
  The certificate file is created the first time mitmproxy runs.

  System-wide (only if a process ignores the variables above; remember to remove it afterwards):
    macOS:    sudo security add-trusted-cert -d -r trustRoot -k /Library/Keychains/System.keychain ~/.mitmproxy/mitmproxy-ca-cert.pem
    Windows:  certutil -addstore -f Root %USERPROFILE%\\.mitmproxy\\mitmproxy-ca-cert.cer   (administrator prompt)

Local capture mode needs administrator rights on Windows and a one-time approval of the mitmproxy system
extension on macOS; see https://docs.mitmproxy.org/stable/concepts/modes/#local-capture
"""


def find_mitmdump(explicit: Optional[str] = None) -> Optional[str]:
    for c in (explicit, os.environ.get("TOKPROF_MITMDUMP"), shutil.which("mitmdump")):
        if c and os.path.isfile(c):
            return c
    return None


def build_command(mitmdump: str, mode: str, port: int, token: str, extra: Optional[list] = None) -> list:
    cmd = [mitmdump, "--mode", mode, "-s", ADDON, "--set", f"tokprof_port={port}", "--set", f"tokprof_token={token}",
           "--set", "termlog_verbosity=warn", "--set", "flow_detail=0"]
    return cmd + list(extra or [])


def run_mitm(
    out_path: str,
    *,
    local: str = "",
    mode: str = "",
    mitmdump: Optional[str] = None,
    components: Optional[dict] = None,
    use_tiktoken: bool = True,
    extra: Optional[list] = None,
    duration: Optional[float] = None,
    quiet: bool = False,
) -> int:
    if bool(local) == bool(mode):
        print("tokprof: choose exactly one of --local PROCESSES (OS-level capture) or --mode MODE (for example regular@8080)", file=sys.stderr)
        return 2
    exe = find_mitmdump(mitmdump)
    if not exe:
        print("tokprof: mitmdump not found. Install mitmproxy outside your project, for example `uv tool install mitmproxy` "
              "or `pipx install mitmproxy`, or pass --mitmdump PATH.", file=sys.stderr)
        return 2
    writer = JsonlWriter(out_path)
    token = secrets.token_hex(16)
    gw = Gateway([], writer.write, salt=secrets.token_bytes(16), components=components, run_id=secrets.token_hex(4),
                 use_tiktoken=use_tiktoken, ingest_token=token)
    thread = GatewayThread(gw)
    port = thread.start()
    cmd = build_command(exe, f"local:{local}" if local else mode, port, token, extra)
    if not quiet:
        print(f"tokprof: profiling to {out_path} via mitmproxy ({'local:' + local if local else mode}); press Ctrl+C to stop."
              " Processes must trust the mitmproxy CA: tokprof mitm --trust-help", file=sys.stderr)
    code = 1
    try:
        proc = subprocess.Popen(cmd)
    except OSError as exc:
        print(f"tokprof: cannot start mitmdump: {exc}", file=sys.stderr)
        thread.stop()
        writer.close()
        return 127
    try:
        while True:
            try:
                code = proc.wait(timeout=duration)
                break
            except subprocess.TimeoutExpired:
                proc.terminate()
                try:
                    code = proc.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    proc.kill()
                    code = 1
                break
            except KeyboardInterrupt:
                proc.terminate()
                try:
                    code = proc.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    proc.kill()
                    code = 1
                break
    finally:
        import time

        time.sleep(0.3)  # let in-flight ingests finish
        thread.stop()
        writer.close()
    if not quiet:
        s = gw.stats
        print(f"tokprof: {s['recorded']} LLM call(s) recorded, {s['record_errors']} recording error(s)", file=sys.stderr)
        summary = gw.detector.render_summary()
        if summary:
            print(summary, file=sys.stderr)
    return 0 if duration or code in (0, -2, -15, 130) else code
