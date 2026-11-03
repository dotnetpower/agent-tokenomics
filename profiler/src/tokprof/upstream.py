"""Upstream connection helpers: HTTP(S)_PROXY and NO_PROXY handling for the gateway's outbound side."""

from __future__ import annotations

import base64
import os
import socket
from typing import Optional
from urllib.parse import unquote, urlsplit

ALWAYS_DIRECT = {"localhost", "127.0.0.1", "::1"}


def proxy_for(host: str, env: Optional[dict] = None) -> Optional[tuple]:
    """Return (proxy_host, proxy_port, auth_header_or_None) for an https upstream, or None for a direct connection."""
    env = os.environ if env is None else env
    raw = env.get("HTTPS_PROXY") or env.get("https_proxy") or env.get("ALL_PROXY") or env.get("all_proxy")
    if not raw:
        return None
    if host in ALWAYS_DIRECT:
        return None
    no_proxy = env.get("NO_PROXY") or env.get("no_proxy") or ""
    for item in (x.strip().lower() for x in no_proxy.split(",") if x.strip()):
        item = item.lstrip("*")
        if item == "" or item == ".":
            return None
        if host.lower() == item.lstrip(".") or host.lower().endswith("." + item.lstrip(".")):
            return None
    if "://" not in raw:
        raw = "http://" + raw
    u = urlsplit(raw)
    if u.scheme not in ("http", ""):
        return None  # https:// or socks proxies are not supported yet: connect directly
    if not u.hostname:
        return None
    auth = None
    if u.username:
        token = f"{unquote(u.username)}:{unquote(u.password or '')}".encode()
        auth = "Basic " + base64.b64encode(token).decode()
    return u.hostname, u.port or 8080, auth


def connect_via_proxy(proxy_host: str, proxy_port: int, host: str, port: int, auth: Optional[str] = None, timeout: float = 15.0) -> socket.socket:
    """Open a CONNECT tunnel and return the connected plain socket (blocking; call from an executor)."""
    sock = socket.create_connection((proxy_host, proxy_port), timeout=timeout)
    try:
        lines = [f"CONNECT {host}:{port} HTTP/1.1", f"Host: {host}:{port}"]
        if auth:
            lines.append(f"Proxy-Authorization: {auth}")
        sock.sendall(("\r\n".join(lines) + "\r\n\r\n").encode("latin-1"))
        buf = b""
        while b"\r\n\r\n" not in buf:  # read byte by byte so no tunneled data is consumed
            chunk = sock.recv(1)
            if not chunk:
                raise ConnectionError("proxy closed the connection during CONNECT")
            buf += chunk
            if len(buf) > 65536:
                raise ConnectionError("proxy response too large")
        status_line = buf.split(b"\r\n", 1)[0].decode("latin-1")
        parts = status_line.split(" ", 2)
        if len(parts) < 2 or parts[1] != "200":
            raise ConnectionError(f"proxy refused CONNECT: {status_line}")
        sock.settimeout(None)
        sock.setblocking(False)
        return sock
    except BaseException:
        sock.close()
        raise
