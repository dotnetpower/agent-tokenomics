import json
import os
import shutil
import subprocess
import sys
import urllib.error
import urllib.request

import pytest

from tokprof._mock_llm import MockProcess
from tokprof.runner import run_profiled
from tokprof.store import read_jsonl

pytestmark = pytest.mark.skipif(shutil.which("openssl") is None, reason="openssl not installed")

CLIENT = r"""
import json, os, urllib.request, urllib.error
base = os.environ["PRIVATE_LLM_BASE_URL"]
body = {"model": "qwen", "messages": [{"role": "user", "content": "hello"}]}
req = urllib.request.Request(base + "/api/ai/ask", data=json.dumps(body).encode(), headers={"Content-Type": "application/json"})
try:
    print("OK", urllib.request.urlopen(req).status)
except urllib.error.HTTPError as e:
    print("HTTP", e.code, e.read().decode()[:300])
"""


@pytest.fixture()
def tls_mock(tmp_path):
    key, crt = str(tmp_path / "k.pem"), str(tmp_path / "ca.pem")
    subprocess.run(["openssl", "req", "-x509", "-newkey", "rsa:2048", "-nodes", "-keyout", key, "-out", crt, "-days", "1",
                    "-subj", "/CN=localhost", "-addext", "subjectAltName=DNS:localhost"], check=True, capture_output=True)
    m = MockProcess(tls=(crt, key))
    yield m, crt
    m.stop()


def run(mock_url, out, capfd, env):
    old = {k: os.environ.get(k) for k in env}
    os.environ.update(env)
    os.environ["PRIVATE_LLM_BASE_URL"] = mock_url
    try:
        code = run_profiled([sys.executable, "-c", CLIENT], str(out), quiet=True, use_tiktoken=False, hooks=False)
    finally:
        os.environ.pop("PRIVATE_LLM_BASE_URL", None)
        for k, v in old.items():
            if v is None:
                os.environ.pop(k, None)
            else:
                os.environ[k] = v
    return code, capfd.readouterr().out


def test_private_ca_is_refused_with_a_helpful_hint_then_trusted(tmp_path, tls_mock, capfd):
    mock, ca = tls_mock
    out = tmp_path / "a.jsonl"
    # The route comes from the pattern-matched variable name (PRIVATE_LLM_BASE_URL contains LLM and ends with _BASE_URL).
    code, stdout = run(mock.url, out, capfd, {})
    assert "HTTP 502" in stdout and "--upstream-ca" in stdout
    assert not out.exists() or list(read_jsonl(str(out))) == []

    out2 = tmp_path / "b.jsonl"
    code, stdout = run(mock.url, out2, capfd, {"TOKPROF_UPSTREAM_CA": ca})
    assert "OK 200" in stdout
    recs = list(read_jsonl(str(out2)))
    assert len(recs) == 1 and recs[0]["provider"] == "openai_chat" and recs[0]["host_class"] == "loopback"


def test_ca_bundle_from_the_client_environment_is_honored(tmp_path, tls_mock, capfd):
    mock, ca = tls_mock
    code, stdout = run(mock.url, tmp_path / "c.jsonl", capfd, {"SSL_CERT_FILE": ca})
    assert "OK 200" in stdout


def test_insecure_upstream_is_an_explicit_opt_in(tmp_path, tls_mock, capfd):
    mock, _ = tls_mock
    code, stdout = run(mock.url, tmp_path / "d.jsonl", capfd, {"TOKPROF_INSECURE_UPSTREAM": "1"})
    assert "OK 200" in stdout
