import socket
import threading

import pytest

from tokprof.upstream import connect_via_proxy, proxy_for


def test_proxy_for_env_rules():
    env = {"HTTPS_PROXY": "http://user:p%40ss@proxy.local:3128", "NO_PROXY": "internal.example.com,.corp.net"}
    host, port, auth = proxy_for("api.openai.com", env)
    assert (host, port) == ("proxy.local", 3128) and auth.startswith("Basic ")
    assert proxy_for("internal.example.com", env) is None
    assert proxy_for("svc.corp.net", env) is None
    assert proxy_for("localhost", env) is None
    assert proxy_for("api.openai.com", {}) is None
    assert proxy_for("x.com", {"https_proxy": "proxy.local:8080"})[:2] == ("proxy.local", 8080)
    assert proxy_for("x.com", {"HTTPS_PROXY": "socks5://p:1080"}) is None  # unsupported scheme: go direct
    assert proxy_for("x.com", {"HTTPS_PROXY": "http://p:1", "NO_PROXY": "*"}) is None


def _fake_proxy(target_port, status="200 Connection established"):
    srv = socket.socket()
    srv.bind(("127.0.0.1", 0))
    srv.listen(5)
    seen = {}

    def serve():
        conn, _ = srv.accept()
        data = b""
        while b"\r\n\r\n" not in data:
            data += conn.recv(1)
        seen["request"] = data.decode()
        conn.sendall(f"HTTP/1.1 {status}\r\n\r\n".encode())
        if status.startswith("200"):
            up = socket.create_connection(("127.0.0.1", target_port))

            def pump(a, b):
                try:
                    while True:
                        d = a.recv(4096)
                        if not d:
                            break
                        b.sendall(d)
                except OSError:
                    pass
                finally:
                    try:
                        b.shutdown(socket.SHUT_WR)
                    except OSError:
                        pass

            t = threading.Thread(target=pump, args=(up, conn), daemon=True)
            t.start()
            pump(conn, up)
            t.join(2)
        conn.close()
        srv.close()

    threading.Thread(target=serve, daemon=True).start()
    return srv.getsockname()[1], seen


def test_connect_via_proxy_tunnels_bytes():
    from tokprof._mock_llm import MockProcess

    m = MockProcess()
    try:
        pport, seen = _fake_proxy(m.port)
        sock = connect_via_proxy("127.0.0.1", pport, "api.example.com", 443, auth="Basic abc")
        sock.setblocking(True)
        sock.sendall(b"GET / HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n")
        data = b""
        while True:
            chunk = sock.recv(4096)
            if not chunk:
                break
            data += chunk
        sock.close()
        assert data.startswith(b"HTTP/1.")
        assert "CONNECT api.example.com:443" in seen["request"] and "Proxy-Authorization: Basic abc" in seen["request"]
    finally:
        m.stop()


def test_connect_via_proxy_refused():
    pport, _ = _fake_proxy(1, status="403 Forbidden")
    with pytest.raises(ConnectionError):
        connect_via_proxy("127.0.0.1", pport, "x.example.com", 443)
