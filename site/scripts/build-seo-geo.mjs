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

const sections = [header];

for (const { fullPath, relPath } of orderedDocs) {
  const raw = readFileSync(fullPath, "utf8");
  const { meta, body } = parseFrontmatterAndBody(raw);
  const cleaned = cleanMdxBody(body);
  const url = docCanonicalUrl(relPath);
  const title = meta.title || relPath;
  const description = meta.description ? `\n> ${meta.description}\n` : "";

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

// 3. Normalize sitemap-0.xml (trailing slash + hreflang on root/ko homepages + demo report) and emit dist/sitemap.xml
if (existsSync(distDir)) {
  const sitemap0 = path.join(distDir, "sitemap-0.xml");
  const sitemapAlias = path.join(distDir, "sitemap.xml");
  if (existsSync(sitemap0)) {
    let xml = readFileSync(sitemap0, "utf8");
    const homeAlternates = `<xhtml:link rel="alternate" hreflang="en" href="${BASE_URL}/"/><xhtml:link rel="alternate" hreflang="ko" href="${BASE_URL}/ko/"/>`;
    xml = xml.replace(
      `<url><loc>${BASE_URL}</loc></url>`,
      `<url><loc>${BASE_URL}/</loc>${homeAlternates}</url>`,
    );
    xml = xml.replace(
      `<url><loc>${BASE_URL}/ko/</loc></url>`,
      `<url><loc>${BASE_URL}/ko/</loc>${homeAlternates}</url>`,
    );
    const demoUrl = `${BASE_URL}/demo/tokprof-sample.html`;
    if (!xml.includes(`<loc>${demoUrl}</loc>`)) {
      xml = xml.replace("</urlset>", `<url><loc>${demoUrl}</loc></url></urlset>`);
    }
    writeFileSync(sitemap0, xml, "utf8");
    writeFileSync(sitemapAlias, xml, "utf8");
  }
}

console.log(
  `[build-seo-geo] synced llms.txt and generated llms-full.txt (${orderedDocs.length} pages, ${
    fullCorpus.split("\n").length
  } lines)`,
);
