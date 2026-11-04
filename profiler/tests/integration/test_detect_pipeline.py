import os
import sys

import pytest

from tokprof._mock_llm import MockProcess
from tokprof.runner import run_profiled
from tokprof.store import read_jsonl

pytest.importorskip("requests")

CLIENT = r"""
import json, os, requests
base = os.environ["MOCK_URL"]
chat = {"model": "qwen", "messages": [{"role": "system", "content": "# Role\nBe brief.\n" + "rule. " * 60}, {"role": "user", "content": "hello"}]}
def post(path, body, **kw):
    r = requests.post(base + path, json=body, **kw)
    assert r.status_code == 200, (path, r.status_code, r.text[:100])
post("/api/ai/ask", chat)                                                     # custom gateway path
post("/v1/completions", {"model": "m", "prompt": "Write a haiku about rain.", "max_tokens": 20})
post("/api/generate", {"model": "llama3", "prompt": "Why is the sky blue?"})  # Ollama NDJSON
post("/api/chat", chat)                                                       # Ollama chat NDJSON
post("/v1beta/models/gemini-pro:generateContent", {"contents": [{"role": "user", "parts": [{"text": "hi there"}]}]})
post("/thing/run", {"prompt": "ambiguous but really an LLM"})                 # decided by the response
post("/thing/imagine", {"prompt": "a cat"})                                   # same shape, not an LLM
post("/v1/embeddings", {"model": "e", "input": "embed me please"})            # token-consuming but not generative
post("/generate", {"inputs": "TGI style prompt", "parameters": {"max_new_tokens": 5}})
"""


@pytest.fixture()
def mock():
    m = MockProcess()
    yield m
    m.stop()


def run(script, out, mock, **kw):
    os.environ["MOCK_URL"] = mock.url
    try:
        return run_profiled([sys.executable, "-c", script], str(out), quiet=True, use_tiktoken=False, **kw)
    finally:
        os.environ.pop("MOCK_URL", None)


def test_hooks_detect_self_hosted_endpoints_on_any_path(tmp_path, mock):
    out = tmp_path / "run.jsonl"
    assert run(CLIENT, out, mock, gateway_env=False, hooks=True) == 0
    recs = list(read_jsonl(str(out)))
    seen = sorted((r["endpoint"], r["provider"]) for r in recs)
    assert seen == sorted([
        ("/api/ai/ask", "openai_chat"),
        ("/v1/completions", "completions"),
        ("/api/generate", "completions"),
        ("/api/chat", "openai_chat"),
        ("/v1beta/models/gemini-pro:generateContent", "gemini"),
        ("/thing/run", "completions"),
        ("/generate", "completions"),
    ]), seen
    by_path = {r["endpoint"]: r for r in recs}
    assert by_path["/api/generate"]["usage"]["input_total"] > 0  # Ollama counters were understood
    assert by_path["/v1beta/models/gemini-pro:generateContent"]["model"] == "gemini-pro"  # model came from the URL
    assert by_path["/generate"]["usage"]["input_total"] > 0  # TGI prefill length
    assert {s["kind"] for s in by_path["/v1/completions"]["segments"]} == {"raw_prompt"}
    assert all(r["host_class"] == "loopback" and r["host_id"] and r["host"] is None for r in recs)  # no host name is stored by default
    assert all(r["detect"]["evidence"] for r in recs)
    assert by_path["/thing/run"]["detect"]["evidence"][-1].startswith("response has")


def test_gateway_route_for_a_private_base_url_detects_any_path(tmp_path, mock):
    out = tmp_path / "gw.jsonl"
    script = (
        "import os, json, urllib.request\n"
        "base = os.environ['VLLM_BASE_URL']\n"
        "body = {'model': 'qwen', 'messages': [{'role': 'user', 'content': 'hello'}]}\n"
        "req = urllib.request.Request(base + '/api/ai/ask', data=json.dumps(body).encode(), headers={'Content-Type': 'application/json'})\n"
        "print(len(urllib.request.urlopen(req).read()))\n"
        "req = urllib.request.Request(base + '/thing/run', data=json.dumps({'prompt': 'hi there friend'}).encode(), headers={'Content-Type': 'application/json'})\n"
        "urllib.request.urlopen(req).read()\n"
        "req = urllib.request.Request(base + '/thing/imagine', data=json.dumps({'prompt': 'cat'}).encode(), headers={'Content-Type': 'application/json'})\n"
        "urllib.request.urlopen(req).read()\n"
    )
    code = run_profiled([sys.executable, "-c", script], str(out), quiet=True, use_tiktoken=False, extra_maps=[f"VLLM_BASE_URL={mock.url}"], hooks=False)
    assert code == 0
    recs = list(read_jsonl(str(out)))
    assert sorted(r["endpoint"].split("/", 2)[-1] for r in recs) == ["api/ai/ask", "thing/run"], [r["endpoint"] for r in recs]
