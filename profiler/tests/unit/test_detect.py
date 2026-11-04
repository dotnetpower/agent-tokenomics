import importlib.util
import json
import os
import shutil
import subprocess
import sys

import pytest

from tokprof import detect
from tokprof.detect import Detector, DetectConfig, classify_host, explain, host_matches, path_hint, response_evidence
from tokprof.parse.request import ANTHROPIC, COMPLETIONS, GEMINI, OPENAI_CHAT, OPENAI_RESPONSES

HERE = os.path.dirname(os.path.abspath(__file__))
VECTORS = os.path.join(HERE, "..", "vectors", "prefilter.json")
BOOT = os.path.join(HERE, "..", "..", "src", "tokprof", "boot")

CHAT = {"model": "m", "messages": [{"role": "system", "content": "be brief"}, {"role": "user", "content": "hi"}]}


def _cases():
    with open(VECTORS, encoding="utf-8") as fh:
        return json.load(fh)["cases"]


def _load(name, filename):
    spec = importlib.util.spec_from_file_location(name, os.path.join(BOOT, filename))
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


# ---- the cheap pre-filter must agree in every implementation -------------------------------------------------------

@pytest.mark.parametrize("case", _cases(), ids=lambda c: c["name"])
def test_prefilter_vectors_core(case):
    assert detect.plausible(case["method"], case["path"], case["body"]) is case["expect"]


@pytest.mark.parametrize("case", _cases(), ids=lambda c: c["name"])
def test_prefilter_vectors_python_hook(case, monkeypatch):
    monkeypatch.delenv("TOKPROF_PORT", raising=False)
    mod = _load("_hook_under_test", "_tokprof_hook.py")
    assert mod._plausible(case["method"], case["path"], case["body"]) is case["expect"]


@pytest.mark.parametrize("case", _cases(), ids=lambda c: c["name"])
def test_prefilter_vectors_mitm_addon(case):
    mod = _load("_addon_under_test", "mitm_addon.py")
    assert mod._plausible(case["method"], case["path"], case["body"]) is case["expect"]


@pytest.mark.skipif(shutil.which("node") is None, reason="node not installed")
def test_prefilter_vectors_node_hook():
    script = (
        "const {plausible} = require(process.argv[1]);"
        "const cases = JSON.parse(require('fs').readFileSync(process.argv[2], 'utf8')).cases;"
        "const bad = cases.filter(c => plausible(c.method, c.path, c.body) !== c.expect).map(c => c.name);"
        "console.log(JSON.stringify(bad));"
    )
    env = {k: v for k, v in os.environ.items() if k not in ("TOKPROF_PORT", "TOKPROF_TOKEN")}
    out = subprocess.run(["node", "-e", script, os.path.join(BOOT, "node_preload.js"), VECTORS], capture_output=True, text=True, env=env, timeout=30)
    assert out.returncode == 0, out.stderr
    assert json.loads(out.stdout.strip().splitlines()[-1]) == []


# ---- host and path classification ----------------------------------------------------------------------------------

@pytest.mark.parametrize("host,port,cls", [
    ("api.openai.com", 443, "provider:openai"),
    ("myres.openai.azure.com", 443, "provider:azure_openai"),
    ("myres.privatelink.openai.azure.com", 443, "provider:azure_openai"),
    ("bedrock-runtime.us-east-1.amazonaws.com", 443, "provider:aws_bedrock"),
    ("127.0.0.1", 8000, "loopback"),
    ("localhost", 11434, "loopback"),
    ("10.20.30.40", 8000, "private-ip"),
    ("172.16.0.9", 80, "private-ip"),
    ("192.168.1.5", 80, "private-ip"),
    ("100.72.1.1", 80, "private-ip"),
    ("vllm.llm.svc.cluster.local", 8000, "private-dns"),
    ("gpu-box.corp", 8000, "private-dns"),
    ("host.docker.internal", 11434, "private-dns"),
    ("vllm", 8000, "local-name"),
    ("example.com", 443, "public"),
    ("8.8.8.8", 443, "public"),
])
def test_classify_host(host, port, cls):
    assert classify_host(host, port).cls == cls


def test_host_hints_are_weak():
    info = classify_host("vllm-prod.corp", 8000)
    assert 0 < info.points < detect.CANDIDATE  # a hint alone must never make a call a candidate


@pytest.mark.parametrize("path,api", [
    ("/v1/chat/completions", OPENAI_CHAT),
    ("/proxy/llm/v1/chat/completions/", OPENAI_CHAT),
    ("/chat/completions", OPENAI_CHAT),
    ("/openai/deployments/gpt-4o/chat/completions", OPENAI_CHAT),
    ("/v1/responses", OPENAI_RESPONSES),
    ("/v1/messages", ANTHROPIC),
    ("/api/chat", OPENAI_CHAT),
    ("/v1/completions", COMPLETIONS),
    ("/completion", COMPLETIONS),
    ("/generate", COMPLETIONS),
    ("/generate_stream", COMPLETIONS),
    ("/api/generate", COMPLETIONS),
    ("/v1beta/models/gemini-2.0-flash:generateContent", GEMINI),
    ("/v1beta/models/gemini-2.0-flash:streamGenerateContent", GEMINI),
    ("/model/anthropic.claude-3/converse", "generic"),
    ("/health", None),
])
def test_path_hint(path, api):
    assert path_hint(path) == api


def test_model_from_path():
    assert detect.model_from_path("/openai/deployments/my-gpt4/chat/completions") == "my-gpt4"
    assert detect.model_from_path("/v1beta/models/gemini-2.0-flash:generateContent") == "gemini-2.0-flash"
    assert detect.model_from_path("/model/anthropic.claude-3-5-sonnet-20240620-v1%3A0/converse") == "anthropic.claude-3-5-sonnet-20240620-v1:0"
    assert detect.model_from_path("/v1/chat/completions") is None


# ---- verdicts -----------------------------------------------------------------------------------------------------

def verdict(url_host, port, path, body, det=None, method="POST"):
    return (det or Detector(DetectConfig())).inspect(method=method, host=url_host, port=port, path=path, body=body)


@pytest.mark.parametrize("host,port,path,body,api", [
    ("api.openai.com", 443, "/v1/chat/completions", CHAT, OPENAI_CHAT),
    ("vllm.llm.svc.cluster.local", 8000, "/v1/chat/completions", CHAT, OPENAI_CHAT),
    ("gateway.corp.example", 443, "/internal/route-7/ask", CHAT, OPENAI_CHAT),  # any path: the body decides
    ("10.1.2.3", 8000, "/v1/completions", {"model": "m", "prompt": "hi", "max_tokens": 5}, COMPLETIONS),
    ("10.1.2.3", 8000, "/generate", {"prompt": "hi", "temperature": 0}, COMPLETIONS),
    ("tgi", 8080, "/generate", {"inputs": "hi", "parameters": {"max_new_tokens": 5}}, COMPLETIONS),
    ("ollama", 11434, "/api/generate", {"model": "llama3", "prompt": "hi"}, COMPLETIONS),
    ("ollama", 11434, "/api/chat", CHAT, OPENAI_CHAT),
    ("x.example.com", 443, "/v1/messages", {"model": "c", "max_tokens": 5, "messages": [{"role": "user", "content": "hi"}]}, ANTHROPIC),
    ("x.example.com", 443, "/any", {"model": "c", "max_tokens": 5, "system": "s", "messages": [{"role": "user", "content": "hi"}]}, ANTHROPIC),
    ("bedrock.example", 443, "/any", {"anthropic_version": "bedrock-2023-05-31", "max_tokens": 5, "messages": [{"role": "user", "content": [{"type": "text", "text": "hi"}]}]}, ANTHROPIC),
    ("g.example", 443, "/v1beta/models/gemini-pro:generateContent", {"contents": [{"role": "user", "parts": [{"text": "hi"}]}]}, GEMINI),
    ("g.example", 443, "/custom", {"contents": [{"role": "user", "parts": [{"text": "hi"}]}]}, GEMINI),
    ("x.example.com", 443, "/v1/responses", {"model": "m", "input": "hi"}, OPENAI_RESPONSES),
    ("x.example.com", 443, "/any", {"model": "m", "input": [{"role": "user", "content": "hi"}]}, OPENAI_RESPONSES),
    ("x.example.com", 443, "/any", {"model": "m", "input": "hi", "instructions": "be brief"}, OPENAI_RESPONSES),
    ("my.openai.azure.com", 443, "/openai/deployments/d1/chat/completions", {"messages": [{"role": "user", "content": "hi"}]}, OPENAI_CHAT),
])
def test_accepted(host, port, path, body, api):
    v = verdict(host, port, path, body)
    assert v.llm and v.api == api, (v.api, v.score, v.evidence, v.reject)
    assert v.evidence


@pytest.mark.parametrize("path,body", [
    ("/v1/embeddings", {"model": "m", "input": "hello world"}),
    ("/v1/images/generations", {"prompt": "a cat"}),
    ("/v1/audio/speech", {"model": "m", "input": "hello", "voice": "a"}),
    ("/v1/messages/count_tokens", CHAT),
    ("/v1/rerank", {"model": "m", "query": "q", "documents": ["a"], "prompt": "x"}),
    ("/v1/models/e:embedContent", {"contents": [{"parts": [{"text": "x"}]}]}),
])
def test_non_generative_endpoints_are_rejected(path, body):
    v = verdict("x.example.com", 443, path, body)
    assert not v.llm and not v.candidate and "non-generative" in v.reject


@pytest.mark.parametrize("body", [
    {"user": "a", "password": "b"},
    {"messages": "not a list"},
    {"messages": []},
    {"messages": [{"id": 1, "text": "x"}]},  # no roles: a chat widget, not a model call
    {"model": "m"},
    [],
    None,
])
def test_unrelated_bodies_are_rejected(body):
    v = verdict("x.example.com", 443, "/api/thing", body)
    assert not v.llm and not v.candidate


def test_get_is_never_a_call():
    assert not verdict("api.openai.com", 443, "/v1/chat/completions", CHAT, method="GET").llm


def test_ambiguous_bodies_need_the_response():
    det = Detector(DetectConfig())
    v = verdict("x.example.com", 443, "/thing/run", {"prompt": "write a poem"}, det)
    assert v.candidate and not v.llm and v.api == COMPLETIONS
    assert det.confirm(v, "x.example.com", 443, "/thing/run", json.dumps({"choices": [{"text": "x"}], "usage": {"prompt_tokens": 3}}), False)
    assert v.llm and not v.candidate


def test_ambiguous_body_with_non_llm_response_is_dropped_and_reported():
    det = Detector(DetectConfig())
    v = verdict("x.example.com", 443, "/thing/imagine", {"prompt": "a cat"}, det)
    assert v.candidate
    assert det.confirm(v, "x.example.com", 443, "/thing/imagine", json.dumps({"data": [{"url": "u"}]}), False) is None
    assert not v.llm
    assert "looked LLM-like" in det.render_summary()


def test_confirmed_endpoint_makes_later_ambiguous_calls_to_the_same_path_accepted():
    det = Detector(DetectConfig())
    first = verdict("llm.corp", 9000, "/x", {"prompt": "hello there"}, det)
    assert first.candidate
    det.confirm(first, "llm.corp", 9000, "/x", json.dumps({"usage": {"prompt_tokens": 2, "completion_tokens": 1}}), False)
    det.accept(first, "llm.corp", 9000, "/x")
    assert verdict("llm.corp", 9000, "/x/", {"prompt": "again"}, det).llm
    # One host can serve an LLM and other APIs: another path, host or port must prove itself again.
    assert not verdict("llm.corp", 9000, "/other", {"prompt": "again"}, det).llm
    assert not verdict("other.corp", 9000, "/x", {"prompt": "again"}, det).llm
    assert not verdict("llm.corp", 9001, "/x", {"prompt": "again"}, det).llm


def test_forced_host_and_path_and_exclusion():
    cfg = DetectConfig(hosts=["gpu*.corp", "10.9.0.0/16", "vllm:8000"], paths=[r"/ask$"], exclude_hosts=["noisy.corp"])
    det = Detector(cfg)
    assert verdict("gpu7.corp", 80, "/x", {"prompt": "hi"}, det).llm
    assert verdict("10.9.4.4", 80, "/x", {"prompt": "hi"}, det).llm
    assert verdict("vllm", 8000, "/x", {"prompt": "hi"}, det).llm
    assert not verdict("vllm", 9999, "/x", {"prompt": "hi"}, det).llm
    assert verdict("any.example", 443, "/team/ask", {"prompt": "hi"}, det).llm
    v = verdict("noisy.corp", 80, "/v1/chat/completions", CHAT, det)
    assert not v.llm and "excluded" in v.reject
    forced_unknown = verdict("gpu7.corp", 80, "/x", {"query": "hi"}, det)
    assert not forced_unknown.llm and "no recognizable" in forced_unknown.reject


def test_config_from_env():
    cfg = DetectConfig.from_env({"TOKPROF_LLM_HOSTS": "a.corp, b.corp:8000", "TOKPROF_LLM_PATHS": "/ask$", "TOKPROF_EXCLUDE_HOSTS": "c.corp"})
    assert cfg.hosts == ["a.corp", "b.corp:8000"] and cfg.paths == ["/ask$"] and cfg.exclude_hosts == ["c.corp"]


def test_host_matches_cidr_port_and_glob():
    assert host_matches(["10.0.0.0/8"], "10.2.3.4", 80)
    assert not host_matches(["10.0.0.0/8"], "11.2.3.4", 80)
    assert host_matches(["*.corp"], "gpu.corp", 80)
    assert host_matches(["gpu.corp:8000"], "gpu.corp", 8000) and not host_matches(["gpu.corp:8000"], "gpu.corp", 80)


def test_invalid_user_regex_is_ignored():
    det = Detector(DetectConfig(paths=["(unclosed"]))
    assert verdict("api.openai.com", 443, "/v1/chat/completions", CHAT, det).llm


# ---- response evidence ---------------------------------------------------------------------------------------------

@pytest.mark.parametrize("text,sse", [
    (json.dumps({"usage": {"prompt_tokens": 1}}), False),
    (json.dumps({"choices": [{"message": {"content": "x"}}]}), False),
    (json.dumps({"candidates": [{"content": {}}]}), False),
    (json.dumps({"content": [{"type": "text", "text": "x"}], "usage": {"input_tokens": 1}}), False),
    (json.dumps({"output": [{"type": "message"}]}), False),
    (json.dumps({"done": True, "prompt_eval_count": 3, "eval_count": 2}), False),
    (json.dumps({"generated_text": "x"}), False),
    ('{"response":"a","done":false}\n{"done":true,"eval_count":2}\n', False),
    ('data: {"choices":[{"delta":{"content":"x"}}]}\n\ndata: [DONE]\n\n', True),
    ('event: message_start\ndata: {"type":"message_start"}\n\n', True),
    (json.dumps([{"candidates": [{"content": {}}]}]), False),
])
def test_response_evidence_positive(text, sse):
    assert response_evidence(text, sse)


@pytest.mark.parametrize("text", ["", "not json", json.dumps({"data": [{"url": "u"}]}), json.dumps({"ok": True}), json.dumps({"error": "nope"}), "<html></html>"])
def test_response_evidence_negative(text):
    assert response_evidence(text, False) is None


# ---- explain -------------------------------------------------------------------------------------------------------

def test_explain_reports_reasons_and_override_hint():
    ok = explain("http://vllm.llm.svc.cluster.local:8000/v1/chat/completions", CHAT)
    assert "LLM call" in ok and "private-dns" in ok and "path looks like" in ok
    no = explain("https://example.com/login", {"user": "a"})
    assert "not recorded" in no and "--llm-host" in no
    amb = explain("https://example.com/thing/run", {"prompt": "hi"}, response=json.dumps({"usage": {"prompt_tokens": 1}}))
    assert "LLM call" in amb and "response has token usage" in amb
