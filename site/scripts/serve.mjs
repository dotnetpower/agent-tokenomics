import { createServer } from "node:http";
import { readFile, stat } from "node:fs/promises";
import { pathToFileURL } from "node:url";
import path from "node:path";
import { distDir, discoveryLink } from "./site-contract.mjs";

export function preferredRepresentation(accept = "*/*") {
  const ranges = accept.toLowerCase().split(",").map((part, order) => {
    const [type, ...parameters] = part.trim().split(";");
    const quality = parameters.map((item) => item.trim()).find((item) => item.startsWith("q="));
    const q = quality ? Number(quality.slice(2)) : 1;
    return { type, q: Number.isFinite(q) && q >= 0 && q <= 1 ? q : 0, order };
  });
  const quality = (type) => {
    for (const [specificity, match] of [type, "text/*", "*/*"].entries()) {
      const candidates = ranges.filter((range) => range.type === match);
      if (candidates.length) return { ...candidates[0], specificity };
    }
    return { q: 0, order: Infinity, specificity: Infinity };
  };
  const html = quality("text/html");
  const md = quality("text/markdown");
  if (html.q === 0 && md.q === 0) return undefined;
  return md.q > html.q || (md.q === html.q &&
    (md.specificity < html.specificity || (md.specificity === html.specificity && md.order < html.order)))
    ? "markdown" : "html";
}

const types = {
  ".html": "text/html; charset=utf-8", ".md": "text/markdown; charset=utf-8",
  ".txt": "text/plain; charset=utf-8", ".json": "application/json; charset=utf-8",
  ".xml": "application/xml; charset=utf-8", ".css": "text/css; charset=utf-8",
  ".js": "text/javascript; charset=utf-8", ".svg": "image/svg+xml",
  ".png": "image/png", ".webp": "image/webp", ".woff2": "font/woff2",
  ".csv": "text/csv; charset=utf-8", ".xlsx": "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
};

export async function createSiteServer(directory = distDir) {
  const index = JSON.parse(await readFile(path.join(directory, "api/pages.json"), "utf8"));
  const spec = JSON.parse(await readFile(path.join(directory, "openapi.json"), "utf8"));
  const canonicalRoot = new URL(`${spec.servers[0].url}/`);
  const prefix = canonicalRoot.pathname.replace(/\/$/, "");
  const routes = new Map(index.pages.map((page) => [new URL(page.url).pathname, page]));
  const aliases = new Set([
    "/.well-known/api-catalog", "/openapi.json", "/auth.md", "/llms.txt", "/llms-full.txt",
    "/robots.txt", "/sitemap.xml", "/sitemap-index.xml", "/rss.xml",
  ]);
  async function handle(request, response) {
    response.setHeader("Vary", "Accept, Accept-Encoding");
    response.setHeader("CDN-Cache-Control", "no-store");
    response.setHeader("Cache-Control", "no-store");
    response.setHeader("X-Content-Type-Options", "nosniff");
    response.setHeader("Link", discoveryLink);
    const send = (status, type, body) => {
      response.writeHead(status, { "Content-Type": type });
      response.end(request.method === "HEAD" ? undefined : body);
    };
    if (!["GET", "HEAD"].includes(request.method)) {
      response.setHeader("Allow", "GET, HEAD");
      send(405, "text/plain; charset=utf-8", "Method not allowed\n");
      return;
    }
    let url;
    let pathname;
    try {
      url = new URL(request.url, "http://localhost");
      pathname = decodeURIComponent(url.pathname);
    } catch {
      send(400, "text/plain; charset=utf-8", "Invalid request URL\n");
      return;
    }
    if (pathname === "/" && prefix) {
      response.writeHead(308, { Location: `${prefix}/${url.search}` });
      response.end();
      return;
    }
    let route = routes.get(pathname);
    if (!route && routes.has(`${pathname}/`)) {
      response.writeHead(308, { Location: `${pathname}/${url.search}` });
      response.end();
      return;
    }
    if (pathname.endsWith("/index.html") && routes.has(pathname.slice(0, -"index.html".length))) {
      response.writeHead(308, { Location: `${pathname.slice(0, -"index.html".length)}${url.search}` });
      response.end();
      return;
    }
    const keyAlias = /^\/[a-f0-9]{32}\.txt$/.test(pathname);
    let rel;
    if (aliases.has(pathname) || keyAlias) rel = pathname.slice(1);
    else if (pathname.startsWith(`${prefix}/`)) rel = pathname.slice(prefix.length + 1);
    if (route) {
      const representation = preferredRepresentation(request.headers.accept);
      if (!representation) {
        send(406, "text/plain; charset=utf-8", "Accept text/html or text/markdown\n");
        return;
      }
      if (representation === "markdown") {
        rel = new URL(route.markdown).pathname.slice(prefix.length + 1);
      } else {
        rel = pathname.slice(prefix.length + 1);
      }
    }
    const file = rel === undefined ? undefined : path.resolve(directory, rel.endsWith("/") || !rel ? `${rel}index.html` : rel);
    const confined = file?.startsWith(`${path.resolve(directory)}${path.sep}`) &&
      !rel.split(/[\\/]/).some((part) => part.startsWith(".") && part !== ".well-known");
    let info;
    if (confined) {
      try { info = await stat(file); }
      catch (error) { if (error.code !== "ENOENT" && error.code !== "ENOTDIR") throw error; }
    }
    if (!info?.isFile() || rel === "404.html") {
      response.setHeader("X-Robots-Tag", "noindex");
      send(404, "text/html; charset=utf-8", await readFile(path.join(directory, "404.html")));
      return;
    }
    const type = rel === ".well-known/api-catalog"
      ? 'application/linkset+json; profile="https://www.rfc-editor.org/info/rfc9727"'
      : rel === "openapi.json" ? "application/openapi+json"
        : types[path.extname(file)] ?? "application/octet-stream";
    send(200, type, await readFile(file));
  }
  return createServer((request, response) => {
    handle(request, response).catch((error) => {
      console.error("[serve] request failed:", error);
      if (!response.headersSent) response.writeHead(500, { "Content-Type": "text/plain; charset=utf-8" });
      response.end("Internal server error\n");
    });
  });
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const port = Number(process.env.PORT ?? 4321);
  if (!Number.isInteger(port) || port < 0 || port > 65535) throw new Error("PORT must be an integer from 0 to 65535");
  const host = process.env.HOST ?? "127.0.0.1";
  const server = await createSiteServer();
  server.listen(port, host, () => console.log(`[serve] http://${host}:${server.address().port}`));
}
