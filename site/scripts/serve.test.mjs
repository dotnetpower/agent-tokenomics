import test from "node:test";
import assert from "node:assert/strict";
import { createSiteServer, preferredRepresentation } from "./serve.mjs";

test("Accept quality and exclusions take precedence; wildcards keep browser HTML", () => {
  for (const accept of [undefined, "*/*", "text/*", "text/html", "text/html, text/markdown;q=0.5", "text/markdown;q=0, */*;q=1"])
    assert.equal(preferredRepresentation(accept), "html", accept);
  for (const accept of ["text/markdown", "text/markdown, text/html", "*/*, text/markdown", "text/html;q=0.1, text/markdown;q=0.9", "text/html;q=0, text/*;q=0.8"])
    assert.equal(preferredRepresentation(accept), "markdown", accept);
  assert.equal(preferredRepresentation("application/json"), undefined);
  assert.equal(preferredRepresentation("text/html;q=0, text/markdown;q=0, */*"), undefined);
});

test("HTTP server keeps 404s honest, methods read-only, and representations isolated", async (t) => {
  const server = await createSiteServer();
  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
  t.after(() => new Promise((resolve) => server.close(resolve)));
  const root = `http://127.0.0.1:${server.address().port}`;
  const homeRedirect = await fetch(root, { redirect: "manual" });
  const home = homeRedirect.status === 308 ? homeRedirect.headers.get("location") : "/";
  const md = await fetch(`${root}${home}`, { headers: { Accept: "text/markdown" } });
  assert.equal(md.headers.get("content-type"), "text/markdown; charset=utf-8");
  assert.match(md.headers.get("vary"), /Accept, Accept-Encoding/);
  assert.equal(md.headers.get("cdn-cache-control"), "no-store");
  assert.match(await md.text(), /^# /);
  const html = await fetch(`${root}${home}`, { headers: { Accept: "text/html" } });
  assert.match(html.headers.get("content-type"), /text\/html/);
  assert.match(await html.text(), /<!DOCTYPE html>/i);
  assert.equal((await fetch(`${root}${home}`, { headers: { Accept: "application/json" } })).status, 406);
  assert.equal((await fetch(`${root}${home}`, { method: "DELETE" })).status, 405);
  for (const pathname of ["/missing-translation/", "/.git/config", "/%2e%2e%2fpackage.json", `${home}404.html`]) {
    const response = await fetch(`${root}${pathname}`);
    assert.equal(response.status, 404, pathname);
    assert.equal(response.headers.get("x-robots-tag"), "noindex");
    assert.match(await response.text(), /noindex/);
  }
  const catalog = await fetch(`${root}/.well-known/api-catalog`, { method: "HEAD" });
  assert.equal(catalog.status, 200);
  assert.match(catalog.headers.get("content-type"), /application\/linkset\+json/);
  assert.match(catalog.headers.get("link"), /rel="api-catalog"/);
  assert.equal(await catalog.text(), "");
});
