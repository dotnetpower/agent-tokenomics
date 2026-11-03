"""Local pass-through gateway.

Clients talk plain HTTP to 127.0.0.1; the gateway forwards to the real upstream (TLS if https) and records
token statistics. It never alters bodies. Failure in the profiling code path is swallowed so the
client still gets its response (fail-open); upstream failures surface as 502 like any real outage.
"""

from __future__ import annotations

import asyncio
import json
import ssl
import sys
import threading
import time
from dataclasses import dataclass
from typing import Callable, Optional
from urllib.parse import urlsplit

from .counting import get_counter
from .parse import SSEParser, detect_provider, parse_request, parse_response_json
from .segment import build_record
from .upstream import connect_via_proxy, proxy_for

HOP_BY_HOP = {
    "connection", "proxy-connection", "keep-alive", "transfer-encoding", "te", "trailer", "upgrade",
}
MAX_HEAD = 64 * 1024
MAX_BODY = 64 * 1024 * 1024
DEFAULT_AZURE_SUFFIXES = (".openai.azure.com", ".cognitiveservices.azure.com", ".services.ai.azure.com")


@dataclass
class Route:
    prefix: str  # path prefix the client uses, e.g. /openai/v1
    scheme: str
    host: str
    port: int
    base_path: str  # path prefix on the upstream, e.g. /v1

    @classmethod
    def from_urls(cls, prefix: str, upstream: str) -> "Route":
        u = urlsplit(upstream)
        scheme = u.scheme or "https"
        port = u.port or (443 if scheme == "https" else 80)
        return cls(prefix=prefix.rstrip("/"), scheme=scheme, host=u.hostname or "", port=port, base_path=u.path.rstrip("/"))


class BodyFramer:
    """Tracks HTTP/1.1 body framing so we know when a response ends and can decode chunked data."""

    def __init__(self, headers: dict, no_body: bool):
        te = headers.get("transfer-encoding", "").lower()
        self.chunked = "chunked" in te
        cl = headers.get("content-length")
        self.remaining = int(cl) if (cl and cl.isdigit() and not self.chunked) else None
        self.done = no_body or (self.remaining == 0)
        self._buf = b""
        self._chunk_left = 0
        self._state = "size"

    def feed(self, data: bytes) -> bytes:
        if self.done:
            return b""
        if self.chunked:
            return self._feed_chunked(data)
        if self.remaining is not None:
            take = data[: self.remaining]
            self.remaining -= len(take)
            if self.remaining == 0:
                self.done = True
            return take
        return data

    def _feed_chunked(self, data: bytes) -> bytes:
        self._buf += data
        out = b""
        while True:
            if self._state == "size":
                i = self._buf.find(b"\r\n")
                if i < 0:
                    break
                line = self._buf[:i].split(b";", 1)[0].strip()
                self._buf = self._buf[i + 2:]
                try:
                    size = int(line, 16)
                except ValueError:
                    self.done = True
                    break
                if size == 0:
                    self._state = "trailer"
                else:
                    self._chunk_left, self._state = size, "data"
            elif self._state == "data":
                if not self._buf:
                    break
                take = self._buf[: self._chunk_left]
                out += take
                self._buf = self._buf[len(take):]
                self._chunk_left -= len(take)
                if self._chunk_left == 0:
                    self._state = "crlf"
            elif self._state == "crlf":
                if len(self._buf) < 2:
                    break
                self._buf, self._state = self._buf[2:], "size"
            elif self._state == "trailer":
                i = self._buf.find(b"\r\n")
                if i < 0:
                    break
                line, self._buf = self._buf[:i], self._buf[i + 2:]
                if line == b"":
                    self.done = True
                    break
        return out


def _parse_head(raw: bytes):
    text = raw.decode("latin-1")
    lines = text.split("\r\n")
    first = lines[0]
    headers: list = []
    for ln in lines[1:]:
        if not ln:
            continue
        k, _, v = ln.partition(":")
        headers.append((k.strip(), v.strip()))
    return first, headers


def _hdict(headers: list) -> dict:
    d: dict = {}
    for k, v in headers:
        d[k.lower()] = v
    return d


async def _read_head(reader: asyncio.StreamReader, limit: int = MAX_HEAD):
    """Read until the blank line that ends an HTTP head; return (head, leftover body bytes).

    Implemented with plain reads because StreamReader.readuntil() occasionally missed a wake-up on
    Python 3.9 (response data arrived, the waiter never resumed).
    """
    buf = bytearray()
    while True:
        chunk = await reader.read(65536)
        if not chunk:
            raise asyncio.IncompleteReadError(bytes(buf), None)
        buf += chunk
        i = buf.find(b"\r\n\r\n")
        if i >= 0:
            return bytes(buf[:i]), bytes(buf[i + 4:])
        if len(buf) > limit:
            raise asyncio.LimitOverrunError("HTTP head too large", len(buf))


async def _read_request_body(reader: asyncio.StreamReader, hd: dict, prefix: bytes = b"") -> bytes:
    if "chunked" in hd.get("transfer-encoding", "").lower():
        framer, out = BodyFramer(hd, False), b""
        data = prefix
        while True:
            if data:
                out += framer.feed(data)
                if len(out) > MAX_BODY:
                    raise ValueError("request body too large")
            if framer.done:
                return out
            data = await reader.read(65536)
            if not data:
                return out
    n = int(hd.get("content-length", "0") or 0)
    if n > MAX_BODY:
        raise ValueError("request body too large")
    if len(prefix) >= n:
        return prefix[:n]
    return prefix + await reader.readexactly(n - len(prefix))


class Gateway:
    def __init__(
        self,
        routes: list,
        sink: Callable[[dict], None],
        *,
        salt: bytes,
        components: Optional[dict] = None,
        run_id: Optional[str] = None,
        azure_hosts: Optional[set] = None,
        azure_suffixes: tuple = DEFAULT_AZURE_SUFFIXES,
        host: str = "127.0.0.1",
        port: int = 0,
        use_tiktoken: bool = True,
        ingest_token: Optional[str] = None,
    ):
        self.routes = sorted(routes, key=lambda r: -len(r.prefix))
        self.sink = sink
        self.salt = salt
        self.components = components or {}
        self.run_id = run_id
        self.azure_hosts = azure_hosts or set()
        self.azure_suffixes = azure_suffixes
        self.host, self.port = host, port
        self.use_tiktoken = use_tiktoken
        self.ingest_token = ingest_token
        self.stats = {"requests": 0, "recorded": 0, "record_errors": 0, "bypassed": 0}
        self._active = 0
        self._last_request: Optional[float] = None
        # The loop keeps only weak references to tasks created by start_server on Python <= 3.11, so a
        # handler suspended mid-request can be garbage-collected and the request silently dropped.
        self._tasks: set = set()
        self._server: Optional[asyncio.AbstractServer] = None
        self._warned: set = set()
        self._ctx = ssl.create_default_context()

    # ---- lifecycle -------------------------------------------------
    async def start(self) -> int:
        self._server = await asyncio.start_server(self._handle, self.host, self.port, limit=2 ** 20)
        self.port = self._server.sockets[0].getsockname()[1]
        return self.port

    async def stop(self) -> None:
        if self._server:
            self._server.close()
            await self._server.wait_closed()

    async def _open_upstream(self, scheme: str, host: str, port: int):
        if scheme != "https":
            return await asyncio.open_connection(host, port)
        proxy = proxy_for(host)
        if proxy:
            loop = asyncio.get_running_loop()
            sock = await loop.run_in_executor(None, connect_via_proxy, proxy[0], proxy[1], host, port, proxy[2])
            return await asyncio.open_connection(sock=sock, ssl=self._ctx, server_hostname=host)
        return await asyncio.open_connection(host, port, ssl=self._ctx, server_hostname=host)

    # ---- routing ---------------------------------------------------
    def _resolve(self, path: str):
        """Return (scheme, host, port, upstream_path) or None."""
        bare, _, query = path.partition("?")
        if bare.startswith("/azure/"):
            rest = bare[len("/azure/"):]
            host, _, tail = rest.partition("/")
            if host in self.azure_hosts or host.endswith(self.azure_suffixes):
                p = "/" + tail
                return "https", host, 443, p + ("?" + query if query else "")
            return None
        for r in self.routes:
            if bare == r.prefix or bare.startswith(r.prefix + "/"):
                p = r.base_path + bare[len(r.prefix):]
                return r.scheme, r.host, r.port, (p or "/") + ("?" + query if query else "")
        return None

    def _warn_once(self, key: str, msg: str) -> None:
        if key not in self._warned:
            self._warned.add(key)
            print(f"tokprof: {msg}", file=sys.stderr)

    # ---- connection handling --------------------------------------
    async def drain(self, timeout: float = 5.0) -> None:
        """Wait for in-flight requests (including their recording) to finish."""
        deadline = time.time() + timeout
        while self._active > 0 and time.time() < deadline:
            await asyncio.sleep(0.02)

    async def _handle(self, reader: asyncio.StreamReader, writer: asyncio.StreamWriter) -> None:
        self._active += 1
        task = asyncio.current_task()
        if task is not None:
            self._tasks.add(task)
        try:
            await self._serve(reader, writer)
        except (ConnectionError, asyncio.IncompleteReadError, asyncio.LimitOverrunError) as exc:
            self.stats["dropped"] = self.stats.get("dropped", 0) + 1
            self._warn_once("drop:" + type(exc).__name__, f"client connection ended early ({type(exc).__name__})")
        except Exception as exc:  # never let one connection take the gateway down
            self._warn_once("conn:" + type(exc).__name__, f"connection error: {exc!r}")
        finally:
            self._active -= 1
            if task is not None:
                self._tasks.discard(task)
            try:
                writer.close()
            except Exception:
                pass

    async def _respond(self, writer, status: int, reason: str, body: bytes, ctype: str = "text/plain") -> None:
        head = (
            f"HTTP/1.1 {status} {reason}\r\nContent-Type: {ctype}\r\n"
            f"Content-Length: {len(body)}\r\nConnection: close\r\n\r\n"
        ).encode("latin-1")
        writer.write(head + body)
        await writer.drain()

    async def _serve(self, reader: asyncio.StreamReader, writer: asyncio.StreamWriter) -> None:
        raw, leftover = await _read_head(reader)
        first, headers = _parse_head(raw)
        method, target, _ = (first.split(" ") + ["", "", ""])[:3]
        hd = _hdict(headers)
        self.stats["requests"] += 1

        if target.startswith("/__tokprof/health"):
            age = None if self._last_request is None else time.time() - self._last_request
            return await self._respond(writer, 200, "OK", json.dumps({"ok": True, "last_request_age_s": age, **self.stats}).encode(), "application/json")
        self._last_request = time.time()
        if target.startswith("/__tokprof/ingest") and method == "POST":
            body = await _read_request_body(reader, hd, leftover)
            return await self._ingest(writer, hd, body)
        if hd.get("expect", "").lower() == "100-continue":
            writer.write(b"HTTP/1.1 100 Continue\r\n\r\n")
            await writer.drain()
        body = await _read_request_body(reader, hd, leftover)

        route = self._resolve(target)
        if route is None:
            self.stats["bypassed"] += 1
            return await self._respond(writer, 404, "Not Found", b"tokprof: no upstream route for this path")
        scheme, host, port, upath = route

        provider, parsed = None, None
        if method == "POST":
            try:
                obj = json.loads(body) if body else None
                provider = detect_provider(target, obj)
                if provider and isinstance(obj, dict):
                    parsed = parse_request(provider, obj)
            except Exception:
                parsed = None

        t0 = time.time()
        try:
            up_reader, up_writer = await self._open_upstream(scheme, host, port)
        except Exception as exc:
            return await self._respond(writer, 502, "Bad Gateway", f"tokprof: upstream connect failed: {exc}".encode())

        fwd = [f"{method} {upath} HTTP/1.1", f"Host: {host}" + (f":{port}" if port not in (80, 443) else "")]
        skip = HOP_BY_HOP | {"host", "content-length", "accept-encoding", "expect"}
        for k, v in headers:
            if k.lower() not in skip:
                fwd.append(f"{k}: {v}")
        fwd += ["Accept-Encoding: identity", "Connection: close"]
        if body or method in ("POST", "PUT", "PATCH"):
            fwd.append(f"Content-Length: {len(body)}")
        up_writer.write(("\r\n".join(fwd) + "\r\n\r\n").encode("latin-1") + body)
        await up_writer.drain()

        status = None
        ttfb = None
        head_sent = False
        collected = bytearray()
        sse: Optional[SSEParser] = None
        err: Optional[str] = None
        try:
            raw_head, pending = await _read_head(up_reader)
            ttfb = int((time.time() - t0) * 1000)
            sline, rheaders = _parse_head(raw_head)
            try:
                status = int(sline.split(" ", 2)[1])
            except (IndexError, ValueError):
                status = None
            rhd = _hdict(rheaders)
            out = [sline]
            for k, v in rheaders:
                if k.lower() not in ("connection", "proxy-connection", "keep-alive"):
                    out.append(f"{k}: {v}")
            out.append("Connection: close")
            writer.write(("\r\n".join(out) + "\r\n\r\n").encode("latin-1"))
            await writer.drain()
            head_sent = True

            is_sse = "text/event-stream" in rhd.get("content-type", "")
            if parsed is not None and is_sse:
                sse = SSEParser(parsed.provider)
            no_body = method == "HEAD" or status in (204, 304) or (status is not None and 100 <= status < 200)
            framer = BodyFramer(rhd, no_body)
            while not framer.done:
                data, pending = pending, b""
                if not data:
                    data = await up_reader.read(65536)
                if not data:
                    break
                writer.write(data)
                await writer.drain()
                if parsed is not None:
                    try:
                        decoded = framer.feed(data)
                        if sse is not None:
                            sse.feed(decoded)
                        elif len(collected) < MAX_BODY:
                            collected += decoded
                    except Exception:
                        parsed = None
                else:
                    framer.feed(data)
        except (ConnectionError, asyncio.IncompleteReadError) as exc:
            err = f"stream_interrupted:{type(exc).__name__}"
            if not head_sent:
                # Nothing reached the client yet: fail visibly, like a real upstream outage would.
                try:
                    await self._respond(writer, 502, "Bad Gateway", f"tokprof: upstream closed the connection: {type(exc).__name__}".encode())
                except Exception:
                    pass
        finally:
            try:
                up_writer.close()
            except Exception:
                pass

        # The client already has its full response: release it before the CPU-bound token counting,
        # and count off the event loop so large prompts never stall other requests.
        try:
            writer.close()
        except Exception:
            pass
        if parsed is not None:
            try:
                response = sse.finish() if sse is not None else parse_response_json(parsed.provider, json.loads(bytes(collected)) if collected else None)
                await asyncio.get_running_loop().run_in_executor(None, self._record, parsed, response, target, status, t0, ttfb, err)
            except Exception as exc:
                self.stats["record_errors"] += 1
                self._warn_once("rec:" + type(exc).__name__, f"could not record a call ({exc!r}); traffic is unaffected")

    async def _ingest(self, writer, hd: dict, body: bytes) -> None:
        """Accept a raw exchange captured by an in-process hook and record it like proxied traffic."""
        if self.ingest_token is None or hd.get("x-tokprof-token") != self.ingest_token:
            return await self._respond(writer, 403, "Forbidden", b"tokprof: bad ingest token")
        try:
            msg = json.loads(body)
            path = str(msg.get("path") or "")
            req_obj = json.loads(msg.get("request") or "null")
            provider = detect_provider(path, req_obj)
            if provider and isinstance(req_obj, dict):
                parsed = parse_request(provider, req_obj)
                text = msg.get("response") or ""
                if msg.get("sse"):
                    sse = SSEParser(provider)
                    sse.feed(text.encode("utf-8", "replace"))
                    response = sse.finish()
                else:
                    try:
                        response = parse_response_json(provider, json.loads(text) if text else None)
                    except ValueError:
                        response = parse_response_json(provider, None)
                t0 = float(msg.get("ts") or time.time())
                await asyncio.get_running_loop().run_in_executor(
                    None, self._record, parsed, response, path, msg.get("status"), t0, msg.get("ttfb_ms"), msg.get("error")
                )
        except Exception as exc:
            self.stats["record_errors"] += 1
            self._warn_once("ingest:" + type(exc).__name__, f"could not record an ingested call ({exc!r})")
        await self._respond(writer, 204, "No Content", b"")

    def _record(self, parsed, response, target, status, t0, ttfb, err) -> None:
        counter = get_counter(parsed.model, prefer_tiktoken=self.use_tiktoken)
        rec = build_record(
            parsed=parsed,
            response=response,
            counter=counter,
            salt=self.salt,
            components=self.components,
            endpoint=target.split("?", 1)[0],
            status=status,
            latency_ms=int((time.time() - t0) * 1000),
            ttfb_ms=ttfb,
            run=self.run_id,
            error=err,
            ts=t0,
        )
        self.sink(rec.to_dict())
        self.stats["recorded"] += 1


class GatewayThread:
    """Runs a Gateway on its own event loop thread."""

    def __init__(self, gateway: Gateway):
        self.gateway = gateway
        self._loop: Optional[asyncio.AbstractEventLoop] = None
        self._thread: Optional[threading.Thread] = None
        self._ready = threading.Event()
        self._startup_error: Optional[BaseException] = None

    def start(self) -> int:
        def run() -> None:
            loop = asyncio.new_event_loop()
            self._loop = loop
            asyncio.set_event_loop(loop)
            try:
                loop.run_until_complete(self.gateway.start())
            except BaseException as exc:
                self._startup_error = exc
                self._ready.set()
                return
            self._ready.set()
            loop.run_forever()
            loop.run_until_complete(self.gateway.stop())
            loop.close()

        self._thread = threading.Thread(target=run, name="tokprof-gateway", daemon=True)
        self._thread.start()
        self._ready.wait(10)
        if self._startup_error:
            raise RuntimeError(f"gateway failed to start: {self._startup_error}")
        return self.gateway.port

    def stop(self) -> None:
        if self._loop and self._thread:
            try:
                asyncio.run_coroutine_threadsafe(self.gateway.drain(), self._loop).result(10)
            except Exception:
                pass
            self._loop.call_soon_threadsafe(self._loop.stop)
            self._thread.join(5)
