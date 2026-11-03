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
        open(flag, "w").close()
        curl.stdin.write(body)
        curl.stdin.close()
        for p in clients.values():
            p.wait(timeout=30)
        time.sleep(2.0)
        if args.tokprof:
            h.send_signal(2)  # SIGINT: tokprof closes the helper's stdin and flushes
        else:
            h.stdin.close()
        h.wait(timeout=30)
        t_out.join(5)
        time.sleep(0.2)
        out = "".join(out_lines)
        srv.shutdown()

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
