"""tokprof in-process hook for Python (loaded through sitecustomize; standard library only).

Activated only when TOKPROF_PORT and TOKPROF_TOKEN are set by `tokprof run`. It copies LLM request/response
bodies that httpx sends to the local tokprof gateway's ingest endpoint, so the analysis happens in the
tokprof process. It never modifies requests or responses and swallows its own errors.
"""

import atexit
import gzip
import http.client
import json
import os
import queue
import re
import sys
import threading
import time
import zlib

_PORT = os.environ.get("TOKPROF_PORT", "")
_TOKEN = os.environ.get("TOKPROF_TOKEN", "")
_LLM_PATH = re.compile(r"/(chat/completions|responses|messages)$")
_CAP = 32 * 1024 * 1024
_q = queue.Queue(maxsize=2000)
_started = False
_warned = False
_inflight = 0
_lock = threading.Lock()


def _warn(msg):
    global _warned
    if not _warned:
        _warned = True
        try:
            sys.stderr.write("tokprof(hook): " + msg + "\n")
        except Exception:
            pass


def _worker():
    global _inflight
    while True:
        payload = _q.get()
        try:
            conn = http.client.HTTPConnection("127.0.0.1", int(_PORT), timeout=5)
            conn.request("POST", "/__tokprof/ingest", body=json.dumps(payload).encode("utf-8"),
                         headers={"Content-Type": "application/json", "X-Tokprof-Token": _TOKEN})
            conn.getresponse().read()
            conn.close()
        except Exception as exc:  # the profiler must never disturb the application
            _warn("could not deliver a captured call: %r" % (exc,))
        finally:
            with _lock:
                _inflight -= 1


def _send(payload):
    global _started, _inflight
    if not _started:
        _started = True
        threading.Thread(target=_worker, name="tokprof-hook", daemon=True).start()
        atexit.register(_flush)
    try:
        with _lock:
            _inflight += 1
        _q.put_nowait(payload)
    except queue.Full:
        with _lock:
            _inflight -= 1


def _flush(timeout=3.0):
    """Give queued captures a moment to be delivered before the interpreter exits."""
    deadline = time.time() + timeout
    while time.time() < deadline:
        with _lock:
            if _inflight <= 0:
                return
        time.sleep(0.01)


def _decode(data, encoding):
    enc = (encoding or "").lower()
    if "gzip" in enc:
        data = gzip.decompress(data)
    elif "deflate" in enc:
        try:
            data = zlib.decompress(data)
        except zlib.error:
            data = zlib.decompress(data, -zlib.MAX_WBITS)
    elif enc and enc != "identity":
        raise ValueError("unsupported content-encoding " + enc)
    return data.decode("utf-8", "replace")


class _Capture:
    """Collects one exchange and ships it when the response body ends."""

    def __init__(self, request, response):
        self.chunks = []
        self.size = 0
        self.t0 = time.time()
        self.ttfb = None
        self.done = False
        self.request = request
        self.response = response

    def add(self, chunk):
        if self.ttfb is None:
            self.ttfb = int((time.time() - self.t0) * 1000)
        if self.size < _CAP:
            self.chunks.append(chunk)
            self.size += len(chunk)

    def finish(self, error=None):
        if self.done:
            return
        self.done = True
        try:
            req, resp = self.request, self.response
            headers = resp.headers
            body = b"".join(self.chunks)
            text = _decode(body, headers.get("content-encoding"))
            content = req.content if isinstance(req.content, (bytes, bytearray)) else b""
            _send({
                "path": req.url.path,
                "request": bytes(content).decode("utf-8", "replace"),
                "status": resp.status_code,
                "response": text,
                "sse": "text/event-stream" in headers.get("content-type", ""),
                "ts": self.t0,
                "ttfb_ms": self.ttfb,
                "error": error,
            })
        except Exception as exc:
            _warn("could not capture a call: %r" % (exc,))


def _wanted(request):
    try:
        if request.method != "POST":
            return False
        url = request.url
        if url.host in ("127.0.0.1", "localhost") and str(url.port) == _PORT:
            return False  # already going through the gateway
        return bool(_LLM_PATH.search(url.path))
    except Exception:
        return False


def _patch_httpx(httpx):
    sync_cls = httpx.HTTPTransport
    async_cls = httpx.AsyncHTTPTransport
    if getattr(sync_cls, "_tokprof_patched", False):
        return
    orig_sync = sync_cls.handle_request
    orig_async = async_cls.handle_async_request

    class TeeSync(httpx.SyncByteStream):
        def __init__(self, inner, cap):
            self._inner, self._cap = inner, cap

        def __iter__(self):
            for chunk in self._inner:
                self._cap.add(chunk)
                yield chunk

        def close(self):
            try:
                if hasattr(self._inner, "close"):
                    self._inner.close()
            finally:
                self._cap.finish()

    class TeeAsync(httpx.AsyncByteStream):
        def __init__(self, inner, cap):
            self._inner, self._cap = inner, cap

        async def __aiter__(self):
            async for chunk in self._inner:
                self._cap.add(chunk)
                yield chunk

        async def aclose(self):
            try:
                if hasattr(self._inner, "aclose"):
                    await self._inner.aclose()
            finally:
                self._cap.finish()

    def handle_request(self, request):
        resp = orig_sync(self, request)
        try:
            if _wanted(request):
                resp.stream = TeeSync(resp.stream, _Capture(request, resp))
        except Exception as exc:
            _warn("hook error: %r" % (exc,))
        return resp

    async def handle_async_request(self, request):
        resp = await orig_async(self, request)
        try:
            if _wanted(request):
                resp.stream = TeeAsync(resp.stream, _Capture(request, resp))
        except Exception as exc:
            _warn("hook error: %r" % (exc,))
        return resp

    sync_cls.handle_request = handle_request
    async_cls.handle_async_request = handle_async_request
    sync_cls._tokprof_patched = True


class _Finder:
    """Patch httpx right after it is first imported."""

    _busy = False

    def find_spec(self, name, path=None, target=None):
        if name != "httpx" or _Finder._busy:
            return None
        import importlib.util

        _Finder._busy = True
        try:
            spec = importlib.util.find_spec(name)
        finally:
            _Finder._busy = False
        if spec is None or spec.loader is None:
            return None
        loader = spec.loader
        orig_exec = loader.exec_module

        def exec_module(module):
            orig_exec(module)
            try:
                _patch_httpx(module)
            except Exception as exc:
                _warn("could not hook httpx: %r" % (exc,))

        loader.exec_module = exec_module
        return spec


def install():
    if not (_PORT and _TOKEN):
        return
    if "httpx" in sys.modules:
        try:
            _patch_httpx(sys.modules["httpx"])
        except Exception as exc:
            _warn("could not hook httpx: %r" % (exc,))
    else:
        sys.meta_path.insert(0, _Finder())
