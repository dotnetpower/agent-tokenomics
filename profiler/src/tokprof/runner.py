"""Run a command with its LLM base URLs pointed at a local gateway (no code changes, no system changes)."""

from __future__ import annotations

import os
import secrets
import signal
import subprocess
import sys
import time
from typing import Optional
from urllib.parse import urlsplit

from .gateway import Gateway, GatewayThread, Route
from .store import JsonlWriter

DEFAULT_OPENAI = "https://api.openai.com/v1"
DEFAULT_ANTHROPIC = "https://api.anthropic.com"


def build_routes(env: dict, extra_maps: Optional[list] = None):
    """Return (routes, env_overrides_template, azure_hosts). Templates contain {port}."""
    routes: list = []
    overrides: dict = {}

    oa = env.get("OPENAI_BASE_URL") or DEFAULT_OPENAI
    oa_path = urlsplit(oa).path.rstrip("/")
    routes.append(Route.from_urls("/openai" + oa_path, oa))
    overrides["OPENAI_BASE_URL"] = "http://127.0.0.1:{port}/openai" + oa_path

    an = env.get("ANTHROPIC_BASE_URL") or DEFAULT_ANTHROPIC
    an_path = urlsplit(an).path.rstrip("/")
    routes.append(Route.from_urls("/anthropic" + an_path, an))
    overrides["ANTHROPIC_BASE_URL"] = "http://127.0.0.1:{port}/anthropic" + an_path

    azure_hosts: set = set()
    az = env.get("AZURE_OPENAI_ENDPOINT")
    if az:
        host = urlsplit(az).hostname or ""
        if host:
            azure_hosts.add(host)
            overrides["AZURE_OPENAI_ENDPOINT"] = "http://127.0.0.1:{port}/azure/" + host

    for i, spec in enumerate(extra_maps or []):
        var, _, upstream = spec.partition("=")
        if not var or not upstream:
            raise ValueError(f"--map expects ENV_VAR=UPSTREAM_URL, got {spec!r}")
        path = urlsplit(upstream).path.rstrip("/")
        prefix = f"/map{i}" + path
        routes.append(Route.from_urls(prefix, upstream))
        overrides[var] = "http://127.0.0.1:{port}" + prefix
    return routes, overrides, azure_hosts


def run_profiled(
    cmd: list,
    out_path: str,
    *,
    components: Optional[dict] = None,
    extra_maps: Optional[list] = None,
    use_tiktoken: bool = True,
    quiet: bool = False,
) -> int:
    env = dict(os.environ)
    routes, overrides, azure_hosts = build_routes(env, extra_maps)
    writer = JsonlWriter(out_path)
    run_id = secrets.token_hex(4)
    gw = Gateway(
        routes,
        writer.write,
        salt=secrets.token_bytes(16),
        components=components,
        run_id=run_id,
        azure_hosts=azure_hosts,
        use_tiktoken=use_tiktoken,
    )
    thread = GatewayThread(gw)
    port = thread.start()
    child_env = dict(env)
    for k, v in overrides.items():
        child_env[k] = v.format(port=port)
    child_env["TOKPROF_ACTIVE"] = "1"

    if not quiet:
        print(f"tokprof: profiling to {out_path} (gateway 127.0.0.1:{port}); only LLM base URLs are redirected", file=sys.stderr)
    code = 1
    proc = None
    try:
        proc = subprocess.Popen(cmd, env=child_env)
        while True:
            try:
                code = proc.wait()
                break
            except KeyboardInterrupt:
                # Forward Ctrl+C to the child and keep waiting so in-flight calls are recorded.
                try:
                    proc.send_signal(signal.SIGINT)
                except Exception:
                    pass
    except FileNotFoundError:
        print(f"tokprof: command not found: {cmd[0]}", file=sys.stderr)
        code = 127
    finally:
        # Let in-flight responses finish before stopping the gateway.
        time.sleep(0.2)
        thread.stop()
        writer.close()
    if not quiet:
        s = gw.stats
        print(f"tokprof: {s['recorded']} LLM call(s) recorded, {s['record_errors']} recording error(s)", file=sys.stderr)
    return code


def default_shell() -> list:
    if os.name == "nt":
        return [os.environ.get("COMSPEC", "cmd.exe")]
    return [os.environ.get("SHELL", "/bin/sh")]
