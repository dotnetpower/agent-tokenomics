import json
import os
import shutil
import socket
import subprocess
import sys
import time
import urllib.request

import pytest

from tokprof._mock_llm import MockProcess
from tokprof.mitm import find_mitmdump
from tokprof.store import read_jsonl

pytestmark = pytest.mark.skipif(find_mitmdump() is None, reason="mitmdump not installed (set TOKPROF_MITMDUMP)")


def free_port():
    s = socket.socket()
    s.bind(("127.0.0.1", 0))
    p = s.getsockname()[1]
    s.close()
    return p


def post(proxy, url, body):
    opener = urllib.request.build_opener(urllib.request.ProxyHandler({"http": proxy}))
    req = urllib.request.Request(url, data=json.dumps(body).encode(), headers={"Content-Type": "application/json"})
    with opener.open(req, timeout=20) as r:
        return r.read()


def test_mitm_regular_mode_records_streaming_and_plain_calls(tmp_path):
    mock = MockProcess()
    port = free_port()
    out = tmp_path / "mitm.jsonl"
    proc = subprocess.Popen(
        [sys.executable, "-m", "tokprof", "mitm", "--mode", f"regular@{port}", "--out", str(out), "--no-tiktoken"],
        stderr=subprocess.PIPE, text=True,
    )
    try:
        for _ in range(100):  # wait until the proxy accepts connections
            try:
                socket.create_connection(("127.0.0.1", port), timeout=0.2).close()
                break
            except OSError:
                time.sleep(0.2)
        else:
            pytest.fail("mitmdump did not start")
        msgs = [{"role": "system", "content": "# Role\nBe brief.\n" + "rule. " * 60}, {"role": "user", "content": "hello"}]
        body = post(f"http://127.0.0.1:{port}", mock.url + "/v1/chat/completions", {"model": "gpt-4o", "messages": msgs})
        assert json.loads(body)["usage"]["prompt_tokens"] > 0  # the application still gets the real answer
        sse = post(f"http://127.0.0.1:{port}", mock.url + "/v1/chat/completions", {"model": "gpt-4o", "messages": msgs, "stream": True})
        assert sse.rstrip().endswith(b"[DONE]")
        time.sleep(1.0)
    finally:
        proc.send_signal(2 if os.name != "nt" else 15)
        try:
            proc.wait(timeout=20)
        except subprocess.TimeoutExpired:
            proc.kill()
        mock.stop()
    recs = list(read_jsonl(str(out)))
    assert len(recs) == 2 and sorted(r["stream"] for r in recs) == [False, True]
    assert all(r["usage"].get("input_total") for r in recs)
    assert "system" in {s["kind"] for r in recs for s in r["segments"]}


def test_trust_help_and_missing_selector(capsys):
    from tokprof.cli import main

    assert main(["mitm", "--trust-help"]) == 0
    assert "NODE_EXTRA_CA_CERTS" in capsys.readouterr().out
    assert main(["mitm", "--out", "x.jsonl"]) == 2
