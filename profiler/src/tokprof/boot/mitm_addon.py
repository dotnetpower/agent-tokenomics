"""mitmproxy addon that copies LLM exchanges to a running tokprof gateway (standard library only).

Used by `tokprof mitm`; it runs inside mitmproxy's own Python environment, so it must not import tokprof.
Options: tokprof_port, tokprof_token. Responses are streamed through untouched (a tee callback), so streaming
clients keep their latency. Only the request path, the request body and the response body are forwarded;
no headers leave this process.
"""

import gzip
import http.client
import json
import re
import threading
import time
import zlib

# Pre-filter only; tokprof's detector decides. Keep identical to tokprof.detect.plausible (tests/vectors/prefilter.json).
_EXCLUDED_PATH = re.compile(
    r"/(embeddings?|moderations?|rerank|tokenize|detokenize|count_tokens|"
    r"audio/(speech|transcriptions|translations)|images?/(generations|edits|variations)|"
    r"files|batches|fine_tuning|uploads|vector_stores|assistants|threads|realtime)(/|$)"
    r"|:(embedContent|batchEmbedContents|countTokens)$",
    re.I,
)
_PLAUSIBLE_KEYS = re.compile(r'"(?:messages|contents)"\s*:\s*\[|"(?:prompt|input|inputs)"\s*:')
_CAP = 32 * 1024 * 1024


def _decode(data, encoding):
    enc = (encoding or "").lower()
    if "gzip" in enc:
        return gzip.decompress(data)
    if "deflate" in enc:
        try:
            return zlib.decompress(data)
        except zlib.error:
            return zlib.decompress(data, -zlib.MAX_WBITS)
    if enc and enc != "identity":
        raise ValueError("unsupported content-encoding " + enc)
    return data


def _plausible(method, path, text):
    if (method or "").upper() != "POST":
        return False
    head = (text or "").lstrip()
    if len(head) < 8 or head[0] != "{":
        return False
    p = path.split("?", 1)[0].split("#", 1)[0].rstrip("/") or "/"
    if _EXCLUDED_PATH.search(p):
        return False
    return _PLAUSIBLE_KEYS.search(text) is not None


class TokprofAddon:
    def __init__(self):
        self.port = 0
        self.token = ""
        self._warned = False

    def load(self, loader):
        loader.add_option("tokprof_port", int, 0, "Port of the tokprof gateway ingest endpoint")
        loader.add_option("tokprof_token", str, "", "Ingest token of the tokprof gateway")

    def configure(self, updated):
        from mitmproxy import ctx

        self.port, self.token = int(ctx.options.tokprof_port or 0), ctx.options.tokprof_token or ""

    def _warn(self, msg):
        if not self._warned:
            self._warned = True
            print("tokprof(mitm): " + msg, flush=True)

    def _wanted(self, flow):
        req = flow.request
        if req.method != "POST" or not self.port:
            return False
        if req.host in ("127.0.0.1", "localhost") and req.port == self.port:
            return False
        return _plausible(req.method, req.path, (req.content or b"").decode("utf-8", "replace"))

    def responseheaders(self, flow):
        if not self._wanted(flow):
            return
        chunks, size, t0 = [], [0], time.time()
        first = [None]
        resp = flow.response

        def tee(data):
            if data:
                if first[0] is None:
                    first[0] = int((time.time() - t0) * 1000)
                if size[0] < _CAP:
                    chunks.append(bytes(data))
                    size[0] += len(data)
            else:
                self._ship(flow, b"".join(chunks), t0, first[0])
            return data

        resp.stream = tee

    def response(self, flow):
        # Not streamed (for example the stream callback was never installed): ship the buffered body.
        if flow.response is not None and not callable(flow.response.stream) and self._wanted(flow):
            self._ship(flow, flow.response.raw_content or b"", flow.request.timestamp_start, None)

    def _ship(self, flow, body, t0, ttfb):
        try:
            resp, req = flow.response, flow.request
            ctype = resp.headers.get("content-type", "")
            payload = {
                "path": req.path.split("?", 1)[0],
                "host": req.host,
                "port": req.port,
                "request": (req.content or b"").decode("utf-8", "replace"),
                "status": resp.status_code,
                "response": _decode(body, resp.headers.get("content-encoding")).decode("utf-8", "replace"),
                "sse": "text/event-stream" in ctype,
                "ts": t0,
                "ttfb_ms": ttfb,
                "latency_ms": int((time.time() - t0) * 1000),
            }
            threading.Thread(target=self._post, args=(payload,), daemon=True).start()
        except Exception as exc:
            self._warn("could not capture a call: %r" % (exc,))

    def _post(self, payload):
        try:
            conn = http.client.HTTPConnection("127.0.0.1", self.port, timeout=5)
            conn.request("POST", "/__tokprof/ingest", body=json.dumps(payload).encode("utf-8"),
                         headers={"Content-Type": "application/json", "X-Tokprof-Token": self.token})
            conn.getresponse().read()
            conn.close()
        except Exception as exc:
            self._warn("could not deliver a captured call: %r" % (exc,))


addons = [TokprofAddon()]
