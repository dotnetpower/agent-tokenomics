import { readFileSync, writeFileSync, mkdirSync, existsSync } from "node:fs";
import path from "node:path";
import { load } from "cheerio";
import TurndownService from "turndown";
import {
  siteDir, distDir, root, origin, base, indexNowKey, limits, walk, canonicalFor,
  localFile, robotsText, xmlEscape as esc,
} from "./site-contract.mjs";

const markdown = new TurndownService({ headingStyle: "atx", codeBlockStyle: "fenced" });
markdown.addRule("table", {
  filter: "table",
  replacement: (_content, node) => {
    const rows = Array.from(node.querySelectorAll("tr")).map((tr) =>
      Array.from(tr.querySelectorAll("th, td")).map((cell) =>
        markdown.turndown(cell.innerHTML).replace(/\|/g, "\\|").replace(/\n/g, " "),
      ),
    );
    if (!rows.length) return "";
    return `\n\n| ${rows[0].join(" | ")} |\n| ${rows[0].map(() => "---").join(" | ")} |\n` +
      rows.slice(1).map((cells) => `| ${cells.join(" | ")} |`).join("\n") + "\n\n";
  },
});
markdown.addRule("definition-list", {
  filter: ["dt", "dd"],
  replacement: (content, node) => node.nodeName === "DT" ? `\n\n**${content}**\n` : `\n${content}\n`,
});

function write(rel, text) {
  const file = path.join(distDir, rel);
  mkdirSync(path.dirname(file), { recursive: true });
  writeFileSync(file, text);
}
const pages = [];
for (const file of walk(distDir, ".html")) {
  const rel = path.relative(distDir, file).split(path.sep).join("/");
  const rawHtml = readFileSync(file, "utf8");
  const $ = load(rel.startsWith("demo/")
    ? rawHtml.replaceAll("https://dotnetpower.github.io/agent-tokenomics/", root)
    : rawHtml);
  const is404 = /(^|\/)404(?:\.html|\/index\.html)$/.test(rel);
  if (is404) {
    $('meta[name="robots"]').remove();
    $("head").append('<meta name="robots" content="noindex, nofollow">');
    $('link[hreflang], script[type="application/ld+json"]').remove();
    writeFileSync(file, $.html());
    continue;
  }
  if (/\bnoindex\b/i.test($('meta[name="robots"]').attr("content") ?? "")) continue;
  const url = canonicalFor(file);
  $(".site-title img").attr("alt", "agent-tokenomics");
  const meta = (key) => $(`meta[name="${key}"], meta[property="${key}"]`).attr("content");
  if (rel.startsWith("demo/")) {
    // The exported report is real content, but its embedded deployment URLs are not portable.
    $('link[rel="canonical"]').attr("href", url);
    $('meta[property="og:url"]').attr("content", url);
    $('meta[property="og:image"], meta[name="twitter:image"]').attr("content", `${root}og-cover.png`);
    $('meta[property="og:image:width"],meta[property="og:image:height"],meta[property="og:image:type"]').remove();
    $("head").append('<meta property="og:image:width" content="1200"><meta property="og:image:height" content="630"><meta property="og:image:type" content="image/png">');
    $('script[type="application/ld+json"]').remove();
    const graph = [
      { "@type": "Organization", "@id": `${root}#organization`, name: "agent-tokenomics", url: root },
      { "@type": "WebSite", "@id": `${root}#website`, name: "agent-tokenomics", url: root },
      { "@type": "WebPage", "@id": url, url, name: $("title").text(), description: meta("description") },
    ];
    $("head").append(`<script type="application/ld+json">${JSON.stringify({ "@context": "https://schema.org", "@graph": graph }).replace(/</g, "\\u003c")}</script>`);
  }
  // Normalize only links to real built directory routes, never manufacture a target.
  $("a[href]").each((_i, element) => {
    const href = $(element).attr("href");
    const target = new URL(href, url);
    if (target.origin !== origin || !target.pathname.startsWith(`${base}/`)) return;
    const file = localFile(target.href);
    if (file && !target.pathname.endsWith("/") && existsSync(path.join(file, "index.html"))) {
      target.pathname += "/";
      $(element).attr("href", `${target.pathname}${target.search}${target.hash}`);
    }
  });
  const faq = $(".at-faq-item").map((_i, element) => ({
    "@type": "Question", name: $(element).find("h3").text().replace(/\s+/g, " ").trim(),
    acceptedAnswer: { "@type": "Answer", text: $(element).find("p").text().replace(/\s+/g, " ").trim() },
  })).get();
  if (faq.length) {
    const script = $('script[type="application/ld+json"]').first();
    const graph = JSON.parse(script.text());
    graph["@graph"] = graph["@graph"].filter((node) => node["@type"] !== "FAQPage");
    graph["@graph"].push({ "@type": "FAQPage", "@id": `${url}#faq`, url, mainEntity: faq });
    script.text(JSON.stringify(graph).replace(/</g, "\\u003c"));
  }
  for (const name of ["llms.txt", "llms-full.txt"]) {
    if (!$(`link[rel="alternate"][href="${root}${name}"]`).length)
      $("head").append(`<link rel="alternate" type="text/plain" href="${root}${name}">`);
  }
  const markdownUrl = url.endsWith("/") ? `${url}index.md` : `${url}.md`;
  if (!$('link[rel="alternate"][type="text/markdown"]').length)
    $("head").append(`<link rel="alternate" type="text/markdown" href="${markdownUrl}">`);
  const title = $("title").text().trim();
  const description = meta("description");
  const language = $("html").attr("lang")?.startsWith("ko") ? "ko" : "en";
  const alternates = Object.fromEntries($('link[rel="alternate"][hreflang]').map((_i, el) =>
    [[$(el).attr("hreflang"), $(el).attr("href")]],
  ).get());
  const content = $(".sl-markdown-content").first().clone();
  const body = content.length ? content : $("main").first().clone();
  if (!body.length) throw new Error(`No document content: ${rel}`);
  body.find('script, style, nav, footer, button, input, select, starlight-toc, .sr-only, [aria-hidden="true"], .token-budget form').remove();
  body.find("a[href], img[src]").each((_i, el) => {
    const attr = el.tagName === "img" ? "src" : "href";
    $(el).attr(attr, new URL($(el).attr(attr), url).href);
  });
  const text = `# ${title}\n\nCanonical URL: ${url}\n\n> ${description}\n\n${limits}\n\n` +
    markdown.turndown(body.html() ?? "").replace(/^# .+\n+/m, "") + "\n";
  write(path.relative(distDir, localFile(markdownUrl)), text);
  pages.push({ url, title, description, language, markdown: markdownUrl, alternates,
    created: meta("date.created"), updated: meta("date.updated") });
  writeFileSync(file, $.html());
}

const urls = new Set(pages.map((page) => page.url));
for (const page of pages) {
  for (const alternate of Object.values(page.alternates)) {
    if (!urls.has(alternate)) throw new Error(`Nonexistent translation: ${page.url} -> ${alternate}`);
  }
}
const urlNode = (page) => `  <url><loc>${esc(page.url)}</loc>${page.updated ? `<lastmod>${esc(page.updated)}</lastmod>` : ""}${Object.entries(page.alternates).map(([lang, href]) =>
  `<xhtml:link rel="alternate" hreflang="${esc(lang)}" href="${esc(href)}"/>`,
).join("")}</url>`;
const urlset = (items) => `<?xml version="1.0" encoding="UTF-8"?>\n<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9" xmlns:xhtml="http://www.w3.org/1999/xhtml">\n${items.map(urlNode).join("\n")}\n</urlset>\n`;
const shards = [];
for (let i = 0; i < pages.length; i += 4000) {
  const name = `sitemap-${shards.length}.xml`;
  write(name, urlset(pages.slice(i, i + 4000)));
  shards.push(name);
}
const sitemapIndex = `<?xml version="1.0" encoding="UTF-8"?>\n<sitemapindex xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">${shards.map((name) => `<sitemap><loc>${root}${name}</loc></sitemap>`).join("")}</sitemapindex>\n`;
write("sitemap-index.xml", sitemapIndex);
write("sitemap.xml", pages.length <= 4000 ? urlset(pages) : sitemapIndex);
write("rss.xml", `<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0"><channel><title>agent-tokenomics</title><link>${root}</link><description>LLM token cost and quality documentation (English / Korean).</description>
${pages.map((page) => `<item><title>${esc(page.title)}</title><link>${esc(page.url)}</link><guid isPermaLink="true">${esc(page.url)}</guid>${page.created ? `<pubDate>${new Date(`${page.created}T00:00:00Z`).toUTCString()}</pubDate>` : ""}<description>${esc(page.description)}</description></item>`).join("\n")}
</channel></rss>\n`);
write("robots.txt", robotsText());
write(`${indexNowKey}.txt`, `${indexNowKey}\n`);
const sourceIndex = readFileSync(path.join(siteDir, "..", "llms.txt"), "utf8");
writeFileSync(path.join(siteDir, "public", "llms.txt"), sourceIndex);
const index = sourceIndex.replaceAll("https://dotnetpower.github.io/agent-tokenomics/", root);
write("llms.txt", index);
write("llms-full.txt", `# agent-tokenomics — Full documentation corpus\n\nSource: ${root}\n\n${limits}\n\n` +
  pages.map((page) => readFileSync(localFile(page.markdown), "utf8")).join("\n---\n\n"));
write("api/pages.json", JSON.stringify({ version: "1", pages }, null, 2) + "\n");
const pageProperties = Object.fromEntries(["url", "title", "description", "language", "markdown"].map((name) =>
  [name, { type: "string", ...(["url", "markdown"].includes(name) ? { format: "uri" } : {}) }],
));
write("openapi.json", JSON.stringify({
  openapi: "3.1.0",
  info: { title: "agent-tokenomics documentation index", version: "1.0.0", description: `Public read-only documentation metadata. ${limits}` },
  servers: [{ url: root.replace(/\/$/, "") }],
  security: [],
  paths: { "/api/pages.json": { get: {
    operationId: "listDocumentationPages", summary: "List published documentation and Markdown URLs",
    responses: { "200": { description: "Published indexable pages", content: { "application/json": {
      schema: { type: "object", required: ["version", "pages"], additionalProperties: false, properties: {
        version: { type: "string", const: "1" },
        pages: { type: "array", items: {
          type: "object", required: [...Object.keys(pageProperties), "alternates"], additionalProperties: false,
          properties: { ...pageProperties, language: { enum: ["en", "ko"] }, alternates: { type: "object", additionalProperties: { type: "string", format: "uri" } } },
        } },
      } },
    } } } },
  } } },
}, null, 2) + "\n");
write(".well-known/api-catalog", JSON.stringify({ linkset: [{
  anchor: `${root}api/pages.json`,
  "service-desc": [{ href: `${root}openapi.json`, type: "application/openapi+json" }],
  "service-doc": [{ href: `${root}llms.txt`, type: "text/plain" }],
  describedby: [{ href: `${root}auth.md`, type: "text/markdown" }],
}] }, null, 2) + "\n");
write("auth.md", `# auth.md

## Authentication policy

Access is **anonymous**. Public documentation, Markdown, discovery files, and \`GET ${root}api/pages.json\` require no credentials. HEAD is also supported by the HTTP server.
There are no private data APIs, write operations, OAuth flows, bot accounts, account issuers, checkout, ACP, or AP2 interfaces.
Do not send credentials, private prompts, or customer data. Unsupported methods receive HTTP 405 from the HTTP server.

## Hosting and limitations

${limits}

The GitHub Pages project deployment serves static files under its project base path. It cannot provide Accept negotiation, custom HTTP Link headers, or domain-root discovery.
The optional Node HTTP server provides those features and root aliases. Deploy it behind HTTPS without overriding its cache-safety headers. See the repository deployment instructions.
`);
console.log(`[build-seo-geo] ${pages.length} rendered pages; sitemap, RSS, Markdown, corpus, and read-only discovery generated`);
