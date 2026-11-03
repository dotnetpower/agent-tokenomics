import os
import subprocess
import sys
import time

import pytest

from tokprof._mock_llm import MockProcess
from tokprof.session import render_env
from tokprof.store import read_jsonl

pytestmark = pytest.mark.skipif(os.name == "nt", reason="background session is POSIX-only for now")


def cli(*args, env=None, check=True):
    r = subprocess.run([sys.executable, "-m", "tokprof", *args], capture_output=True, text=True, env=env, timeout=60)
    if check:
        assert r.returncode == 0, r.stderr + r.stdout
    return r


def test_session_lifecycle_routes_new_processes(tmp_path):
    mock = MockProcess()
    out = str(tmp_path / "s.jsonl")
    state = str(tmp_path / "state.json")
    base_env = dict(os.environ, OPENAI_BASE_URL=mock.url + "/v1", ANTHROPIC_BASE_URL=mock.url)
    try:
        assert cli("status", "--state", state, check=False).returncode == 1  # nothing running
        cli("start", "--out", out, "--state", state, "--no-tiktoken", env=base_env)
        assert "active session" in cli("status", "--state", state).stdout
        assert cli("start", "--out", out, "--state", state, check=False).returncode == 1  # no second session

        exports = cli("env", "--state", state, "--shell", "bash").stdout
        assert "export OPENAI_BASE_URL='http://127.0.0.1:" in exports and "NODE_OPTIONS" in exports
        unset = cli("env", "--state", state, "--shell", "bash", "--unset").stdout
        assert "OPENAI_BASE_URL=" in unset  # restored to the original (mock) value

        child_env = dict(os.environ)
        for line in exports.splitlines():
            k, _, v = line.replace("export ", "", 1).partition("=")
            child_env[k] = v.strip("'")
        script = (
            "import json, os, urllib.request\n"
            "b = os.environ['OPENAI_BASE_URL']\n"
            "req = urllib.request.Request(b + '/chat/completions', data=json.dumps({'model':'m','messages':[{'role':'user','content':'hi'}]}).encode(), headers={'Content-Type':'application/json'})\n"
            "urllib.request.urlopen(req, timeout=10).read()\n"
        )
        subprocess.run([sys.executable, "-c", script], env=child_env, check=True, timeout=30)

        time.sleep(0.3)
        r = cli("stop", "--state", state, check=False)
        assert r.returncode == 2  # just handled a request: refuse without --force
        assert cli("stop", "--state", state, "--force").returncode == 0
        assert cli("status", "--state", state, check=False).returncode == 1
        assert len(list(read_jsonl(out))) == 1
    finally:
        cli("stop", "--state", state, "--force", check=False)
        mock.stop()


def test_render_env_shells_and_unset():
    st = {"env": {"A": "x'y"}, "originals": {"A": None, "B": "old"}}
    assert render_env(st, "bash") == "export A='x'\"'\"'y'"
    assert render_env(st, "powershell") == "$env:A = 'x''y'"
    assert render_env(st, "cmd") == "set A=x'y"
    assert render_env(st, "fish") == "set -gx A 'x'\"'\"'y'"
    assert render_env(st, "bash", unset=True).splitlines() == ["unset A", "export B='old'"]
