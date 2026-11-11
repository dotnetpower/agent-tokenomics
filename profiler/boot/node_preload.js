// tokprof in-process hook for Node (loaded with --require via NODE_OPTIONS; no dependencies).
//
// Copies LLM request/response bodies sent with global fetch, http.request, or https.request (including
// clients built on them such as axios) to the local tokprof gateway's ingest endpoint.
// It never modifies requests or responses and swallows its own errors.
"use strict";

const crypto = require("crypto");
const fs = require("fs");
const http = require("http");
const https = require("https");
const zlib = require("zlib");

const PORT = process.env.TOKPROF_PORT;
const TOKEN = process.env.TOKPROF_TOKEN || "";
const INGEST_URL = process.env.TOKPROF_INGEST_URL || "";
const STDOUT_LOG = String(process.env.TOKPROF_STDOUT_LOG || "").trim().toLowerCase();
const STDOUT_FILE = String(process.env.TOKPROF_STDOUT_FILE || "").trim();
const HAS_STDOUT = ["1", "true", "stdout", "stderr"].includes(STDOUT_LOG) || Boolean(STDOUT_FILE);
const ACTIVE = Boolean((PORT && TOKEN) || INGEST_URL || HAS_STDOUT);
const CHUNK_MAX_CHARS = 20000;
const CAP = 32 * 1024 * 1024;
const origHttpRequest = http.request;
const origHttpsRequest = https.request;
// Pre-filter only; tokprof's detector decides. Keep identical to tokprof.detect.plausible (tests/vectors/prefilter.json).
const EXCLUDED_PATH = /\/(embeddings?|moderations?|rerank|tokenize|detokenize|count_tokens|audio\/(speech|transcriptions|translations)|images?\/(generations|edits|variations)|files|batches|fine_tuning|uploads|vector_stores|assistants|threads|realtime)(\/|$)|:(embedContent|batchEmbedContents|countTokens)$/i;
const PLAUSIBLE_KEYS = /"(?:messages|contents|chat_history)"\s*:\s*\[|"(?:prompt|input|inputs|preamble|message)"\s*:/;

function plausible(method, path, text) {
  try {
    if (String(method).toUpperCase() !== "POST" || !text) return false;
    const sampleHead = text.length > 512 ? text.slice(0, 512) : text;
    const head = sampleHead.replace(/^\s+/, "");
    if (head.length < 8 || head[0] !== "{") return false;
    const p = (path.split("?")[0].split("#")[0].replace(/\/+$/, "")) || "/";
    if (EXCLUDED_PATH.test(p)) return false;
    if (text.length <= 81920) return PLAUSIBLE_KEYS.test(text);
    return PLAUSIBLE_KEYS.test(text.slice(0, 65536)) || PLAUSIBLE_KEYS.test(text.slice(-16384));
  } catch (_) { return false; }
}
if (typeof module !== "undefined" && module.exports) module.exports.plausible = plausible;
let warned = false;

function warn(msg) {
  if (!warned) {
    warned = true;
    try { process.stderr.write("tokprof(hook): " + msg + "\n"); } catch (_) { /* ignore */ }
  }
}

function emitStdoutChunks(payload) {
  try {
    const raw = Buffer.from(JSON.stringify(payload), "utf8");
    const b64 = zlib.gzipSync(raw).toString("base64");
    const cid = crypto.createHash("sha256").update(raw).update(String(Date.now())).digest("hex").slice(0, 12);
    const total = Math.max(1, Math.ceil(b64.length / CHUNK_MAX_CHARS));
    let out = "";
    for (let i = 0; i < total; i++) {
      const part = b64.slice(i * CHUNK_MAX_CHARS, (i + 1) * CHUNK_MAX_CHARS);
      out += "[TOKPROF_CHUNK] " + JSON.stringify({ id: cid, seq: i, total, data: part }) + "\n";
    }
    if (STDOUT_FILE) {
      fs.appendFileSync(STDOUT_FILE, out, "utf8");
    } else if (STDOUT_LOG === "stdout") {
      process.stdout.write(out);
    } else {
      process.stderr.write(out);
    }
  } catch (e) {
    warn("could not emit stdout chunks: " + e.message);
  }
}

const pendingBodies = new Map();
let nextSendId = 1;

function flushPendingOnExit() {
  if (pendingBodies.size === 0) return;
  try {
    const items = Array.from(pendingBodies.values());
    pendingBodies.clear();
    const { spawn } = require("child_process");
    const targetUrl = INGEST_URL || `http://127.0.0.1:${PORT}/__tokprof/ingest`;
    const script = `const http=require("http"),https=require("https");const u=new URL(process.argv[1]);const tok=process.argv[2];const items=JSON.parse(process.argv[3]);for(const b of items){const buf=Buffer.from(b,"base64");const m=u.protocol==="https:"?https:http;const r=m.request({hostname:u.hostname,port:Number(u.port)||(u.protocol==="https:"?443:80),path:u.pathname||"/__tokprof/ingest",method:"POST",headers:{"Content-Type":"application/json","Content-Length":buf.length,"X-Tokprof-Token":tok}},res=>res.resume());r.on("error",()=>{});r.end(buf);}`;
    const child = spawn(
      process.execPath,
      ["-e", script, targetUrl, TOKEN, JSON.stringify(items.map((b) => b.toString("base64")))],
      { detached: true, stdio: "ignore", env: {} }
    );
    child.unref();
  } catch (_) { /* best effort */ }
}

if (ACTIVE) {
  const origProcessExit = process.exit;
  process.exit = function tokprofProcessExit(code) {
    flushPendingOnExit();
    return origProcessExit.call(this, code);
  };
}

function send(payload) {
  try {
    if (HAS_STDOUT) {
      emitStdoutChunks(payload);
      if (!((PORT && TOKEN) || INGEST_URL)) return;
    }
    const body = Buffer.from(JSON.stringify(payload), "utf8");
    const sendId = nextSendId++;
    pendingBodies.set(sendId, body);
    let reqOpts;
    let reqFn = origHttpRequest;
    let reqMod = http;
    if (INGEST_URL) {
      const u = new URL(INGEST_URL);
      const isHttps = u.protocol === "https:";
      reqFn = isHttps ? origHttpsRequest : origHttpRequest;
      reqMod = isHttps ? https : http;
      reqOpts = {
        host: u.hostname,
        port: Number(u.port) || (isHttps ? 443 : 80),
        path: u.pathname || "/__tokprof/ingest",
        method: "POST",
        headers: { "Content-Type": "application/json", "Content-Length": body.length, "X-Tokprof-Token": TOKEN },
        agent: false,
      };
    } else {
      reqOpts = {
        host: "127.0.0.1", port: Number(PORT), path: "/__tokprof/ingest", method: "POST",
        headers: { "Content-Type": "application/json", "Content-Length": body.length, "X-Tokprof-Token": TOKEN },
        agent: false,
      };
    }
    const done = () => { pendingBodies.delete(sendId); };
    const req = reqFn.call(reqMod, reqOpts, (res) => {
      res.on("end", done);
      res.resume();
    });
    req.on("error", (e) => { done(); warn("could not deliver a captured call: " + e.message); });
    req.setTimeout(5000, () => { done(); req.destroy(); });
    req.end(body);
  } catch (e) {
    warn("could not deliver a captured call: " + e.message);
  }
}

function decodeBody(buf, encoding) {
  const enc = String(encoding || "").toLowerCase().trim();
  if (!enc || enc === "identity") return buf.toString("utf8");
  const zopts = { maxOutputLength: CAP };
  try {
    if (enc.includes("gzip")) return zlib.gunzipSync(buf, zopts).toString("utf8");
    if (enc.includes("deflate")) {
      try { return zlib.inflateSync(buf, zopts).toString("utf8"); }
      catch (_) { return zlib.inflateRawSync(buf, zopts).toString("utf8"); }
    }
    if (enc.includes("br") && typeof zlib.brotliDecompressSync === "function") {
      return zlib.brotliDecompressSync(buf, zopts).toString("utf8");
    }
    if (enc.includes("zstd") && typeof zlib.zstdDecompressSync === "function") {
      return zlib.zstdDecompressSync(buf, zopts).toString("utf8");
    }
  } catch (_) { /* fall through: keep the request even if response decompression fails */ }
  return "";
}

function parseReqArgs(defaultProto, arg0, arg1) {
  let method = "GET", hostname = "localhost", port = 0, pathname = "/", proto = defaultProto;
  let opts = null;
  if (typeof arg0 === "string" || arg0 instanceof URL) {
    const u = new URL(String(arg0));
    proto = u.protocol || defaultProto;
    hostname = u.hostname || "localhost";
    port = Number(u.port) || 0;
    pathname = u.pathname || "/";
    if (arg1 && typeof arg1 === "object" && typeof arg1 !== "function") opts = arg1;
  } else if (arg0 && typeof arg0 === "object") {
    opts = arg0;
  }
  if (opts) {
    if (opts.protocol) proto = opts.protocol;
    if (opts.method) method = String(opts.method).toUpperCase();
    if (opts.hostname || opts.host) {
      const h = String(opts.hostname || opts.host);
      if (h.startsWith("[")) {
        const i = h.indexOf("]");
        hostname = i > 0 ? h.slice(1, i) : h;
        if (!opts.port && h[i + 1] === ":") port = Number(h.slice(i + 2)) || port;
      } else if (h.includes(":")) {
        const parts = h.split(":");
        hostname = parts[0];
        if (!opts.port) port = Number(parts[1]) || port;
      } else {
        hostname = h;
      }
    }
    if (opts.port) port = Number(opts.port) || port;
    if (opts.path || opts.pathname) pathname = String(opts.path || opts.pathname).split("?")[0] || "/";
  }
  if (!port) port = proto === "https:" ? 443 : 80;
  return { method, hostname, port, pathname };
}

function wrapHttpModule(mod, defaultProto) {
  const orig = mod.request;
  if (!orig || orig._tokprofWrapped) return;
  function wrappedRequest(arg0, arg1) {
    const req = orig.apply(this, arguments);
    try {
      if (req._tokprofHooked) return req;
      const info = parseReqArgs(defaultProto, arg0, arg1);
      const viaGateway = (info.hostname === "127.0.0.1" || info.hostname === "localhost") && String(info.port) === String(PORT);
      if (info.method !== "POST" || viaGateway) return req;
      req._tokprofHooked = true;
      const reqChunks = [];
      let reqSize = 0;
      const addReq = (chunk, enc) => {
        if (!chunk || typeof chunk === "function" || reqSize >= CAP) return;
        const buf = Buffer.isBuffer(chunk) ? chunk : (chunk instanceof Uint8Array ? Buffer.from(chunk) : Buffer.from(String(chunk), typeof enc === "string" ? enc : "utf8"));
        reqChunks.push(buf);
        reqSize += buf.length;
      };
      const origWrite = req.write, origEnd = req.end;
      req.write = function(chunk, enc) { addReq(chunk, enc); return origWrite.apply(this, arguments); };
      req.end = function(chunk, enc) { addReq(chunk, enc); return origEnd.apply(this, arguments); };
      const t0 = Date.now();
      req.on("response", (res) => {
        try {
          const reqText = Buffer.concat(reqChunks).toString("utf8");
          if (!plausible("POST", info.pathname, reqText)) return;
          const respChunks = [];
          let respSize = 0, ttfb = null, sent = false;
          const origPush = res.push;
          res.push = function(chunk, enc) {
            try {
              if (chunk !== null && chunk !== undefined) {
                if (ttfb === null) ttfb = Date.now() - t0;
                if (respSize < CAP) {
                  const buf = Buffer.isBuffer(chunk) ? chunk : Buffer.from(chunk, typeof enc === "string" ? enc : undefined);
                  respChunks.push(buf);
                  respSize += buf.length;
                }
              } else if (chunk === null && !sent) {
                sent = true;
                const text = decodeBody(Buffer.concat(respChunks), res.headers && res.headers["content-encoding"]);
                const ctype = String((res.headers && res.headers["content-type"]) || "");
                send({
                  path: info.pathname, host: info.hostname, port: info.port, request: reqText,
                  status: res.statusCode, response: text, sse: ctype.includes("text/event-stream"),
                  ts: t0 / 1000, ttfb_ms: ttfb, latency_ms: Date.now() - t0, error: null,
                });
              }
            } catch (e) { warn("hook error: " + e.message); }
            return origPush.apply(this, arguments);
          };
        } catch (e) { warn("hook error: " + e.message); }
      });
    } catch (e) { warn("hook error: " + e.message); }
    return req;
  }
  wrappedRequest._tokprofWrapped = true;
  mod.request = wrappedRequest;
}

function wrapHttp2Module() {
  try {
    const http2 = require("http2");
    if (!http2 || typeof http2.connect !== "function" || http2.connect._tokprofWrapped) return;
    const origConnect = http2.connect;
    function wrappedConnect(authority, options, listener) {
      const session = origConnect.apply(this, arguments);
      try {
        const authUrl = new URL(typeof authority === "string" ? authority : String(authority));
        const viaGateway = (authUrl.hostname === "127.0.0.1" || authUrl.hostname === "localhost") && String(authUrl.port) === String(PORT);
        if (viaGateway || !session || typeof session.request !== "function" || session._tokprofWrapped) return session;
        session._tokprofWrapped = true;
        const origReq = session.request;
        session.request = function wrappedH2Request(headers) {
          const stream = origReq.apply(this, arguments);
          try {
            const method = String((headers && (headers[":method"] || headers.method)) || "GET").toUpperCase();
            const rawPath = String((headers && (headers[":path"] || headers.path)) || "/");
            const pathname = rawPath.split("?")[0] || "/";
            if (method !== "POST") return stream;
            const reqChunks = [];
            let reqSize = 0;
            const addReq = (chunk, enc) => {
              if (!chunk || typeof chunk === "function" || reqSize >= CAP) return;
              const buf = Buffer.isBuffer(chunk) ? chunk : (chunk instanceof Uint8Array ? Buffer.from(chunk) : Buffer.from(String(chunk), typeof enc === "string" ? enc : "utf8"));
              reqChunks.push(buf);
              reqSize += buf.length;
            };
            const origWrite = stream.write, origEnd = stream.end;
            stream.write = function(chunk, enc) { addReq(chunk, enc); return origWrite.apply(this, arguments); };
            stream.end = function(chunk, enc) { addReq(chunk, enc); return origEnd.apply(this, arguments); };
            const t0 = Date.now();
            stream.on("response", (respHeaders) => {
              try {
                const reqText = Buffer.concat(reqChunks).toString("utf8");
                if (!plausible("POST", pathname, reqText)) return;
                const respChunks = [];
                let respSize = 0, ttfb = null;
                stream.on("data", (chunk) => {
                  if (ttfb === null) ttfb = Date.now() - t0;
                  if (respSize < CAP) {
                    const buf = Buffer.isBuffer(chunk) ? chunk : Buffer.from(chunk);
                    respChunks.push(buf);
                    respSize += buf.length;
                  }
                });
                stream.on("end", () => {
                  const text = decodeBody(Buffer.concat(respChunks), respHeaders && respHeaders["content-encoding"]);
                  const ctype = String((respHeaders && respHeaders["content-type"]) || "");
                  const status = Number((respHeaders && respHeaders[":status"]) || 200);
                  send({
                    path: pathname, host: authUrl.hostname, port: Number(authUrl.port) || (authUrl.protocol === "https:" ? 443 : 80),
                    request: reqText, status, response: text, sse: ctype.includes("text/event-stream"),
                    ts: t0 / 1000, ttfb_ms: ttfb !== null ? ttfb : (Date.now() - t0), latency_ms: Date.now() - t0, error: null,
                  });
                });
              } catch (e) { warn("h2 hook error: " + e.message); }
            });
          } catch (e) { warn("h2 hook error: " + e.message); }
          return stream;
        };
      } catch (_) { /* ignore */ }
      return session;
    }
    wrappedConnect._tokprofWrapped = true;
    http2.connect = wrappedConnect;
  } catch (_) { /* ignore if http2 unavailable */ }
}

if (ACTIVE) {
  wrapHttpModule(http, "http:");
  wrapHttpModule(https, "https:");
  wrapHttp2Module();
}

async function extractFetchBodyText(input, init) {
  const rawBody = init && init.body !== undefined ? init.body : null;
  if (rawBody !== null) {
    if (typeof rawBody === "string") return rawBody;
    if (rawBody instanceof Uint8Array) return Buffer.from(rawBody).toString("utf8");
    if (rawBody instanceof ArrayBuffer) return Buffer.from(rawBody).toString("utf8");
    if (typeof URLSearchParams !== "undefined" && rawBody instanceof URLSearchParams) return rawBody.toString();
    if (typeof Blob !== "undefined" && rawBody instanceof Blob) return await rawBody.text();
  }
  if (input && typeof input === "object" && typeof input.clone === "function" && typeof input.text === "function") {
    try {
      return await input.clone().text();
    } catch (_) { return null; }
  }
  return null;
}

if (ACTIVE && typeof globalThis.fetch === "function") {
  const origFetch = globalThis.fetch;
  globalThis.fetch = async function tokprofFetch(input, init) {
    let capture = null;
    const reqStartMs = Date.now();
    try {
      const url = new URL(typeof input === "string" || input instanceof URL ? String(input) : input.url);
      const method = ((init && init.method) || (input && input.method) || "GET").toUpperCase();
      const viaGateway = (url.hostname === "127.0.0.1" || url.hostname === "localhost") && url.port === PORT;
      if (method === "POST" && !viaGateway) {
        const text = await extractFetchBodyText(input, init);
        if (text !== null && plausible(method, url.pathname, text)) {
          capture = { path: url.pathname, host: url.hostname, port: Number(url.port) || (url.protocol === "https:" ? 443 : 80), request: text, ts: reqStartMs / 1000 };
        }
      }
    } catch (_) { capture = null; }

    const res = await origFetch.apply(this, arguments);
    if (capture) {
      try {
        const headersAtMs = Date.now();
        const copy = res.clone();
        if (copy.body && typeof copy.body.getReader === "function") {
          (async () => {
            const reader = copy.body.getReader();
            const chunks = [];
            let size = 0;
            let firstByteMs = null;
            for (;;) {
              const { done, value } = await reader.read();
              if (done) break;
              if (firstByteMs === null) firstByteMs = Date.now() - reqStartMs;
              if (value && size < CAP) {
                const buf = Buffer.from(value);
                chunks.push(buf);
                size += buf.length;
              }
            }
            const text = Buffer.concat(chunks).toString("utf8");
            send({
              path: capture.path, host: capture.host, port: capture.port, request: capture.request, status: res.status, response: text,
              sse: (res.headers.get("content-type") || "").includes("text/event-stream"),
              ts: capture.ts, ttfb_ms: firstByteMs !== null ? firstByteMs : (headersAtMs - reqStartMs),
              latency_ms: Date.now() - reqStartMs, error: null,
            });
          })().catch((e) => warn("could not read a response copy: " + e.message));
        } else {
          copy.text().then((text) => {
            send({
              path: capture.path, host: capture.host, port: capture.port, request: capture.request, status: res.status, response: text,
              sse: (res.headers.get("content-type") || "").includes("text/event-stream"),
              ts: capture.ts, ttfb_ms: headersAtMs - reqStartMs, latency_ms: Date.now() - reqStartMs, error: null,
            });
          }).catch((e) => warn("could not read a response copy: " + e.message));
        }
      } catch (e) {
        warn("hook error: " + e.message);
      }
    }
    return res;
  };
}
