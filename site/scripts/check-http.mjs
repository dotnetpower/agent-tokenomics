import { readFileSync } from "node:fs";
import path from "node:path";
import { pathToFileURL } from "node:url";
import { load } from "cheerio";
import { createSiteServer } from "./serve.mjs";
import { distDir, indexNowKey, localFile } from "./site-contract.mjs";

export async function inspectHttp(target, { staticHost = false } = {}) {
  const errors = [];
  const check = (condition, message) => { if (!condition) errors.push(message); };
  const index = JSON.parse(readFileSync(path.join(distDir, "api/pages.json"), "utf8"));
  const spec = JSON.parse(readFileSync(path.join(distDir, "openapi.json"), "utf8"));
  const canonicalRoot = new URL(`${spec.servers[0].url}/`);
  const deployment = new URL(target);
  if (!["localhost", "127.0.0.1", "[::1]"].includes(deployment.hostname)) {
    check(deployment.origin === canonicalRoot.origin, "public deployment origin must match the built canonical origin");
    check(deployment.protocol === "https:", "public deployment must use HTTPS");
  }
  const transport = (canonical) => {
    const source = new URL(canonical);
    return new URL(`${source.pathname}${source.search}`, deployment.origin).href;
  };
  const get = (url, headers) => fetch(url, { headers, redirect: "manual", signal: AbortSignal.timeout(20000) });
  const assets = new Set();
  const safety = (response, label) => {
    const vary = response.headers.get("vary")?.toLowerCase().split(",").map((item) => item.trim()) ?? [];
    check(vary.includes("accept") && vary.includes("accept-encoding"), `${label}: Vary must include Accept and Accept-Encoding`);
    check(/\bno-store\b/i.test(response.headers.get("cdn-cache-control") ?? ""), `${label}: CDN cache must be no-store`);
  };
  for (const page of index.pages) {
    const response = await get(transport(page.url), { Accept: "text/html" });
    const html = await response.text();
    check(response.status === 200 && response.headers.get("content-type")?.includes("text/html"), `${page.url}: HTML HTTP status/type`);
    check(html.includes(`<title>${page.title.replaceAll("&", "&amp;").replaceAll("<", "&lt;").replaceAll(">", "&gt;")}</title>`), `${page.url}: deployed title differs from build`);
    check(html === readFileSync(localFile(page.url), "utf8"), `${page.url}: deployed HTML differs from the verified artifact`);
    const $ = load(html);
    $('link[rel="stylesheet"][href],script[src],img[src]').each((_i, element) => {
      const url = new URL($(element).attr("src") ?? $(element).attr("href"), page.url);
      if (localFile(url.href)) assets.add(url.href);
    });
    const md = await get(transport(page.markdown));
    const expectedMarkdown = readFileSync(localFile(page.markdown), "utf8");
    check(md.status === 200 && (await md.text()) === expectedMarkdown, `${page.url}: static Markdown not deployed or differs from build`);
    if (staticHost) continue;
    safety(response, page.url);
    for (const accept of ["text/markdown, text/html;q=0.5", "text/html;q=0.1, text/markdown;q=0.9"]) {
      const mdResponse = await get(transport(page.url), { Accept: accept });
      const text = await mdResponse.text();
      check(text === expectedMarkdown, `${page.url}: negotiated Markdown differs from the verified artifact`);
      check(mdResponse.status === 200 && mdResponse.headers.get("content-type") === "text/markdown; charset=utf-8", `${page.url}: Markdown HTTP status/type`);
      check(text.startsWith(`# ${page.title}\n`) && !/<(script|style|nav|footer)\b/i.test(text), `${page.url}: Markdown contaminated by HTML`);
      safety(mdResponse, `${page.url} Markdown`);
    }
    const htmlAgain = await get(transport(page.url), { Accept: "text/html, text/markdown;q=0.1" });
    check((await htmlAgain.text()) === html, `${page.url}: HTML changed after Markdown request`);
    safety(htmlAgain, `${page.url} HTML after Markdown`);
  }
  assets.add(new URL("og-cover.png", canonicalRoot).href);
  for (const asset of assets) {
    const response = await get(transport(asset));
    check(response.status === 200, `${asset}: deployed asset HTTP ${response.status}`);
    check(Buffer.from(await response.arrayBuffer()).equals(readFileSync(localFile(asset))), `${asset}: deployed bytes differ from the verified artifact`);
  }
  const prefix = canonicalRoot.pathname.replace(/\/$/, "");
  const resources = {
    "robots.txt": "text/plain", "sitemap.xml": "xml", "rss.xml": "xml",
    "llms.txt": "text/plain", "llms-full.txt": "text/plain",
    "openapi.json": staticHost ? "json" : "application/openapi+json",
    ".well-known/api-catalog": staticHost ? undefined : "application/linkset+json",
    "auth.md": staticHost ? undefined : "text/markdown",
    "api/pages.json": "application/json", [`${indexNowKey}.txt`]: "text/plain",
  };
  for (const [resource, type] of Object.entries(resources)) {
    const response = await get(new URL(`${prefix}/${resource}`, deployment.origin));
    const actual = await response.text();
    check(response.status === 200, `${resource}: HTTP ${response.status}`);
    check(!type || response.headers.get("content-type")?.includes(type), `${resource}: incorrect Content-Type`);
    check(actual === readFileSync(path.join(distDir, resource), "utf8"), `${resource}: deployed content differs from build`);
  }
  const missing = await get(new URL(`${prefix}/__preflight_missing_page_7b49/`, deployment.origin), { Accept: "text/html" });
  check(missing.status === 404, `missing page: HTTP ${missing.status}, expected 404`);
  check(/noindex/.test(await missing.text()), "missing page: noindex missing");
  if (!staticHost) {
    const home = await get(new URL(`${prefix}/`, deployment.origin));
    const link = home.headers.get("link") ?? "";
    for (const rel of ["api-catalog", "service-desc", "service-doc", "describedby"])
      check(link.includes(`rel="${rel}"`), `homepage Link header missing ${rel}`);
    for (const resource of [".well-known/api-catalog", "openapi.json", "llms.txt", "llms-full.txt", "auth.md", "robots.txt", `${indexNowKey}.txt`]) {
      const response = await get(new URL(`/${resource}`, deployment.origin));
      check(response.status === 200, `root /${resource}: HTTP ${response.status}`);
      check((await response.text()) === readFileSync(path.join(distDir, resource), "utf8"), `root /${resource}: content mismatch or soft 404`);
      const expectedType = resource === ".well-known/api-catalog" ? "application/linkset+json"
        : resource === "openapi.json" ? "application/openapi+json"
          : resource === "auth.md" ? "text/markdown" : "text/plain";
      check(response.headers.get("content-type")?.includes(expectedType), `root /${resource}: incorrect Content-Type`);
    }
    const head = await fetch(new URL("/.well-known/api-catalog", deployment.origin), { method: "HEAD", signal: AbortSignal.timeout(20000) });
    check(head.status === 200 && !!head.headers.get("link") && !(await head.text()), "catalog HEAD must have Link header and no body");
    const method = await fetch(transport(index.pages[0].url), { method: "POST", signal: AbortSignal.timeout(20000) });
    check(method.status === 405 && method.headers.get("allow") === "GET, HEAD", "read-only method policy");
  }
  return { errors, pages: index.pages.length, profile: staticHost ? "static-only (HTTP agent requirements NOT certified)" : "full" };
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  let server;
  try {
    const targetArg = process.argv.find((arg) => arg.startsWith("--url="));
    let target = targetArg?.slice("--url=".length);
    if (!target) {
      server = await createSiteServer();
      await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
      target = `http://127.0.0.1:${server.address().port}`;
    }
    const result = await inspectHttp(target, { staticHost: process.argv.includes("--static-host") });
    console.log(`[check-http] ${result.profile}; ${result.pages} pages; ${result.errors.length} errors`);
    if (result.errors.length) {
      console.error(result.errors.slice(0, 80).join("\n"));
      process.exitCode = 1;
    }
  } finally {
    if (server) await new Promise((resolve, reject) => server.close((error) => error ? reject(error) : resolve()));
  }
}
