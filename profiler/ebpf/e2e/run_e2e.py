#!/usr/bin/env python3
"""End-to-end check of `tokprof-ebpf trace` against real TLS clients.

Needs Linux, root (via passwordless sudo for the helper binary) and: openssl, curl, node.
It starts a local HTTPS server with a throwaway certificate, starts several clients
that wait for a signal, attaches the helper to them, releases them and compares the
captured exchanges with what was sent. Nothing leaves this machine.

    python3 e2e/run_e2e.py --helper bin/tokprof-ebpf [--sudo]
"""
import argparse
import json
import os
import shutil
import ssl
import subprocess
import sys
import tempfile
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

REQ = {"model": "e2e-model", "messages": [{"role": "system", "content": "be brief"}, {"role": "user", "content": "ping " * 50}]}
REQ_STREAM = dict(REQ, stream=True)
REQ_LARGE = {"model": "e2e-model", "messages": [{"role": "user", "content": "lorem ipsum " * 12000}]}  # about 144 KB, several capture chunks
RESP = {"id": "r1", "usage": {"prompt_tokens": 77, "completion_tokens": 3, "total_tokens": 80}, "choices": []}
SSE = ('data: {"choices":[{"delta":{"content":"hi"}}]}\n\n'
       'data: {"choices":[],"usage":{"prompt_tokens":77,"completion_tokens":3,"total_tokens":80}}\n\n'
       "data: [DONE]\n\n")


class Handler(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def log_message(self, *a):
        pass

    def do_POST(self):
        body = self.rfile.read(int(self.headers.get("Content-Length", 0)))
        if json.loads(body).get("stream"):
            data = SSE.encode()
            self.send_response(200)
            self.send_header("Content-Type", "text/event-stream")
            self.send_header("Transfer-Encoding", "chunked")
            self.end_headers()
            for part in (data[:40], data[40:]):
                self.wfile.write(b"%x\r\n%s\r\n" % (len(part), part))
            self.wfile.write(b"0\r\n\r\n")
        else:
            data = json.dumps(RESP).encode()
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(data)))
            self.end_headers()
            self.wfile.write(data)


PY_CLIENT = r"""
import json, os, ssl, sys, time, urllib.request
flag, url, payload = sys.argv[1:4]
if payload.startswith('@'): payload = open(payload[1:]).read()
ctx = ssl.create_default_context(); ctx.check_hostname = False; ctx.verify_mode = ssl.CERT_NONE
while not os.path.exists(flag): time.sleep(0.05)
req = urllib.request.Request(url, data=payload.encode(), headers={"Content-Type": "application/json", "Authorization": "Bearer SECRET-TOKEN"})
print(len(urllib.request.urlopen(req, context=ctx).read()))
time.sleep(0.5)
"""

NODE_CLIENT = r"""
const fs = require('fs');
let [flag, url, payload] = process.argv.slice(2);
if (payload.startsWith('@')) payload = fs.readFileSync(payload.slice(1), 'utf8');
const t = setInterval(async () => {
  if (!fs.existsSync(flag)) return;
  clearInterval(t);
  const r = await fetch(url, {method: 'POST', body: payload, headers: {'content-type': 'application/json', authorization: 'Bearer SECRET-TOKEN'}});
  console.log((await r.text()).length);
  setTimeout(() => process.exit(0), 500);
}, 50);
"""


def check_tokprof(path, expected, err_lines):
    rows = [json.loads(x) for x in open(path, encoding="utf-8") if x.strip()] if os.path.exists(path) else []
    sample = json.dumps(rows[0])[:400] if rows else "-"
    leaked = "SECRET-TOKEN" in open(path, encoding="utf-8").read() if rows else False
    ok = len(rows) == expected and not leaked
    print(f"{'PASS' if ok else 'FAIL'}  tokprof ebpf wrote {len(rows)} record(s), expected {expected}{' LEAK' if leaked else ''}")
    print("sample:", sample)
    print("\n".join(err_lines))
    return 0 if ok else 1


H2_SERVER = r"""
const http2 = require('http2'), fs = require('fs');
const [port, key, crt] = process.argv.slice(2);
const RESP = JSON.stringify({id: 'r1', usage: {prompt_tokens: 77, completion_tokens: 3, total_tokens: 80}, choices: []});
const SSE1 = 'data: {"choices":[{"delta":{"content":"hi"}}]}\n\n';
const SSE2 = 'data: {"choices":[],"usage":{"prompt_tokens":77,"completion_tokens":3,"total_tokens":80}}\n\ndata: [DONE]\n\n';
http2.createSecureServer({key: fs.readFileSync(key), cert: fs.readFileSync(crt)}).on('stream', (st) => {
  let b = ''; st.on('data', (d) => b += d);
  st.on('end', () => {
    if (JSON.parse(b).stream) { st.respond({':status': 200, 'content-type': 'text/event-stream'}); st.write(SSE1); setTimeout(() => st.end(SSE2), 20); }
    else { st.respond({':status': 200, 'content-type': 'application/json'}); st.end(RESP); }
  });
}).listen(+port, '127.0.0.1');
"""

H2_CLIENT = r"""
const http2 = require('http2'), fs = require('fs');
let [flag, url, payload] = process.argv.slice(2);
if (payload.startsWith('@')) payload = fs.readFileSync(payload.slice(1), 'utf8');
function go() {
  const u = new URL(url); let failed = false;
  const retry = () => { if (!failed) { failed = true; setTimeout(go, 300); } };
  const c = http2.connect(u.origin, {rejectUnauthorized: false});
  c.on('error', retry);
  const req = c.request({':method': 'POST', ':path': u.pathname, 'content-type': 'application/json', authorization: 'Bearer SECRET-TOKEN'});
  req.on('error', retry);
  let d = ''; req.on('data', (x) => d += x);
  req.on('end', () => { console.log(d.length); c.close(); setTimeout(() => process.exit(0), 500); });
  req.end(payload);
}
const t = setInterval(() => { if (fs.existsSync(flag)) { clearInterval(t); go(); } }, 50);
"""


def free_port():
    import socket
    s = socket.socket()
    s.bind(("127.0.0.1", 0))
    port = s.getsockname()[1]
    s.close()
    return port


GO_CLIENT = r"""
package main

import (
	"bytes"
	"fmt"
	"io"
	"net/http"
	"os"
	"time"
)

func main() {
	flag, url, body := os.Args[1], os.Args[2], os.Args[3]
	for {
		if _, err := os.Stat(flag); err == nil {
			break
		}
		time.Sleep(50 * time.Millisecond)
	}
	req, _ := http.NewRequest("POST", url, bytes.NewReader([]byte(body)))
	req.Header.Set("Content-Type", "application/json")
	req.Header.Set("Authorization", "Bearer SECRET-TOKEN")
	resp, err := http.DefaultClient.Do(req)
	if err != nil {
		fmt.Println("error", err)
		os.Exit(1)
	}
	b, _ := io.ReadAll(resp.Body)
	fmt.Println(len(b))
	time.Sleep(500 * time.Millisecond)
}
"""


def build_go_client(tmp):
    """A Go client (raw write/read syscalls, no libc) when a Go toolchain is available; '' otherwise."""
    go = shutil.which("go") or os.path.expanduser("~/sdk/go/bin/go")
    if not os.path.exists(go):
        return ""
    src = os.path.join(tmp, "goclient")
    os.makedirs(src, exist_ok=True)
    open(os.path.join(src, "main.go"), "w").write(GO_CLIENT)
    open(os.path.join(src, "go.mod"), "w").write("module goclient\n\ngo 1.20\n")
    out = os.path.join(tmp, "goclient.bin")
    r = subprocess.run([go, "build", "-o", out, "."], cwd=src, capture_output=True, text=True)
    return out if r.returncode == 0 else ""


def find_uv_python():
    base = os.path.expanduser("~/.local/share/uv/python")
    if not os.path.isdir(base):
        return None
    for d in sorted(os.listdir(base), reverse=True):
        exe = os.path.join(base, d, "bin", "python3")
        if os.path.exists(exe) and d.count(".") >= 2:
            return exe
    return None


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--helper", required=True)
    ap.add_argument("--sudo", action="store_true", help="run the helper through `sudo -n`")
    ap.add_argument("--tokprof", action="store_true", help="drive the helper through `tokprof ebpf` and check the JSONL it writes")
    ap.add_argument("--keep", action="store_true", help="keep the temp dir")
    args = ap.parse_args()
    helper = os.path.abspath(args.helper)

    tmp = tempfile.mkdtemp(prefix="tokprof-e2e-")
    go_client = build_go_client(tmp)
    os.chmod(tmp, 0o700)
    try:
        key, crt = os.path.join(tmp, "k.pem"), os.path.join(tmp, "c.pem")
        subprocess.run(["openssl", "req", "-x509", "-newkey", "rsa:2048", "-nodes", "-keyout", key, "-out", crt,
                        "-days", "1", "-subj", "/CN=localhost"], check=True, capture_output=True)
        ctx = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
        ctx.load_cert_chain(crt, key)
        srv = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        srv.socket = ctx.wrap_socket(srv.socket, server_side=True)
        threading.Thread(target=srv.serve_forever, daemon=True).start()
        url = f"https://127.0.0.1:{srv.server_address[1]}/v1/chat/completions"

        plain_srv = ThreadingHTTPServer(("127.0.0.1", 0), Handler)  # cleartext HTTP, like an on-prem vLLM
        threading.Thread(target=plain_srv.serve_forever, daemon=True).start()
        purl = f"http://127.0.0.1:{plain_srv.server_address[1]}/v1/chat/completions"

        flag = os.path.join(tmp, "go")
        pyc, nodec = os.path.join(tmp, "c.py"), os.path.join(tmp, "c.js")
        open(pyc, "w").write(PY_CLIENT)
        open(nodec, "w").write(NODE_CLIENT)
        env = dict(os.environ, NODE_TLS_REJECT_UNAUTHORIZED="0")
        body, stream = json.dumps(REQ), json.dumps(REQ_STREAM)

        clients = {}  # name -> (Popen, kind)
        clients["system-python"] = subprocess.Popen(["/usr/bin/python3", pyc, flag, url, body], stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True)
        clients["system-python-sse"] = subprocess.Popen(["/usr/bin/python3", pyc, flag, url, stream], stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True)
        uv = find_uv_python()
        if uv:
            clients["uv-python"] = subprocess.Popen([uv, pyc, flag, url, body], stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True)
        large = os.path.join(tmp, "large.json")
        open(large, "w").write(json.dumps(REQ_LARGE))
        clients["system-python-large"] = subprocess.Popen(["/usr/bin/python3", pyc, flag, url, "@" + large], stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True)
        clients["node-large"] = subprocess.Popen(["node", nodec, flag, url, "@" + large], stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True, env=env)
        clients["node"] = subprocess.Popen(["node", nodec, flag, url, body], stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True, env=env)
        clients["node-sse"] = subprocess.Popen(["node", nodec, flag, url, stream], stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True, env=env)
        curl = subprocess.Popen(["curl", "-sk", "-X", "POST", url, "-H", "Content-Type: application/json",
                                 "-H", "Authorization: Bearer SECRET-TOKEN", "-d", "@-"],
                                stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True)
        clients["curl"] = curl
        # Cleartext HTTP: no TLS library is involved, so these are seen through the syscall probes.
        large_p = os.path.join(tmp, "large.json")
        quiet = dict(stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True)
        clients["plain-python"] = subprocess.Popen(["/usr/bin/python3", pyc, flag, purl, body], **quiet)
        clients["plain-python-sse"] = subprocess.Popen(["/usr/bin/python3", pyc, flag, purl, stream], **quiet)
        clients["plain-python-large"] = subprocess.Popen(["/usr/bin/python3", pyc, flag, purl, "@" + large_p], **quiet)
        clients["plain-node"] = subprocess.Popen(["node", nodec, flag, purl, body], env=env, **quiet)
        clients["plain-node-sse"] = subprocess.Popen(["node", nodec, flag, purl, stream], env=env, **quiet)
        clients["plain-node-large"] = subprocess.Popen(["node", nodec, flag, purl, "@" + large_p], env=env, **quiet)
        if uv:
            clients["plain-uv-python"] = subprocess.Popen([uv, pyc, flag, purl, body], **quiet)
        plain_curl = subprocess.Popen(["curl", "-s", "-X", "POST", purl, "-H", "Content-Type: application/json",
                                       "-H", "Authorization: Bearer SECRET-TOKEN", "-d", "@-"], stdin=subprocess.PIPE, **quiet)
        clients["plain-curl"] = plain_curl
        if os.path.exists(go_client):
            clients["plain-go"] = subprocess.Popen([go_client, flag, purl, body], **quiet)

        # HTTP/2 can only be followed from the first byte of a connection, so these clients connect after the
        # helper is attached: the Node client waits for the flag file, and curl retries until the server appears.
        h2port = free_port()
        h2url = f"https://127.0.0.1:{h2port}/v1/chat/completions"
        h2srv_js, h2cl_js = os.path.join(tmp, "h2srv.js"), os.path.join(tmp, "h2cl.js")
        open(h2srv_js, "w").write(H2_SERVER)
        open(h2cl_js, "w").write(H2_CLIENT)
        clients["node-h2"] = subprocess.Popen(["node", h2cl_js, flag, h2url, body], stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True)
        clients["node-h2-sse"] = subprocess.Popen(["node", h2cl_js, flag, h2url, stream], stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True)
        bodyfile = os.path.join(tmp, "body.json")
        open(bodyfile, "w").write(body)
        clients["curl-h2"] = subprocess.Popen(
            ["curl", "-sk", "--http2", "--retry", "40", "--retry-delay", "1", "--retry-connrefused", "-X", "POST", h2url,
             "-H", "Content-Type: application/json", "-H", "Authorization: Bearer SECRET-TOKEN", "-d", "@" + bodyfile],
            stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True)
        time.sleep(1.0)

        pids = ",".join(str(p.pid) for p in clients.values())
        cmd = ([ "sudo", "-n"] if args.sudo else []) + [helper, "trace", "--pid", pids, "--idle", "500ms", "--exit-on-stdin-eof", "--verbose"] + (["--dump-events"] if os.environ.get("E2E_DUMP") else [])
        jsonl = os.path.join(tmp, "out.jsonl")
        penv = None
        if args.tokprof:
            src = os.path.join(os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__)))), "src")
            cmd = [sys.executable, "-m", "tokprof", "ebpf", "--pid", pids, "--out", jsonl, "--helper", helper]
            penv = dict(os.environ, PYTHONPATH=src)
        h = subprocess.Popen(cmd, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True,
                             env=penv if args.tokprof else None)
        err_lines, out_lines = [], []
        ready_evt = threading.Event()

        def pump_err():
            for line in h.stderr:
                err_lines.append(line.rstrip())
                if line.strip().endswith("ready"):
                    ready_evt.set()

        threading.Thread(target=pump_err, daemon=True).start()
        t_out = threading.Thread(target=lambda: out_lines.extend(h.stdout), daemon=True)
        t_out.start()
        ready = ready_evt.wait(30)
        if not ready:
            print("helper did not become ready:\n" + "\n".join(err_lines))
            h.kill()
            return 2
        time.sleep(2.5)  # let it attach to every client (rescan runs every second)
        h2srv = subprocess.Popen(["node", h2srv_js, str(h2port), key, crt], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        time.sleep(0.5)
        open(large_p, "w").write(json.dumps(REQ_LARGE))
        open(flag, "w").close()
        curl.stdin.write(body)
        curl.stdin.close()
        plain_curl.stdin.write(body)
        plain_curl.stdin.close()
        for p in clients.values():
            p.wait(timeout=30)
        time.sleep(2.0)
        if args.tokprof:
            h.send_signal(2)  # SIGINT: tokprof closes the helper's stdin and flushes
        else:
            h.stdin.close()
        try:
            h.wait(timeout=150)
        except subprocess.TimeoutExpired:
            print("helper did not stop after its stdin closed; last log lines:\n" + "\n".join(err_lines[-15:]))
            return 3
        t_out.join(5)
        time.sleep(0.2)
        out = "".join(out_lines)
        srv.shutdown()
        plain_srv.shutdown()
        h2srv.kill()

        if os.environ.get('E2E_DEBUG'):
            print('RAW', out[:1500])
        if args.tokprof:
            return check_tokprof(jsonl, len(clients), err_lines)
        lines = [json.loads(x) for x in out.splitlines() if x.strip()]
        by_pid = {}
        for ln in lines:
            by_pid.setdefault(ln["pid"], []).append(ln)
        failed = 0
        for name, p in clients.items():
            got = by_pid.get(p.pid, [])
            ok = len(got) == 1
            detail = ""
            if ok:
                g = got[0]
                want = REQ_LARGE if "large" in name else REQ_STREAM if "sse" in name else REQ
                checks = {
                    "request": json.loads(g["request"]) == want,
                    "path": g["path"] == "/v1/chat/completions",
                    "status": g["status"] == 200,
                    "usage-in-response": "77" in g["response"],
                    "sse-flag": g["sse"] == ("sse" in name),
                }
                bad = [k for k, v in checks.items() if not v]
                ok = not bad
                detail = f"status={g['status']} sse={g['sse']} resp={len(g['response'])}B ttfb={g['ttfb_ms']:.1f}ms"
                if bad:
                    detail += f" FAILED CHECKS: {bad} response={g['response'][:130]!r}"
            blob = json.dumps(got)
            leaked = "SECRET-TOKEN" in blob
            if not (ok and not leaked) and not got:
                try:
                    detail += f" client rc={p.returncode} stdout={(p.stdout.read() or '')[:120]!r}"
                except Exception:
                    pass
            print(f"{'PASS' if ok and not leaked else 'FAIL'}  {name:18} exchanges={len(got)} {detail}{' LEAK' if leaked else ''}")
            failed += 0 if ok and not leaked else 1
        print("\n".join(err_lines))
        return 1 if failed else 0
    finally:
        for p in list(locals().get('clients', {}).values()):
            if p.poll() is None:
                p.kill()
        if not args.keep:
            shutil.rmtree(tmp, ignore_errors=True)


if __name__ == "__main__":
    sys.exit(main())
