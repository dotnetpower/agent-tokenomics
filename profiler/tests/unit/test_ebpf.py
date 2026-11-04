import json
import os
import secrets
import sys

import pytest

from tokprof import ebpf
from tokprof.gateway import Gateway


def _gateway(records):
    return Gateway([], records.append, salt=secrets.token_bytes(16), run_id="t", use_tiktoken=False)


def _msg(**over):
    body = {"model": "m", "messages": [{"role": "system", "content": "be brief"}, {"role": "user", "content": "hello there"}]}
    msg = {
        "path": "/v1/chat/completions",
        "request": json.dumps(body),
        "response": json.dumps({"usage": {"prompt_tokens": 20, "completion_tokens": 4, "total_tokens": 24}}),
        "sse": False,
        "status": 200,
        "ts": 1000.0,
        "ttfb_ms": 12.5,
        "latency_ms": 80,
    }
    msg.update(over)
    return msg


def test_ingest_message_records_usage_and_latency():
    out = []
    _gateway(out).ingest_message(_msg())
    assert len(out) == 1
    rec = out[0]
    assert rec["usage"]["input_total"] == 20
    assert rec["latency_ms"] == 80  # taken from the capture, not from now - ts
    assert rec["ttfb_ms"] == 12.5


def test_ingest_message_sse_response():
    sse = 'data: {"choices":[],"usage":{"prompt_tokens":20,"completion_tokens":3,"total_tokens":23}}\n\ndata: [DONE]\n\n'
    out = []
    _gateway(out).ingest_message(_msg(response=sse, sse=True))
    assert out[0]["usage"]["input_total"] == 20 and out[0]["usage"]["output"] == 3


def test_ingest_message_ignores_non_llm_and_never_raises():
    out = []
    gw = _gateway(out)
    gw.ingest_message(_msg(path="/healthz", request=json.dumps({"ok": True})))
    gw.ingest_message({"request": "not json"})
    assert out == []
    assert gw.stats["record_errors"] >= 0


def test_ingest_lines_skips_garbage():
    out = []
    gw = _gateway(out)
    lines = [json.dumps(_msg()) + "\n", "\n", "not json\n", json.dumps(_msg(ts=1001.0)) + "\n"]
    assert ebpf._ingest_lines(lines, gw) == 2
    assert len(out) == 2


def test_helper_command_building(monkeypatch):
    monkeypatch.setattr(ebpf.shutil, "which", lambda name: "/usr/bin/sudo" if name == "sudo" else None)
    monkeypatch.setattr(ebpf.os, "geteuid", lambda: 1000, raising=False)
    monkeypatch.setattr(ebpf.sys.stderr, "isatty", lambda: False, raising=False)
    cmd = ebpf.helper_command("/x/tokprof-ebpf", "12,13", "node", False, "5m", True)
    assert cmd[:3] == ["sudo", "-n", "/x/tokprof-ebpf"]
    assert cmd[3:5] == ["trace", "--exit-on-stdin-eof"]
    assert ["--pid", "12,13"] == cmd[cmd.index("--pid"):cmd.index("--pid") + 2]
    assert "--comm" in cmd and "--duration" in cmd and "--all" not in cmd
    assert ebpf.helper_command("/x/h", "", "", True, None, False) == ["/x/h", "trace", "--exit-on-stdin-eof", "--all"]
    assert "--no-plain" in ebpf.helper_command("/x/h", "1", "", False, None, False, plain=False)
    monkeypatch.setattr(ebpf.os, "geteuid", lambda: 0, raising=False)
    assert ebpf.helper_command("/x/h", "1", "", False, None, True)[0] == "/x/h"


def test_find_helper_explicit_path(tmp_path, monkeypatch):
    exe = tmp_path / "tokprof-ebpf"
    exe.write_text("#!/bin/sh\n")
    exe.chmod(0o755)
    assert ebpf.find_helper(str(exe)) == str(exe)
    monkeypatch.setenv("TOKPROF_EBPF_HELPER", str(exe))
    assert ebpf.find_helper() == str(exe)
    assert ebpf.find_helper(str(tmp_path / "missing")) == str(exe)  # falls through to the env var


@pytest.mark.skipif(sys.platform == "win32", reason="POSIX shell stub")
def test_run_ebpf_with_stub_helper_records_calls(tmp_path):
    line = json.dumps(_msg())
    stub = tmp_path / "tokprof-ebpf"
    stub.write_text(f"#!/bin/sh\necho '{line}'\n")  # prints one exchange and exits
    stub.chmod(0o755)
    out = tmp_path / "run.jsonl"
    if not sys.platform.startswith("linux"):
        assert ebpf.run_ebpf(str(out), pids="1", helper=str(stub), use_sudo=False, quiet=True) == 2
        return
    code = ebpf.run_ebpf(str(out), pids="1", helper=str(stub), use_sudo=False, quiet=True)
    assert code == 0
    rows = [json.loads(x) for x in out.read_text().splitlines()]
    assert len(rows) == 1 and rows[0]["usage"]["input_total"] == 20
    assert oct(os.stat(out).st_mode & 0o777) == "0o600"


def test_run_ebpf_requires_a_selector(tmp_path):
    assert ebpf.run_ebpf(str(tmp_path / "x.jsonl"), quiet=True) == 2
