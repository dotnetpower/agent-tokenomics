"""A long-running profiling session: `tokprof start` / `env` / `status` / `stop`.

The session is a background gateway. `tokprof env` prints the environment that routes *new* processes
through it; processes that are already running are not affected. Nothing outside the child environments
you export is changed, and `tokprof env --unset` prints how to restore the previous values.
"""

from __future__ import annotations

import json
import os
import secrets
import signal
import subprocess
import sys
import threading
import time
import urllib.request
from typing import Optional

from .gateway import Gateway, GatewayThread
from .runner import build_routes, hook_env
from .store import JsonlWriter

OVERRIDDEN = ("OPENAI_BASE_URL", "ANTHROPIC_BASE_URL", "AZURE_OPENAI_ENDPOINT", "PYTHONPATH", "NODE_OPTIONS", "TOKPROF_PORT", "TOKPROF_TOKEN")


def state_path(override: Optional[str] = None) -> str:
    if override:
        return override
    base = os.environ.get("TOKPROF_HOME") or os.path.join(os.path.expanduser("~"), ".tokprof")
    return os.path.join(base, "session.json")


def _read_state(path: str) -> Optional[dict]:
    try:
        with open(path, "r", encoding="utf-8") as fh:
            return json.load(fh)
    except (OSError, ValueError):
        return None


def _alive(pid: int) -> bool:
    if os.name == "nt":
        try:
            import ctypes

            kernel32 = ctypes.windll.kernel32
            handle = kernel32.OpenProcess(0x1000, False, int(pid))  # PROCESS_QUERY_LIMITED_INFORMATION
            if not handle:
                return False
            try:
                code = ctypes.c_ulong()
                if not kernel32.GetExitCodeProcess(handle, ctypes.byref(code)):
                    return False
                return code.value == 259  # STILL_ACTIVE
            finally:
                kernel32.CloseHandle(handle)
        except Exception:
            return False
    try:
        os.kill(pid, 0)
        return True
    except OSError:
        return False


def _health(port: int) -> Optional[dict]:
    try:
        with urllib.request.urlopen(f"http://127.0.0.1:{port}/__tokprof/health", timeout=2) as r:
            return json.loads(r.read())
    except Exception:
        return None


def serve(
    out: str,
    state_file: str,
    extra_maps: Optional[list] = None,
    use_tiktoken: bool = True,
    *,
    use_ebpf: bool = False,
    pids: str = "",
    comms: str = "",
) -> int:
    """Run the gateway (and optional Linux eBPF tap) in the foreground until SIGTERM/SIGINT, then drain and exit."""
    env = dict(os.environ)
    routes, overrides, azure_hosts = build_routes(env, extra_maps)
    writer = JsonlWriter(out)
    token = secrets.token_hex(16)
    stop = threading.Event()
    gw = Gateway(routes, writer.write, salt=secrets.token_bytes(16), run_id=secrets.token_hex(4),
                 azure_hosts=azure_hosts, use_tiktoken=use_tiktoken, ingest_token=token, on_shutdown=stop.set)
    thread = GatewayThread(gw)
    port = thread.start()

    ebpf_proc = None
    ebpf_thread = None
    ebpf_scope = None
    if use_ebpf:
        from .ebpf import _ingest_lines, ebpf_can_run, helper_command

        exe = ebpf_can_run(interactive=False)
        if exe:
            everything = not (pids or comms)
            cmd = helper_command(exe, pids, comms, everything, duration=None, use_sudo=True, ignore_pid=os.getpid())
            try:
                ebpf_proc = subprocess.Popen(
                    cmd, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                    text=True, encoding="utf-8", errors="replace",
                )
                ready_evt = threading.Event()

                def _drain_err(stream) -> None:
                    for line in stream:
                        if line.strip().endswith("ready"):
                            ready_evt.set()

                threading.Thread(target=_drain_err, args=(ebpf_proc.stderr,), name="tokprof-ebpf-stderr", daemon=True).start()
                if ready_evt.wait(timeout=10.0) and ebpf_proc.poll() is None:
                    ebpf_thread = threading.Thread(target=_ingest_lines, args=(ebpf_proc.stdout, gw), name="tokprof-ebpf-ingest", daemon=True)
                    ebpf_thread.start()
                    ebpf_scope = f"pid={pids}" if pids else (f"comm={comms}" if comms else "all processes & Docker containers")
                else:
                    try:
                        ebpf_proc.kill()
                    except Exception:
                        pass
                    ebpf_proc = None
            except OSError:
                ebpf_proc = None

    originals = {k: env.get(k) for k in OVERRIDDEN}
    child = {k: v.format(port=port) for k, v in overrides.items()}
    child.update(hook_env(env, port, token))
    os.makedirs(os.path.dirname(os.path.abspath(state_file)), exist_ok=True)
    tmp = state_file + ".tmp"
    fd = os.open(tmp, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
    with os.fdopen(fd, "w", encoding="utf-8") as fh:
        json.dump({"pid": os.getpid(), "port": port, "out": os.path.abspath(out), "started": time.time(),
                   "token": token, "ebpf": ebpf_scope, "env": child, "originals": originals}, fh)
    os.replace(tmp, state_file)

    for sig in (signal.SIGINT, signal.SIGTERM):
        signal.signal(sig, lambda *_: stop.set())
    try:
        while not stop.wait(0.5):
            pass
    finally:
        if ebpf_proc is not None:
            try:
                ebpf_proc.stdin.close()
            except OSError:
                pass
            if ebpf_thread is not None:
                ebpf_thread.join(timeout=2.0)
            if ebpf_proc.poll() is None:
                try:
                    ebpf_proc.terminate()
                    ebpf_proc.wait(timeout=1.0)
                except Exception:
                    try:
                        ebpf_proc.kill()
                    except Exception:
                        pass
        thread.stop()
        writer.close()
        try:
            os.remove(state_file)
        except OSError:
            pass
    return 0


def start(
    out: str,
    state_file: str,
    extra_maps: Optional[list] = None,
    use_tiktoken: bool = True,
    *,
    use_ebpf: Optional[bool] = None,
    pids: str = "",
    comms: str = "",
) -> int:
    existing = _read_state(state_file)
    if existing and _alive(existing["pid"]):
        if _health(existing.get("port", 0)):
            print(f"tokprof: a session is already running (pid {existing['pid']}, port {existing['port']}); use `tokprof status`", file=sys.stderr)
            return 1
        try:
            os.remove(state_file)
        except OSError:
            pass
    if use_ebpf is None:
        use_ebpf = sys.platform.startswith("linux")
    if use_ebpf:
        from .ebpf import ebpf_can_run

        use_ebpf = ebpf_can_run(interactive=True) is not None
    cmd = [sys.executable, "-m", "tokprof", "serve", "--out", out, "--state", state_file]
    for m in extra_maps or []:
        cmd += ["--map", m]
    if not use_tiktoken:
        cmd.append("--no-tiktoken")
    if use_ebpf:
        cmd.append("--ebpf")
    else:
        cmd.append("--no-ebpf")
    if pids:
        cmd += ["--pid", pids]
    if comms:
        cmd += ["--comm", comms]
    kwargs: dict = {"stdin": subprocess.DEVNULL, "stdout": subprocess.DEVNULL, "stderr": subprocess.DEVNULL}
    if os.name == "nt":
        kwargs["creationflags"] = 0x00000008 | 0x00000200  # DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP
    else:
        kwargs["start_new_session"] = True
    subprocess.Popen(cmd, **kwargs)
    deadline = time.time() + 15
    sleep_s = 0.01
    while time.time() < deadline:
        st = _read_state(state_file)
        if st and _health(st["port"]):
            ebpf_info = f", eBPF: {st['ebpf']}" if st.get("ebpf") else ""
            print(f"tokprof: session started (pid {st['pid']}, gateway 127.0.0.1:{st['port']}{ebpf_info})")
            print(f"  recording to: {st['out']}")
            if st.get("ebpf"):
                print("  Running host processes, background services, and Docker containers are captured automatically.")
                print("  Check progress: tokprof status   |   Live view: tokprof report --watch   |   Finish & report: tokprof stop")
            else:
                print('  Route NEW processes through it in this shell with:  eval "$(tokprof env)"')
                print("  Stop and view report with: tokprof stop")
            return 0
        time.sleep(sleep_s)
        sleep_s = min(0.08, sleep_s * 1.5)
    print("tokprof: the session did not start in time; run `tokprof serve` in the foreground to see errors", file=sys.stderr)
    return 1


def status(state_file: str) -> int:
    st = _read_state(state_file)
    if not st or not _alive(st["pid"]):
        print("tokprof: no active session")
        return 1
    h = _health(st["port"]) or {}
    ebpf_info = f", eBPF: {st['ebpf']}" if st.get("ebpf") else ""
    print(f"tokprof: active session pid {st['pid']}, gateway 127.0.0.1:{st['port']}{ebpf_info}, recording to {st['out']}")
    print(f"  requests seen: {h.get('requests', '?')}, calls recorded: {h.get('recorded', '?')}, record errors: {h.get('record_errors', '?')}")
    return 0


def _quote(shell: str, value: str) -> str:
    if shell == "powershell":
        return "'" + value.replace("'", "''") + "'"
    if shell == "cmd":
        return value
    return "'" + value.replace("'", "'\"'\"'") + "'"


def render_env(st: dict, shell: str, unset: bool = False) -> str:
    lines = []
    items = st["originals"].items() if unset else st["env"].items()
    for k, v in items:
        if unset:
            if v is None:
                lines.append({"powershell": f"Remove-Item Env:{k} -ErrorAction SilentlyContinue", "cmd": f"set {k}=", "fish": f"set -e {k}"}.get(shell, f"unset {k}"))
                continue
        if shell == "powershell":
            lines.append(f"$env:{k} = {_quote(shell, v)}")
        elif shell == "cmd":
            lines.append(f"set {k}={v}")
        elif shell == "fish":
            lines.append(f"set -gx {k} {_quote(shell, v)}")
        else:
            lines.append(f"export {k}={_quote(shell, v)}")
    return "\n".join(lines)


def detect_shell() -> str:
    if os.name == "nt":
        return "powershell" if os.environ.get("PSModulePath") and not os.environ.get("PROMPT") else "cmd"
    name = os.path.basename(os.environ.get("SHELL", "bash"))
    return "fish" if name == "fish" else "bash"


def env(state_file: str, shell: Optional[str], unset: bool) -> int:
    st = _read_state(state_file)
    if not st or not _alive(st["pid"]):
        print("tokprof: no active session; start one with `tokprof start`", file=sys.stderr)
        return 1
    print(render_env(st, shell or detect_shell(), unset))
    return 0


def stop(state_file: str, force: bool) -> int:
    st = _read_state(state_file)
    if not st or not _alive(st["pid"]):
        print("tokprof: no active session")
        return 1
    h = _health(st["port"]) or {}
    if not force and h.get("last_request_age_s") is not None and h["last_request_age_s"] < 30:
        print(f"tokprof: the gateway handled a request {h['last_request_age_s']:.0f}s ago; processes started with its environment would "
              "fail after it stops. Stop them first, or use --force.", file=sys.stderr)
        return 2
    if st.get("token"):
        try:
            req = urllib.request.Request(
                f"http://127.0.0.1:{st['port']}/__tokprof/shutdown",
                data=b"",
                headers={"X-Tokprof-Token": st["token"]},
                method="POST",
            )
            with urllib.request.urlopen(req, timeout=3) as r:
                r.read()
        except Exception:
            pass
    deadline = time.time() + 15
    killed = False
    sleep_s = 0.01
    while time.time() < deadline and _alive(st["pid"]):
        if not killed and (not st.get("token") or time.time() + 10 >= deadline):
            killed = True
            try:
                os.kill(st["pid"], signal.SIGTERM)
            except OSError:
                break
        time.sleep(sleep_s)
        sleep_s = min(0.08, sleep_s * 1.5)
    try:
        os.remove(state_file)
    except OSError:
        pass
    print(f"tokprof: session stopped. Recorded data: {st['out']}")
    if not st.get("ebpf") or h.get("last_request_age_s") is not None:
        print("Processes that were started with the session environment still point at the stopped gateway; "
              "restart them, or restore this shell with: eval \"$(tokprof env --unset)\" (run it BEFORE stopping next time)")
    return 0
