// build-seo-geo.mjs — Post-build SEO & GEO artifact generator for agent-tokenomics.
//
// 1. Keeps `site/public/llms.txt` and `site/dist/llms.txt` synced with root `llms.txt`.
// 2. Generates `site/dist/llms-full.txt` (and `site/public/llms-full.txt`) by concatenating
//    every published English and Korean documentation page in deterministic order with
//    canonical URLs and frontmatter descriptions for one-shot generative-engine ingestion.
// 3. Emits `site/dist/sitemap.xml` (copied from `sitemap-0.xml` when present) alongside
//    `sitemap-index.xml` so crawlers requesting `/sitemap.xml` or `/sitemap-index.xml` both succeed.

import { readFileSync, writeFileSync, readdirSync, statSync, existsSync, copyFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const siteDir = path.resolve(scriptDir, "..");
const repoRoot = path.resolve(siteDir, "..");
const docsRoot = path.join(siteDir, "src", "content", "docs");
const publicDir = path.join(siteDir, "public");
const distDir = path.join(siteDir, "dist");

const SITE_URL = (process.env.SITE_URL ?? "https://dotnetpower.github.io").replace(/\/$/, "");
const BASE_PATH = (process.env.BASE_PATH ?? "/agent-tokenomics").replace(/\/$/, "");
const BASE_URL = `${SITE_URL}${BASE_PATH}`;

function walkDocs(dir) {
  return readdirSync(dir)
    .sort()
    .flatMap((name) => {
      const full = path.join(dir, name);
      return statSync(full).isDirectory() ? walkDocs(full) : [full];
    })
    .filter((f) => /\.(md|mdx)$/.test(f));
}

function parseFrontmatterAndBody(raw) {
  const match = raw.match(/^---\r?\n([\s\S]*?)\r?\n---\r?\n?([\s\S]*)$/);
  if (!match) return { meta: {}, body: raw.trim() };
  const [, fmText, body] = match;
  const meta = {};
  for (const line of fmText.split(/\r?\n/)) {
    const m = line.match(/^([a-zA-Z0-9_-]+):\s*(.+)$/);
    if (m) {
      meta[m[1]] = m[2].replace(/^["']|["']$/g, "").trim();
    }
  }
  return { meta, body: body.trim() };
}

function cleanMdxBody(body) {
  return body
    .replace(/^import\s+[^\n]+\n?/gm, "")
    .replace(/<LandingPage[^>]*\/>/g, "")
    .trim();
}

function docCanonicalUrl(relPath) {
  let slug = relPath.replace(/\.(md|mdx)$/, "");
  if (slug === "index") return `${BASE_URL}/`;
  if (slug.endsWith("/index")) slug = slug.slice(0, -"/index".length);
  return `${BASE_URL}/${slug}/`;
}

// 1. Sync llms.txt from repo root to site/public/llms.txt and site/dist/llms.txt
const rootLlmsPath = path.join(repoRoot, "llms.txt");
const publicLlmsPath = path.join(publicDir, "llms.txt");
if (existsSync(rootLlmsPath)) {
  const llmsContent = readFileSync(rootLlmsPath, "utf8");
  writeFileSync(publicLlmsPath, llmsContent, "utf8");
  if (existsSync(distDir)) {
    writeFileSync(path.join(distDir, "llms.txt"), llmsContent, "utf8");
  }
}

// 2. Build llms-full.txt corpus (English pages first, then Korean pages)
const allDocFiles = walkDocs(docsRoot).map((f) => ({
  fullPath: f,
  relPath: path.relative(docsRoot, f).split(path.sep).join("/"),
}));

const enDocs = allDocFiles.filter((d) => !d.relPath.startsWith("ko/"));
const koDocs = allDocFiles.filter((d) => d.relPath.startsWith("ko/"));
const orderedDocs = [...enDocs, ...koDocs];

const header = `# agent-tokenomics — Full Documentation Corpus (English & Korean)

Source: ${BASE_URL}/
Repository: https://github.com/dotnetpower/agent-tokenomics
Structured Index: ${BASE_URL}/llms.txt
Generated for generative-engine ingestion (one file, one fetch).
Each section below is one published documentation page separated by \`---\` rulers.
`;

function xmlEscape(value) {
  return String(value ?? "")
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&apos;");
}

const sections = [header];
const rssItems = [];
const BUILD_ISO = "2026-11-06T03:00:00Z";
const BUILD_PUBDATE = new Date(BUILD_ISO).toUTCString();
const INDEXNOW_KEY = "4f8b9c2d1e6a3b7c8d9e0f1a2b3c4d5e";

// Write IndexNow verification key file to public/ and dist/
writeFileSync(path.join(publicDir, `${INDEXNOW_KEY}.txt`), `${INDEXNOW_KEY}\n`, "utf8");
if (existsSync(distDir)) {
  writeFileSync(path.join(distDir, `${INDEXNOW_KEY}.txt`), `${INDEXNOW_KEY}\n`, "utf8");
}

for (const { fullPath, relPath } of orderedDocs) {
  const raw = readFileSync(fullPath, "utf8");
  const { meta, body } = parseFrontmatterAndBody(raw);
  const cleaned = cleanMdxBody(body);
  const url = docCanonicalUrl(relPath);
  const title = meta.title || relPath;
  const descText =
    meta.description ||
    "Tokenomics and token economics guide for LLMs & AI agents: cut token costs in VS Code Copilot, speed up coding with cctx, and profile calls with tokprof.";
  const description = meta.description ? `\n> ${meta.description}\n` : "";

  rssItems.push(
    `    <item>\n` +
      `      <title>${xmlEscape(title)}</title>\n` +
      `      <link>${xmlEscape(url)}</link>\n` +
      `      <guid isPermaLink="true">${xmlEscape(url)}</guid>\n` +
      `      <description>${xmlEscape(descText)}</description>\n` +
      `      <pubDate>${BUILD_PUBDATE}</pubDate>\n` +
      `    </item>`,
  );

  sections.push(
    `\n---\n\n## ${title} (${relPath})\n\nCanonical URL: <${url}>${description}\n${
      cleaned || "(Interactive landing page — see structured index in llms.txt and Track A / Track B sections below.)"
    }\n`,
  );
}

const fullCorpus = sections.join("");
if (existsSync(distDir)) {
  writeFileSync(path.join(distDir, "llms-full.txt"), fullCorpus, "utf8");
}

// 3. Generate clean, standards-compliant sitemap-0.xml, sitemap.xml, and sitemap-index.xml
//    with full ISO-8601 <lastmod> and reciprocal en / ko / x-default hreflang links.
if (existsSync(distDir)) {
  const sitemap0 = path.join(distDir, "sitemap-0.xml");
  const sitemapAlias = path.join(distDir, "sitemap.xml");
  const sitemapIndex = path.join(distDir, "sitemap-index.xml");
  const demoUrl = `${BASE_URL}/demo/tokprof-sample.html`;

  const urlNodes = [];
  for (const { relPath } of orderedDocs) {
    const loc = docCanonicalUrl(relPath);
    const baseRel = relPath.startsWith("ko/") ? relPath.slice(3) : relPath;
    const enHref = docCanonicalUrl(baseRel);
    const koHref = docCanonicalUrl(`ko/${baseRel}`);
    urlNodes.push(
      `  <url>\n` +
        `    <loc>${xmlEscape(loc)}</loc>\n` +
        `    <lastmod>${BUILD_ISO}</lastmod>\n` +
        `    <xhtml:link rel="alternate" hreflang="en" href="${xmlEscape(enHref)}"/>\n` +
        `    <xhtml:link rel="alternate" hreflang="ko" href="${xmlEscape(koHref)}"/>\n` +
        `    <xhtml:link rel="alternate" hreflang="x-default" href="${xmlEscape(enHref)}"/>\n` +
        `  </url>`,
    );
  }
  urlNodes.push(
    `  <url>\n` +
      `    <loc>${xmlEscape(demoUrl)}</loc>\n` +
      `    <lastmod>${BUILD_ISO}</lastmod>\n` +
      `  </url>`,
  );

  const sitemapXml =
    `<?xml version="1.0" encoding="UTF-8"?>\n` +
    `<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9" xmlns:xhtml="http://www.w3.org/1999/xhtml">\n` +
    `${urlNodes.join("\n")}\n` +
    `</urlset>\n`;

  writeFileSync(sitemap0, sitemapXml, "utf8");
  writeFileSync(sitemapAlias, sitemapXml, "utf8");

  const idxXml =
    `<?xml version="1.0" encoding="UTF-8"?>\n` +
    `<sitemapindex xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">\n` +
    `  <sitemap>\n` +
    `    <loc>${BASE_URL}/sitemap-0.xml</loc>\n` +
    `    <lastmod>${BUILD_ISO}</lastmod>\n` +
    `  </sitemap>\n` +
    `</sitemapindex>\n`;
  writeFileSync(sitemapIndex, idxXml, "utf8");

  // 4. Generate dist/rss.xml for search engine & AI feed aggregators
  rssItems.push(
    `    <item>\n` +
      `      <title>${xmlEscape("tokprof Edition Report Sample — Interactive LLM Token Profile")}</title>\n` +
      `      <link>${xmlEscape(demoUrl)}</link>\n` +
      `      <guid isPermaLink="true">${xmlEscape(demoUrl)}</guid>\n` +
      `      <description>${xmlEscape(
        "Interactive tokprof HTML Edition Dossier sample: per-session, per-turn, and per-call LLM input token breakdown, prompt inspector, and waste findings.",
      )}</description>\n` +
      `      <pubDate>${BUILD_PUBDATE}</pubDate>\n` +
      `    </item>`,
  );

  const rssXml =
    `<?xml version="1.0" encoding="UTF-8"?>\n` +
    `<rss version="2.0" xmlns:atom="http://www.w3.org/2005/Atom">\n` +
    `  <channel>\n` +
    `    <title>agent-tokenomics — Tokenomics &amp; Token Economics for LLMs &amp; AI Agents</title>\n` +
    `    <link>${BASE_URL}/</link>\n` +
    `    <description>Tokenomics and token economics guide for LLMs &amp; AI agents: cut token costs in VS Code Copilot, speed up coding with cctx, and profile calls with tokprof.</description>\n` +
    `    <language>en</language>\n` +
    `    <lastBuildDate>${BUILD_PUBDATE}</lastBuildDate>\n` +
    `    <atom:link href="${BASE_URL}/rss.xml" rel="self" type="application/rss+xml"/>\n` +
    `${rssItems.join("\n")}\n` +
    `  </channel>\n` +
    `</rss>\n`;
  writeFileSync(path.join(distDir, "rss.xml"), rssXml, "utf8");
}

console.log(
  `[build-seo-geo] synced llms.txt and generated llms-full.txt (${orderedDocs.length} pages, ${
    fullCorpus.split("\n").length
  } lines)`,
);
