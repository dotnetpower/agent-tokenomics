"""End-to-end self test: mock LLM server + clients (Python, Node, curl) through the gateway."""

from __future__ import annotations

import os
import shutil
import subprocess
import sys
import tempfile

from ._mock_llm import MockProcess
from .runner import run_profiled
from .store import read_jsonl

PY_CLIENT = r"""
import json, os, urllib.request
def post(url, body, headers=None):
    req = urllib.request.Request(url, data=json.dumps(body).encode(), headers={"Content-Type": "application/json", **(headers or {})})
    with urllib.request.urlopen(req, timeout=20) as r:
        return r.read()
oa = os.environ["OPENAI_BASE_URL"]; an = os.environ["ANTHROPIC_BASE_URL"]
sys_msg = "# Role\nYou are a helpful assistant.\n" + ("rule one. " * 80) + "\n# Style\n" + ("be brief. " * 80)
tools = [{"type": "function", "function": {"name": "get_weather", "description": "Get weather", "parameters": {"type": "object", "properties": {"city": {"type": "string"}}}}}]
msgs = [{"role": "system", "content": sys_msg}, {"role": "user", "content": "hello"}, {"role": "assistant", "content": "hi"}, {"role": "user", "content": "what is the weather? use-tool"}]
post(oa + "/chat/completions", {"model": "gpt-4o", "messages": msgs, "tools": tools})
post(oa + "/chat/completions", {"model": "gpt-4o", "messages": msgs, "tools": tools, "stream": True})
post(oa + "/responses", {"model": "gpt-4o", "instructions": sys_msg, "input": "hello"})
post(an + "/v1/messages", {"model": "claude-x", "max_tokens": 16, "system": sys_msg, "messages": [{"role": "user", "content": "hello"}]})
"""

NODE_CLIENT = r"""
const base = process.env.OPENAI_BASE_URL;
fetch(base + "/chat/completions", {method: "POST", headers: {"Content-Type": "application/json"},
  body: JSON.stringify({model: "gpt-4o", messages: [{role: "system", content: "You are terse."}, {role: "user", content: "ping from node"}]})})
  .then(r => r.text()).then(() => process.exit(0)).catch(e => { console.error(e); process.exit(1); });
"""


def run_selftest() -> int:
    mock = MockProcess()
    tmp = tempfile.mkdtemp(prefix="tokprof-selftest-")
    out = os.path.join(tmp, "selftest.jsonl")
    saved = {k: os.environ.get(k) for k in ("OPENAI_BASE_URL", "ANTHROPIC_BASE_URL", "AZURE_OPENAI_ENDPOINT")}
    os.environ["OPENAI_BASE_URL"] = mock.url + "/v1"
    os.environ["ANTHROPIC_BASE_URL"] = mock.url
    os.environ.pop("AZURE_OPENAI_ENDPOINT", None)
    results = []
    try:
        clients = [("python (urllib)", [sys.executable, "-c", PY_CLIENT], 4)]
        if shutil.which("node"):
            clients.append(("node (fetch)", ["node", "-e", NODE_CLIENT], 1))
        if shutil.which("curl") and os.name != "nt":
            # Body goes through a file so shell quoting cannot alter it; the URL comes from the child's env.
            body_path = os.path.join(tmp, "body.json")
            with open(body_path, "w", encoding="utf-8") as fh:
                fh.write('{"model":"gpt-4o","messages":[{"role":"user","content":"ping from curl"}]}')
            curl = (
                'curl -s -o /dev/null -X POST -H "Content-Type: application/json" '
                f'--data-binary @"{body_path}" "$OPENAI_BASE_URL/chat/completions"'
            )
            clients.append(("curl", ["sh", "-c", curl], 1))
        for name, cmd, expected in clients:
            before = len(list(read_jsonl(out))) if os.path.exists(out) else 0
            code = run_profiled(cmd, out, quiet=True, use_tiktoken=False)
            after = len(list(read_jsonl(out))) if os.path.exists(out) else 0
            got = after - before
            results.append((name, code == 0 and got == expected, f"exit={code}, recorded {got}/{expected} call(s)"))
        recs = list(read_jsonl(out)) if os.path.exists(out) else []
        with_usage = sum(1 for r in recs if r.get("usage", {}).get("input_total"))
        results.append(("usage captured (json + sse)", bool(recs) and with_usage == len(recs), f"{with_usage}/{len(recs)} records"))
        kinds = {s["kind"] for r in recs for s in r["segments"]}
        results.append(("segments decomposed", {"system", "user_prompt", "tool_def"} <= kinds, ", ".join(sorted(kinds))))
        providers = {r["provider"] for r in recs}
        results.append(("providers seen", {"openai_chat", "openai_responses", "anthropic"} <= providers, ", ".join(sorted(providers))))
    finally:
        mock.stop()
        for k, v in saved.items():
            if v is None:
                os.environ.pop(k, None)
            else:
                os.environ[k] = v
        shutil.rmtree(tmp, ignore_errors=True)

    ok = all(r[1] for r in results)
    print("tokprof selftest")
    for name, passed, detail in results:
        print(f"  [{'ok' if passed else 'FAIL'}] {name}: {detail}")
    print("PASS" if ok else "FAIL")
    return 0 if ok else 1
