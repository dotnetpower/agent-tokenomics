import json
import os
import shutil
import sys

import pytest

from tokprof._mock_llm import MockProcess
from tokprof.runner import run_profiled
from tokprof.store import read_jsonl

httpx = pytest.importorskip("httpx")

PY_CLIENT = r"""
import asyncio, json, os, httpx
base = os.environ["MOCK_URL"]            # hard-coded upstream: bypasses *_BASE_URL on purpose
sys_msg = "# Role\nYou are helpful.\n" + ("rule. " * 100)
msgs = [{"role": "system", "content": sys_msg}, {"role": "user", "content": "hello"}]
with httpx.Client() as c:
    c.post(base + "/v1/chat/completions", json={"model": "gpt-4o", "messages": msgs})
    with c.stream("POST", base + "/v1/chat/completions", json={"model": "gpt-4o", "messages": msgs, "stream": True}) as r:
        for _ in r.iter_bytes():
            pass
    c.post(base + "/v1/models/ignored", json={"x": 1}) if False else None
async def main():
    async with httpx.AsyncClient() as c:
        await c.post(base + "/v1/messages", json={"model": "claude-x", "max_tokens": 8, "system": sys_msg, "messages": [{"role": "user", "content": "hi"}]})
asyncio.run(main())
"""

NODE_CLIENT = r"""
const base = process.env.MOCK_URL;
(async () => {
  await fetch(base + "/v1/chat/completions", {method: "POST", headers: {"Content-Type": "application/json"},
    body: JSON.stringify({model: "gpt-4o", messages: [{role: "system", content: "terse"}, {role: "user", content: "node hello"}]})});
  const r = await fetch(base + "/v1/chat/completions", {method: "POST", headers: {"Content-Type": "application/json"},
    body: JSON.stringify({model: "gpt-4o", stream: true, messages: [{role: "user", content: "node stream"}]})});
  await r.text();
})().then(() => setTimeout(() => process.exit(0), 300)).catch((e) => { console.error(e); process.exit(1); });
"""


@pytest.fixture()
def mock():
    m = MockProcess()
    yield m
    m.stop()


def run(cmd, out, mock, **kw):
    os.environ["MOCK_URL"] = mock.url
    try:
        return run_profiled(cmd, str(out), quiet=True, use_tiktoken=False, **kw)
    finally:
        os.environ.pop("MOCK_URL", None)


def test_python_hook_captures_hardcoded_base_url(tmp_path, mock):
    out = tmp_path / "run.jsonl"
    code = run([sys.executable, "-c", PY_CLIENT], out, mock, gateway_env=False, hooks=True)
    assert code == 0
    recs = list(read_jsonl(str(out)))
    assert sorted(r["provider"] for r in recs) == ["anthropic", "openai_chat", "openai_chat"]
    assert [r["stream"] for r in recs if r["provider"] == "openai_chat"].count(True) == 1
    assert all(r["usage"].get("input_total") for r in recs)
    assert "system" in {s["kind"] for r in recs for s in r["segments"]}


def test_hooks_disabled_records_nothing_without_gateway_env(tmp_path, mock):
    out = tmp_path / "none.jsonl"
    code = run([sys.executable, "-c", PY_CLIENT], out, mock, gateway_env=False, hooks=False)
    assert code == 0
    assert not out.exists() or list(read_jsonl(str(out))) == []


def test_no_double_recording_when_gateway_and_hooks_are_both_on(tmp_path, mock):
    out = tmp_path / "both.jsonl"
    script = (
        "import os, httpx\n"
        "base = os.environ['OPENAI_BASE_URL']\n"
        "httpx.post(base + '/chat/completions', json={'model': 'gpt-4o', 'messages': [{'role': 'user', 'content': 'hi'}]})\n"
    )
    os.environ["OPENAI_BASE_URL"] = mock.url + "/v1"
    try:
        code = run_profiled([sys.executable, "-c", script], str(out), quiet=True, use_tiktoken=False)
    finally:
        os.environ.pop("OPENAI_BASE_URL", None)
    assert code == 0
    assert len(list(read_jsonl(str(out)))) == 1  # the hook skips requests that already go through the gateway


@pytest.mark.skipif(shutil.which("node") is None, reason="node not installed")
def test_node_hook_captures_fetch(tmp_path, mock):
    out = tmp_path / "node.jsonl"
    code = run(["node", "-e", NODE_CLIENT], out, mock, gateway_env=False, hooks=True)
    assert code == 0
    recs = list(read_jsonl(str(out)))
    assert len(recs) == 2 and sorted(r["stream"] for r in recs) == [False, True]
    assert all(r["usage"].get("input_total") for r in recs)


def test_hook_never_breaks_the_app_when_gateway_is_unreachable(tmp_path, mock):
    # Point the hook at a dead port: the application must still succeed.
    env = dict(os.environ, TOKPROF_PORT="1", TOKPROF_TOKEN="x", MOCK_URL=mock.url)
    from tokprof.runner import BOOT_DIR
    import subprocess

    env["PYTHONPATH"] = BOOT_DIR
    r = subprocess.run([sys.executable, "-c", PY_CLIENT], env=env, capture_output=True, text=True, timeout=60)
    assert r.returncode == 0, r.stderr
