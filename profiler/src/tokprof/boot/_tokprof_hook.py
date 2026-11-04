"""tokprof in-process hook for Python (loaded through sitecustomize; standard library only).

Activated only when TOKPROF_PORT and TOKPROF_TOKEN are set by `tokprof run`. It copies LLM request/response
bodies that httpx, requests and aiohttp send to the local tokprof gateway's ingest endpoint, so the analysis happens in the
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
from urllib.parse import urlsplit

_PORT = os.environ.get("TOKPROF_PORT", "")
_TOKEN = os.environ.get("TOKPROF_TOKEN", "")
# Pre-filter only; tokprof's detector decides. Keep identical to tokprof.detect.plausible (tests/vectors/prefilter.json).
_EXCLUDED_PATH = re.compile(
    r"/(embeddings?|moderations?|rerank|tokenize|detokenize|count_tokens|"
    r"audio/(speech|transcriptions|translations)|images?/(generations|edits|variations)|"
    r"files|batches|fine_tuning|uploads|vector_stores|assistants|threads|realtime)(/|$)"
    r"|:(embedContent|batchEmbedContents|countTokens)$",
    re.I,
)
_PLAUSIBLE_KEYS = re.compile(r'"(?:messages|contents|chat_history)"\s*:\s*\[|"(?:prompt|input|inputs|preamble|message)"\s*:')
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
    enc = (encoding or "").lower().strip()
    if not enc or enc == "identity":
        return data.decode("utf-8", "replace")
    try:
        if "gzip" in enc:
            return gzip.decompress(data).decode("utf-8", "replace")
        if "deflate" in enc:
            try:
                return zlib.decompress(data).decode("utf-8", "replace")
            except zlib.error:
                return zlib.decompress(data, -zlib.MAX_WBITS).decode("utf-8", "replace")
        if "br" in enc:
            for mod_name in ("brotli", "brotlicffi"):
                try:
                    return __import__(mod_name).decompress(data).decode("utf-8", "replace")
                except Exception:
                    pass
            return ""
        if "zstd" in enc:
            try:
                from compression import zstd as _zstd

                return _zstd.decompress(data).decode("utf-8", "replace")
            except Exception:
                try:
                    import zstandard

                    return zstandard.ZstdDecompressor().decompress(data, max_output_size=_CAP).decode("utf-8", "replace")
                except Exception:
                    return ""
    except Exception:
        return ""
    return ""


class _Capture:
    """Collects one exchange and ships it when the response body ends.

    ``meta`` is called once at the end and returns (status, content-encoding, content-type, already_decoded).
    """

    def __init__(self, path, request_text, meta, host="", port=0):
        self.chunks = []
        self.size = 0
        self.t0 = time.time()
        self.ttfb = None
        self.done = False
        self.path = path
        self.request_text = request_text
        self.meta = meta
        self.host = host
        self.port = port

    def add(self, chunk):
        if self.ttfb is None:
            self.ttfb = int((time.time() - self.t0) * 1000)
        if self.size < _CAP:
            self.chunks.append(bytes(chunk))
            self.size += len(chunk)

    def finish(self, error=None):
        if self.done:
            return
        self.done = True
        try:
            status, encoding, ctype, decoded = self.meta()
            body = b"".join(self.chunks)
            text = body.decode("utf-8", "replace") if decoded else _decode(body, encoding)
            _send({
                "path": self.path,
                "host": self.host,
                "port": self.port,
                "request": self.request_text,
                "status": status,
                "response": text,
                "sse": "text/event-stream" in (ctype or ""),
                "ts": self.t0,
                "ttfb_ms": self.ttfb,
                "latency_ms": int((time.time() - self.t0) * 1000),
                "error": error,
            })
        except Exception as exc:
            _warn("could not capture a call: %r" % (exc,))


def _to_text(body):
    if isinstance(body, (bytes, bytearray, memoryview)):
        return bytes(body).decode("utf-8", "replace")
    if isinstance(body, str):
        return body
    if isinstance(body, dict):
        try:
            return json.dumps(body)
        except Exception:
            return None
    if hasattr(body, "read") and hasattr(body, "seek") and hasattr(body, "tell"):
        try:
            pos = body.tell()
            raw = body.read(_CAP)
            body.seek(pos)
            return _to_text(raw)
        except Exception:
            return None
    return None  # non-seekable streams or multipart bodies are not captured


def _httpx_capture(request, resp):
    content = request.content if isinstance(request.content, (bytes, bytearray)) else b""
    headers = resp.headers
    return _Capture(
        request.url.path,
        bytes(content).decode("utf-8", "replace"),
        lambda: (resp.status_code, headers.get("content-encoding"), headers.get("content-type", ""), False),
        request.url.host,
        request.url.port or (443 if request.url.scheme == "https" else 80),
    )


def _plausible(method, path, text):
    try:
        if (method or "").upper() != "POST" or not text:
            return False
        head = text.lstrip()
        if len(head) < 8 or head[0] != "{":
            return False
        p = (path or "").split("?", 1)[0].split("#", 1)[0].rstrip("/") or "/"
        if _EXCLUDED_PATH.search(p):
            return False
        return _PLAUSIBLE_KEYS.search(text) is not None
    except Exception:
        return False


def _is_candidate(method, host, port, path, text):
    if host in ("127.0.0.1", "localhost") and str(port) == _PORT:
        return False  # already going through the gateway
    return _plausible(method, path, text)


def _wanted(request):
    try:
        url = request.url
        content = request.content if isinstance(request.content, (bytes, bytearray)) else b""
        return _is_candidate(request.method, url.host, url.port, url.path, bytes(content).decode("utf-8", "replace"))
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
                resp.stream = TeeSync(resp.stream, _httpx_capture(request, resp))
        except Exception as exc:
            _warn("hook error: %r" % (exc,))
        return resp

    async def handle_async_request(self, request):
        resp = await orig_async(self, request)
        try:
            if _wanted(request):
                resp.stream = TeeAsync(resp.stream, _httpx_capture(request, resp))
        except Exception as exc:
            _warn("hook error: %r" % (exc,))
        return resp

    sync_cls.handle_request = handle_request
    async_cls.handle_async_request = handle_async_request
    sync_cls._tokprof_patched = True


_in_requests = threading.local()


def _patch_requests(requests):
    from requests.adapters import HTTPAdapter

    if getattr(HTTPAdapter, "_tokprof_patched", False):
        return
    orig_send = HTTPAdapter.send

    def send(self, request, *args, **kwargs):
        _in_requests.depth = getattr(_in_requests, "depth", 0) + 1
        try:
            resp = orig_send(self, request, *args, **kwargs)
        finally:
            _in_requests.depth -= 1
        try:
            parts = urlsplit(request.url)
            text = _to_text(request.body)
            if text is not None and _is_candidate(request.method, parts.hostname, parts.port, parts.path, text):
                cap = _Capture(
                    parts.path, text,
                    # iter_content yields bytes already decoded by urllib3
                    lambda: (resp.status_code, None, resp.headers.get("content-type", ""), True),
                    parts.hostname or "", parts.port or (443 if parts.scheme == "https" else 80),
                )
                orig_iter = resp.iter_content

                def iter_content(chunk_size=1, decode_unicode=False):
                    try:
                        for chunk in orig_iter(chunk_size, decode_unicode):
                            cap.add(chunk.encode("utf-8") if isinstance(chunk, str) else chunk)
                            yield chunk
                    finally:
                        cap.finish()

                resp.iter_content = iter_content
        except Exception as exc:
            _warn("hook error: %r" % (exc,))
        return resp

    HTTPAdapter.send = send
    HTTPAdapter._tokprof_patched = True


def _patch_urllib3(urllib3):
    from urllib3.connectionpool import HTTPConnectionPool

    if getattr(HTTPConnectionPool, "_tokprof_patched", False):
        return
    orig_urlopen = HTTPConnectionPool.urlopen

    def urlopen(self, method, url, body=None, *args, **kwargs):
        resp = orig_urlopen(self, method, url, body=body, *args, **kwargs)
        if getattr(_in_requests, "depth", 0) > 0:
            return resp  # requests already taps this exchange at the HTTPAdapter layer
        try:
            parts = urlsplit(url)
            path = parts.path or "/"
            host = getattr(self, "host", "") or parts.hostname or ""
            scheme = getattr(self, "scheme", "http") or parts.scheme or "http"
            port = getattr(self, "port", None) or parts.port or (443 if scheme == "https" else 80)
            text = _to_text(body if body is not None else kwargs.get("body"))
            if text is not None and _is_candidate(method, host, port, path, text):
                headers = getattr(resp, "headers", {}) or {}
                cap = _Capture(
                    path, text,
                    lambda: (getattr(resp, "status", 200), None, headers.get("content-type", "") or headers.get("Content-Type", ""), True),
                    host, int(port),
                )
                preloaded = getattr(resp, "_body", None)
                if isinstance(preloaded, (bytes, bytearray)):
                    cap.add(preloaded)
                    cap.finish()
                else:
                    orig_stream = resp.stream
                    orig_read = resp.read

                    def stream(amt=65536, decode_content=None, **skw):
                        try:
                            for chunk in orig_stream(amt, decode_content=decode_content, **skw):
                                if chunk:
                                    cap.add(chunk.encode("utf-8") if isinstance(chunk, str) else chunk)
                                yield chunk
                        finally:
                            cap.finish()

                    def read(amt=None, decode_content=None, **rkw):
                        data = orig_read(amt, decode_content=decode_content, **rkw)
                        if data:
                            cap.add(data.encode("utf-8") if isinstance(data, str) else data)
                        if amt is None or not data:
                            cap.finish()
                        return data

                    resp.stream = stream
                    resp.read = read
        except Exception as exc:
            _warn("hook error: %r" % (exc,))
        return resp

    HTTPConnectionPool.urlopen = urlopen
    HTTPConnectionPool._tokprof_patched = True


def _patch_urllib_request(urllib_request):
    OpenerDirector = urllib_request.OpenerDirector
    if getattr(OpenerDirector, "_tokprof_patched", False):
        return
    orig_open = OpenerDirector.open

    def open_(self, fullurl, data=None, *args, **kwargs):
        resp = orig_open(self, fullurl, data, *args, **kwargs)
        try:
            if isinstance(fullurl, urllib_request.Request):
                url = fullurl.full_url
                body = data if data is not None else fullurl.data
                method = fullurl.get_method()
            else:
                url = str(fullurl)
                body = data
                method = "POST" if body is not None else "GET"
            parts = urlsplit(url)
            port = parts.port or (443 if parts.scheme == "https" else 80)
            text = _to_text(body)
            if text is not None and _is_candidate(method, parts.hostname, port, parts.path, text):
                headers = getattr(resp, "headers", {}) or {}
                cap = _Capture(
                    parts.path or "/", text,
                    lambda: (getattr(resp, "status", None) or getattr(resp, "code", 200),
                             headers.get("Content-Encoding") or headers.get("content-encoding"),
                             headers.get("Content-Type", "") or headers.get("content-type", ""), False),
                    parts.hostname or "", port,
                )
                orig_read = resp.read
                orig_readline = getattr(resp, "readline", None)
                orig_close = getattr(resp, "close", None)

                def read(amt=None):
                    chunk = orig_read() if amt is None else orig_read(amt)
                    if chunk:
                        cap.add(chunk)
                    if amt is None or not chunk:
                        cap.finish()
                    return chunk

                def readline(limit=-1):
                    line = orig_readline() if limit < 0 else orig_readline(limit)
                    if line:
                        cap.add(line)
                    else:
                        cap.finish()
                    return line

                def close():
                    try:
                        if orig_close:
                            orig_close()
                    finally:
                        cap.finish()

                resp.read = read
                if orig_readline:
                    resp.readline = readline
                if orig_close:
                    resp.close = close
        except Exception as exc:
            _warn("hook error: %r" % (exc,))
        return resp

    OpenerDirector.open = open_
    OpenerDirector._tokprof_patched = True


def _patch_aiohttp(aiohttp):
    ClientSession = aiohttp.ClientSession
    if getattr(ClientSession, "_tokprof_patched", False):
        return
    orig_request = ClientSession._request

    async def _request(self, method, str_or_url, *args, **kwargs):
        resp = await orig_request(self, method, str_or_url, *args, **kwargs)
        try:
            url = resp.url
            if kwargs.get("json") is not None:
                text = json.dumps(kwargs["json"])
            else:
                text = _to_text(kwargs.get("data"))
            if text is not None and _is_candidate(method, url.host, url.port, url.path, text):
                _tee_aiohttp(resp, url.path, text, url.host or "", url.port or (443 if url.scheme == "https" else 80))
        except Exception as exc:
            _warn("hook error: %r" % (exc,))
        return resp

    ClientSession._request = _request
    ClientSession._tokprof_patched = True


def _tee_aiohttp(resp, path, text, host="", port=0):
    cap = _Capture(
        path, text,
        # the parser already removed any content-encoding before feeding the stream
        lambda: (resp.status, None, resp.headers.get("Content-Type", ""), True),
        host, port,
    )
    stream = resp.content
    for chunk in list(getattr(stream, "_buffer", ())):
        cap.add(chunk)
    base = type(stream)

    # StreamReader may use __slots__, so instance attributes are read-only; an empty-slot
    # subclass with the same layout lets this one instance be tapped.
    class Tapped(base):
        __slots__ = ()

        def feed_data(self, data, *a, **k):
            try:
                cap.add(data)
            except Exception:
                pass
            return base.feed_data(self, data, *a, **k)

        def feed_eof(self):
            try:
                cap.finish()
            except Exception:
                pass
            return base.feed_eof(self)

    stream.__class__ = Tapped
    if stream.is_eof():
        cap.finish()


_PATCHERS = {
    "httpx": _patch_httpx,
    "requests": _patch_requests,
    "urllib3": _patch_urllib3,
    "aiohttp": _patch_aiohttp,
    "urllib.request": _patch_urllib_request,
}


class _Finder:
    """Patch each supported HTTP library right after it is first imported."""

    def __init__(self):
        self._busy = set()

    def find_spec(self, name, path=None, target=None):
        if name not in _PATCHERS or name in self._busy:
            return None
        import importlib.util

        self._busy.add(name)
        try:
            spec = importlib.util.find_spec(name)
        finally:
            self._busy.discard(name)
        if spec is None or spec.loader is None:
            return None
        loader = spec.loader
        orig_exec = loader.exec_module

        def exec_module(module):
            orig_exec(module)
            try:
                _PATCHERS[name](module)
            except Exception as exc:
                _warn("could not hook %s: %r" % (name, exc))

        loader.exec_module = exec_module
        return spec


def install():
    if not (_PORT and _TOKEN):
        return
    missing = False
    for name, patcher in _PATCHERS.items():
        if name in sys.modules:
            try:
                patcher(sys.modules[name])
            except Exception as exc:
                _warn("could not hook %s: %r" % (name, exc))
        else:
            missing = True
    if missing:
        sys.meta_path.insert(0, _Finder())
