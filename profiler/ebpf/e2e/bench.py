#!/usr/bin/env python3
"""Rough overhead check for the eBPF tap: N keep-alive HTTPS POSTs with and without the helper attached.

    python3 e2e/bench.py --helper bin/tokprof-ebpf --sudo [--n 2000] [--kb 4]
"""
import argparse
import http.client
import json
import os
import shutil
import ssl
import statistics
import subprocess
import sys
import tempfile
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

CLIENT = r"""
import http.client, json, os, ssl, sys, time
port, n, kb, flag = int(sys.argv[1]), int(sys.argv[2]), int(sys.argv[3]), sys.argv[4]
ctx = ssl.create_default_context(); ctx.check_hostname = False; ctx.verify_mode = ssl.CERT_NONE
body = json.dumps({"model": "m", "messages": [{"role": "user", "content": "x" * (kb * 1024)}]})
c = http.client.HTTPSConnection("127.0.0.1", port, context=ctx)
c.connect(); c.sock.setsockopt(6, 1, 1)  # TCP_NODELAY: avoid the 40 ms Nagle/delayed-ACK stall hiding the overhead
while not os.path.exists(flag): time.sleep(0.05)
lat = []
for _ in range(n):
    t = time.perf_counter()
    c.request("POST", "/v1/chat/completions", body, {"Content-Type": "application/json"})
    c.getresponse().read()
    lat.append((time.perf_counter() - t) * 1000)
lat.sort()
print(json.dumps({"mean_ms": sum(lat) / n, "p50_ms": lat[n // 2], "p99_ms": lat[int(n * .99)]}))
"""


class H(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def setup(self):
        super().setup()
        self.request.setsockopt(6, 1, 1)

    def log_message(self, *a):
        pass

    def do_POST(self):
        self.rfile.read(int(self.headers["Content-Length"]))
        d = b'{"usage":{"prompt_tokens":1,"completion_tokens":1}}'
        self.send_response(200)
        self.send_header("Content-Length", str(len(d)))
        self.end_headers()
        self.wfile.write(d)


def run_client(port, n, kb, tmp, helper, sudo):
    flag = os.path.join(tmp, "go%d" % time.time_ns())
    cp = os.path.join(tmp, "client.py")
    open(cp, "w").write(CLIENT)
    p = subprocess.Popen([sys.executable if False else "/usr/bin/python3", cp, str(port), str(n), str(kb), flag],
                         stdout=subprocess.PIPE, text=True)
    h = None
    if helper:
        cmd = (["sudo", "-n"] if sudo else []) + [helper, "trace", "--pid", str(p.pid), "--exit-on-stdin-eof"]
        h = subprocess.Popen(cmd, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        for line in h.stderr:
            if line.strip().endswith("ready"):
                break
        threading.Thread(target=lambda: [None for _ in h.stderr], daemon=True).start()
        n_out = []
        threading.Thread(target=lambda: n_out.extend(h.stdout), daemon=True).start()
        time.sleep(2.5)
    open(flag, "w").close()
    res = json.loads(p.communicate()[0])
    if h:
        time.sleep(1.0)
        h.stdin.close()
        h.wait(timeout=30)
        res["captured"] = len(n_out)
    return res


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--helper", required=True)
    ap.add_argument("--sudo", action="store_true")
    ap.add_argument("--n", type=int, default=2000)
    ap.add_argument("--kb", type=int, default=4)
    ap.add_argument("--rounds", type=int, default=3)
    a = ap.parse_args()
    tmp = tempfile.mkdtemp(prefix="tokprof-bench-")
    try:
        key, crt = os.path.join(tmp, "k.pem"), os.path.join(tmp, "c.pem")
        subprocess.run(["openssl", "req", "-x509", "-newkey", "rsa:2048", "-nodes", "-keyout", key, "-out", crt,
                        "-days", "1", "-subj", "/CN=localhost"], check=True, capture_output=True)
        ctx = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
        ctx.load_cert_chain(crt, key)
        srv = ThreadingHTTPServer(("127.0.0.1", 0), H)
        srv.socket = ctx.wrap_socket(srv.socket, server_side=True)
        threading.Thread(target=srv.serve_forever, daemon=True).start()
        port = srv.server_address[1]
        base, tap = [], []
        for _ in range(a.rounds):  # interleave so drift affects both equally
            base.append(run_client(port, a.n, a.kb, tmp, None, a.sudo))
            tap.append(run_client(port, a.n, a.kb, tmp, os.path.abspath(a.helper), a.sudo))
        for name, rows in (("without tap", base), ("with tap", tap)):
            print(f"{name:12} mean={statistics.mean(r['mean_ms'] for r in rows):.3f} ms  "
                  f"p50={statistics.mean(r['p50_ms'] for r in rows):.3f} ms  p99={statistics.mean(r['p99_ms'] for r in rows):.3f} ms"
                  + (f"  captured={[r['captured'] for r in rows]}" if name == "with tap" else ""))
        print(f"requests per round={a.n}, body={a.kb} KiB, rounds={a.rounds}")
    finally:
        shutil.rmtree(tmp, ignore_errors=True)


if __name__ == "__main__":
    main()
