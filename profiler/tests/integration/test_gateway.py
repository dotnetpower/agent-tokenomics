import asyncio
import json
import os
import socket
import threading
import urllib.error
import urllib.request

import pytest

from tokprof._mock_llm import MockProcess
from tokprof.gateway import BodyFramer, Gateway, GatewayThread, Route
from tokprof.runner import build_routes
from tokprof.store import JsonlWriter, ReadStats, read_jsonl

SALT = b"salt"


@pytest.fixture()
def mock():
    m = MockProcess()
    yield m
    m.stop()


def make_gateway(mock, sink, **kw):
    routes = [Route.from_urls("/openai/v1", mock.url + "/v1"), Route.from_urls("/anthropic", mock.url)]
    gt = GatewayThread(Gateway(routes, sink, salt=SALT, use_tiktoken=False, **kw))
    port = gt.start()
    return gt, port


def post(url, body):
    req = urllib.request.Request(url, data=json.dumps(body).encode(), headers={"Content-Type": "application/json"})
    with urllib.request.urlopen(req, timeout=10) as r:
        return r.status, r.read()


def test_records_nonstream_and_stream_through_gateway(mock):
    recs = []
    gt, port = make_gateway(mock, recs.append)
    try:
        base = f"http://127.0.0.1:{port}/openai/v1"
        msgs = [{"role": "system", "content": "sys " * 50}, {"role": "user", "content": "hello"}]
        st, data = post(base + "/chat/completions", {"model": "gpt-4o", "messages": msgs})
        assert st == 200 and json.loads(data)["choices"]
        st, data = post(base + "/chat/completions", {"model": "gpt-4o", "messages": msgs, "stream": True})
        assert b"[DONE]" in data  # client receives the stream unchanged
    finally:
        gt.stop()
    assert len(recs) == 2
    assert recs[0]["stream"] is False and recs[1]["stream"] is True
    assert all(r["usage"]["input_total"] > 0 for r in recs)
    assert {s["kind"] for s in recs[0]["segments"]} == {"system", "user_prompt"}


def test_tool_call_names_come_from_response(mock):
    recs = []
    gt, port = make_gateway(mock, recs.append)
    try:
        body = {"model": "gpt-4o", "messages": [{"role": "user", "content": "use-tool please"}],
                "tools": [{"type": "function", "function": {"name": "get_weather"}}, {"type": "function", "function": {"name": "unused_tool"}}]}
        post(f"http://127.0.0.1:{port}/openai/v1/chat/completions", body)
    finally:
        gt.stop()
    assert recs[0]["tool_calls"] == ["get_weather"]
    assert sorted(recs[0]["tools_defined"]) == ["get_weather", "unused_tool"]


def test_unknown_route_is_404_and_health_works(mock):
    recs = []
    gt, port = make_gateway(mock, recs.append)
    try:
        with pytest.raises(urllib.error.HTTPError) as e:
            urllib.request.urlopen(f"http://127.0.0.1:{port}/nope", timeout=5)
        assert e.value.code == 404
        health = json.loads(urllib.request.urlopen(f"http://127.0.0.1:{port}/__tokprof/health", timeout=5).read())
        assert health["ok"] is True
    finally:
        gt.stop()


def test_recording_failure_is_fail_open(mock):
    def broken_sink(_):
        raise RuntimeError("disk full")

    gt, port = make_gateway(mock, broken_sink)
    try:
        st, data = post(f"http://127.0.0.1:{port}/openai/v1/chat/completions", {"model": "m", "messages": [{"role": "user", "content": "hi"}]})
        assert st == 200 and json.loads(data)["choices"]  # the client is unaffected
    finally:
        gt.stop()  # drains in-flight recording before we look at the counters
    assert gt.gateway.stats["record_errors"] == 1


def test_non_json_and_non_llm_requests_pass_through_unrecorded(mock):
    recs = []
    gt, port = make_gateway(mock, recs.append)
    try:
        req = urllib.request.Request(f"http://127.0.0.1:{port}/openai/v1/chat/completions", data=b"not json", method="POST")
        with pytest.raises(urllib.error.HTTPError) as e:  # mock rejects bad JSON with 400; gateway relays it
            urllib.request.urlopen(req, timeout=5)
        assert e.value.code == 400
        assert urllib.request.urlopen(f"http://127.0.0.1:{port}/openai/v1/models", timeout=5).status == 200
    finally:
        gt.stop()
    assert recs == []


def test_upstream_down_returns_502():
    s = socket.socket()
    s.bind(("127.0.0.1", 0))
    dead_port = s.getsockname()[1]
    s.close()
    gt = GatewayThread(Gateway([Route.from_urls("/x", f"http://127.0.0.1:{dead_port}")], lambda r: None, salt=SALT))
    port = gt.start()
    try:
        with pytest.raises(urllib.error.HTTPError) as e:
            urllib.request.urlopen(f"http://127.0.0.1:{port}/x/anything", timeout=5)
        assert e.value.code == 502
    finally:
        gt.stop()


def test_concurrent_requests(mock):
    recs = []
    lock = threading.Lock()

    def sink(r):
        with lock:
            recs.append(r)

    gt, port = make_gateway(mock, sink)
    errors = []

    def worker(i):
        try:
            post(f"http://127.0.0.1:{port}/openai/v1/chat/completions", {"model": "m", "messages": [{"role": "user", "content": f"q{i}"}]})
        except Exception as exc:
            errors.append(exc)

    # Several rounds of concurrent clients. Regression test: on Python <= 3.11 handler tasks that were
    # not strongly referenced were garbage-collected mid-request, silently dropping ~5% of requests.
    for _ in range(8):
        ts = [threading.Thread(target=worker, args=(i,)) for i in range(20)]
        [t.start() for t in ts]
        [t.join(15) for t in ts]
    gt.stop()
    assert not errors and len(recs) == 160


def test_body_framer_chunked_and_length():
    f = BodyFramer({"transfer-encoding": "chunked"}, False)
    raw = b"5\r\nhello\r\n6\r\n world\r\n0\r\n\r\n"
    out = b"".join(f.feed(raw[i:i + 3]) for i in range(0, len(raw), 3))
    assert out == b"hello world" and f.done
    g = BodyFramer({"content-length": "4"}, False)
    assert g.feed(b"abcdEXTRA") == b"abcd" and g.done
    assert BodyFramer({}, True).done


def test_build_routes_keeps_existing_upstream_and_azure():
    routes, overrides, azure = build_routes({"OPENAI_BASE_URL": "http://localhost:4000/v1", "AZURE_OPENAI_ENDPOINT": "https://r1.openai.azure.com/"})
    assert overrides["OPENAI_BASE_URL"] == "http://127.0.0.1:{port}/openai/v1"
    oa = next(r for r in routes if r.prefix == "/openai/v1")
    assert (oa.scheme, oa.host, oa.port, oa.base_path) == ("http", "localhost", 4000, "/v1")
    assert overrides["AZURE_OPENAI_ENDPOINT"].endswith("/azure/r1.openai.azure.com") and "r1.openai.azure.com" in azure
    _, ov2, _ = build_routes({}, ["MY_BASE=http://h:9/api"])
    assert ov2["MY_BASE"].startswith("http://127.0.0.1:{port}/map0/api")


def test_azure_route_only_for_allowed_hosts():
    gw = Gateway([], lambda r: None, salt=SALT, azure_hosts={"mine.openai.azure.com"})
    assert gw._resolve("/azure/mine.openai.azure.com/openai/deployments/d/chat/completions?api-version=1")[1] == "mine.openai.azure.com"
    assert gw._resolve("/azure/evil.example.com/x") is None
    assert gw._resolve("/azure/other.openai.azure.com/x") is not None  # allowed suffix


def test_jsonl_writer_private_file_and_tolerant_reader(tmp_path):
    p = str(tmp_path / "out.jsonl")
    w = JsonlWriter(p)
    w.write({"segments": [], "a": 1})
    w.close()
    if os.name != "nt":
        assert oct(os.stat(p).st_mode & 0o777) == "0o600"
    with open(p, "a") as fh:
        fh.write("{broken\n\n")
        fh.write('{"segments": [], "a": 2}\n')
    stats = ReadStats()
    got = list(read_jsonl(p, stats))
    assert [g["a"] for g in got] == [1, 2] and stats.bad_lines == 1
