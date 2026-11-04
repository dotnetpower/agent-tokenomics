"""A tiny local mock of OpenAI-chat, OpenAI-responses, and Anthropic-messages APIs.

Used by `tokprof selftest` and the tests. Usage numbers are computed from the request with a fixed rule
(content tokens plus 4 per message), so a profile of the mock's traffic has a known, small residual.
"""

from __future__ import annotations

import json
from typing import Optional
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
    for key in ("prompt", "inputs"):
        if isinstance(body.get(key), str):
            n += _C.count(body[key])
    for c in body.get("contents") or []:  # Gemini
        for part in (c or {}).get("parts") or []:
            if isinstance(part, dict) and isinstance(part.get("text"), str):
                n += _C.count(part["text"]) + PER_MESSAGE_OVERHEAD
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

        # Self-hosted style servers (Ollama, llama.cpp, Gemini-shaped) used by the endpoint-detection tests.
        if path.endswith("/api/chat") or path.endswith("/api/generate"):
            chat = path.endswith("/api/chat")
            chunk = {"model": body.get("model"), "done": False}
            chunk.update({"message": {"role": "assistant", "content": reply}} if chat else {"response": reply})
            last = {"model": body.get("model"), "done": True, "prompt_eval_count": pt, "eval_count": ot}
            lines = (json.dumps(chunk) + "\n" + json.dumps(last) + "\n").encode()
            self.send_response(200)
            self.send_header("Content-Type", "application/x-ndjson")
            self.send_header("Content-Length", str(len(lines)))
            self.end_headers()
            self.wfile.write(lines)
            return
        if ":generateContent" in path or ":streamGenerateContent" in path:
            usage = {"promptTokenCount": pt, "candidatesTokenCount": ot, "totalTokenCount": pt + ot}
            cand = [{"content": {"role": "model", "parts": [{"text": reply}]}}]
            if ":stream" in path:
                return self._sse([f"data: {json.dumps({'candidates': cand})}\n\n", f"data: {json.dumps({'candidates': cand, 'usageMetadata': usage})}\n\n"])
            return self._json({"candidates": cand, "usageMetadata": usage})
        if path.endswith("/completions") and "messages" not in body or path.endswith("/generate"):
            if path.endswith("/generate"):
                return self._json({"generated_text": reply, "details": {"prefill": [0] * pt, "generated_tokens": ot}})
            return self._json({"choices": [{"text": reply}], "usage": {"prompt_tokens": pt, "completion_tokens": ot}})
        if path.endswith("/embeddings"):
            return self._json({"data": [{"embedding": [0.1, 0.2]}], "usage": {"prompt_tokens": pt, "total_tokens": pt}})
        if path.endswith("/thing/run"):  # ambiguous body, but the answer is a model answer
            return self._json({"choices": [{"text": reply}], "usage": {"prompt_tokens": pt, "completion_tokens": ot}})
        if path.endswith("/thing/imagine"):  # same body shape, not an LLM
            return self._json({"data": [{"url": "https://example.invalid/cat.png"}]})
        known_suffix = path.endswith(("/chat/completions", "/responses", "/messages"))
        if path.endswith("/chat/completions") or (not known_suffix and "messages" in body):
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
    def __init__(self, tls: Optional[tuple] = None) -> None:
        self.server = _Server(("127.0.0.1", 0), _Handler)
        self.tls = tls
        if tls:
            import ssl

            ctx = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
            ctx.load_cert_chain(*tls)
            self.server.socket = ctx.wrap_socket(self.server.socket, server_side=True)
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
        return f"{'https' if self.tls else 'http'}://localhost:{self.port}" if self.tls else f"http://127.0.0.1:{self.port}"


class MockProcess:
    """The mock in its own process: keeps tests and selftest independent of this process's threads."""

    def __init__(self, tls: Optional[tuple] = None) -> None:
        import subprocess
        import sys

        self.tls = tls
        args = [sys.executable, "-m", "tokprof._mock_llm"] + (["--tls", tls[0], tls[1]] if tls else [])
        self._proc = subprocess.Popen(args, stdout=subprocess.PIPE, text=True)
        line = self._proc.stdout.readline().strip()
        if not line.isdigit():
            self._proc.kill()
            raise RuntimeError("mock LLM failed to start")
        self.port = int(line)

    @property
    def url(self) -> str:
        return f"https://localhost:{self.port}" if self.tls else f"http://127.0.0.1:{self.port}"

    def start(self) -> "MockProcess":
        return self

    def stop(self) -> None:
        self._proc.kill()
        self._proc.wait(5)
        self._proc.stdout.close()


if __name__ == "__main__":
    import time

    import sys

    _m = MockLLM(tuple(sys.argv[2:4]) if len(sys.argv) >= 4 and sys.argv[1] == "--tls" else None).start()
    print(_m.port, flush=True)
    while True:
        time.sleep(3600)
