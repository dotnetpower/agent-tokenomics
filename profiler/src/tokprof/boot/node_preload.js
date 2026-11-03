// tokprof in-process hook for Node (loaded with --require via NODE_OPTIONS; no dependencies).
//
// Copies LLM request/response bodies sent with global fetch to the local tokprof gateway's ingest endpoint.
// It never modifies requests or responses and swallows its own errors.
"use strict";

const http = require("http");

const PORT = process.env.TOKPROF_PORT;
const TOKEN = process.env.TOKPROF_TOKEN;
const LLM_PATH = /\/(chat\/completions|responses|messages)$/;
let warned = false;

function warn(msg) {
  if (!warned) {
    warned = true;
    try { process.stderr.write("tokprof(hook): " + msg + "\n"); } catch (_) { /* ignore */ }
  }
}

function send(payload) {
  try {
    const body = Buffer.from(JSON.stringify(payload), "utf8");
    const req = http.request({
      host: "127.0.0.1", port: Number(PORT), path: "/__tokprof/ingest", method: "POST",
      headers: { "Content-Type": "application/json", "Content-Length": body.length, "X-Tokprof-Token": TOKEN },
      agent: false,
    }, (res) => res.resume());
    req.on("error", (e) => warn("could not deliver a captured call: " + e.message));
    req.setTimeout(5000, () => req.destroy());
    req.end(body);
  } catch (e) {
    warn("could not deliver a captured call: " + e.message);
  }
}

function bodyText(body) {
  if (typeof body === "string") return body;
  if (body instanceof Uint8Array) return Buffer.from(body).toString("utf8");
  if (body instanceof ArrayBuffer) return Buffer.from(body).toString("utf8");
  return null; // streams, FormData, Blob: not captured
}

if (PORT && TOKEN && typeof globalThis.fetch === "function") {
  const origFetch = globalThis.fetch;
  globalThis.fetch = async function tokprofFetch(input, init) {
    let capture = null;
    try {
      const url = new URL(typeof input === "string" || input instanceof URL ? String(input) : input.url);
      const method = ((init && init.method) || (input && input.method) || "GET").toUpperCase();
      const viaGateway = (url.hostname === "127.0.0.1" || url.hostname === "localhost") && url.port === PORT;
      if (method === "POST" && !viaGateway && LLM_PATH.test(url.pathname)) {
        const text = bodyText(init && init.body);
        if (text !== null) capture = { path: url.pathname, request: text, ts: Date.now() / 1000 };
      }
    } catch (_) { capture = null; }

    const res = await origFetch.apply(this, arguments);
    if (capture) {
      try {
        const copy = res.clone();
        const t0 = Date.now();
        copy.text().then((text) => {
          send({
            path: capture.path, request: capture.request, status: res.status, response: text,
            sse: (res.headers.get("content-type") || "").includes("text/event-stream"),
            ts: capture.ts, ttfb_ms: Date.now() - t0, error: null,
          });
        }).catch((e) => warn("could not read a response copy: " + e.message));
      } catch (e) {
        warn("hook error: " + e.message);
      }
    }
    return res;
  };
}
