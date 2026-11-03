"""A tiny local mock of OpenAI-chat, OpenAI-responses, and Anthropic-messages APIs.

Used by `tokprof selftest` and the tests. Usage numbers are computed from the request with a fixed rule
(content tokens plus 4 per message), so a profile of the mock's traffic has a known, small residual.
"""

from __future__ import annotations

import json
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

from .counting import HeuristicCounter

_C = HeuristicCounter()
PER_MESSAGE_OVERHEAD = 4


def _prompt_tokens(body: dict) -> int:
    n = 0
    system = body.get("system")
    if isinstance(system, str):
        n += _C.count(system)
    for m in body.get("messages") or body.get("input") or []:
        if isinstance(m, dict):
            c = m.get("content")
            text = c if isinstance(c, str) else json.dumps(c) if c is not None else ""
            n += _C.count(text) + PER_MESSAGE_OVERHEAD
    if isinstance(body.get("input"), str):
        n += _C.count(body["input"])
    if isinstance(body.get("instructions"), str):
        n += _C.count(body["instructions"])
    for t in body.get("tools") or []:
        n += _C.count(json.dumps(t, ensure_ascii=False, separators=(",", ":"), sort_keys=True))
    return n


class _Handler(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def log_message(self, *args):  # silence
        pass

    def _json(self, obj, code=200):
        data = json.dumps(obj).encode()
        self.send_response(code)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def _sse(self, events):
        self.send_response(200)
        self.send_header("Content-Type", "text/event-stream")
        self.send_header("Transfer-Encoding", "chunked")
        self.end_headers()
        for ev in events:
            payload = ev.encode()
            self.wfile.write(f"{len(payload):x}\r\n".encode() + payload + b"\r\n")
            self.wfile.flush()
        self.wfile.write(b"0\r\n\r\n")

    def do_POST(self):
        n = int(self.headers.get("Content-Length", "0"))
        try:
            body = json.loads(self.rfile.read(n) or b"{}")
        except ValueError:
            return self._json({"error": {"message": "bad json"}}, 400)
        path = self.path.split("?", 1)[0]
        pt = _prompt_tokens(body)
        reply = "ok " * 5
        ot = _C.count(reply)
        stream = bool(body.get("stream"))
        want_tool = any((m.get("role") == "user" and "use-tool" in json.dumps(m.get("content"))) for m in (body.get("messages") or []) if isinstance(m, dict))

        if path.endswith("/chat/completions"):
            if stream:
                evs = [
                    f"data: {json.dumps({'choices': [{'delta': {'content': reply}}]})}\n\n",
                    f"data: {json.dumps({'choices': [], 'usage': {'prompt_tokens': pt, 'completion_tokens': ot, 'prompt_tokens_details': {'cached_tokens': 0}}})}\n\n",
                    "data: [DONE]\n\n",
                ]
                return self._sse(evs)
            msg = {"role": "assistant", "content": reply}
            if want_tool:
                msg = {"role": "assistant", "content": None, "tool_calls": [{"id": "c1", "type": "function", "function": {"name": "get_weather", "arguments": "{}"}}]}
            return self._json({"choices": [{"message": msg}], "usage": {"prompt_tokens": pt, "completion_tokens": ot, "prompt_tokens_details": {"cached_tokens": 0}}})
        if path.endswith("/responses"):
            usage = {"input_tokens": pt, "output_tokens": ot, "input_tokens_details": {"cached_tokens": 0}}
            if stream:
                evs = [
                    f"data: {json.dumps({'type': 'response.output_text.delta', 'delta': reply})}\n\n",
                    f"data: {json.dumps({'type': 'response.completed', 'response': {'usage': usage}})}\n\n",
                ]
                return self._sse(evs)
            return self._json({"output": [{"type": "message", "content": [{"type": "output_text", "text": reply}]}], "usage": usage})
        if path.endswith("/messages"):
            if stream:
                evs = [
                    f"event: message_start\ndata: {json.dumps({'type': 'message_start', 'message': {'usage': {'input_tokens': pt, 'output_tokens': 1}}})}\n\n",
                    f"event: content_block_delta\ndata: {json.dumps({'type': 'content_block_delta', 'delta': {'type': 'text_delta', 'text': reply}})}\n\n",
                    f"event: message_delta\ndata: {json.dumps({'type': 'message_delta', 'usage': {'output_tokens': ot}})}\n\n",
                ]
                return self._sse(evs)
            return self._json({"content": [{"type": "text", "text": reply}], "usage": {"input_tokens": pt, "output_tokens": ot}})
        self._json({"error": {"message": "not found"}}, 404)

    def do_GET(self):
        self._json({"ok": True})


class _Server(ThreadingHTTPServer):
    request_queue_size = 128  # the default backlog of 5 resets connections under concurrent load


class MockLLM:
    def __init__(self) -> None:
        self.server = _Server(("127.0.0.1", 0), _Handler)
        self.server.daemon_threads = True
        self.port = self.server.server_address[1]
        self._thread = threading.Thread(target=self.server.serve_forever, daemon=True)

    def start(self) -> "MockLLM":
        self._thread.start()
        return self

    def stop(self) -> None:
        self.server.shutdown()
        self.server.server_close()

    @property
    def url(self) -> str:
        return f"http://127.0.0.1:{self.port}"


class MockProcess:
    """The mock in its own process: keeps tests and selftest independent of this process's threads."""

    def __init__(self) -> None:
        import subprocess
        import sys

        self._proc = subprocess.Popen([sys.executable, "-m", "tokprof._mock_llm"], stdout=subprocess.PIPE, text=True)
        line = self._proc.stdout.readline().strip()
        if not line.isdigit():
            self._proc.kill()
            raise RuntimeError("mock LLM failed to start")
        self.port = int(line)

    @property
    def url(self) -> str:
        return f"http://127.0.0.1:{self.port}"

    def start(self) -> "MockProcess":
        return self

    def stop(self) -> None:
        self._proc.kill()
        self._proc.wait(5)
        self._proc.stdout.close()


if __name__ == "__main__":
    import time

    _m = MockLLM().start()
    print(_m.port, flush=True)
    while True:
        time.sleep(3600)
