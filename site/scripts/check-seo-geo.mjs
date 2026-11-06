// check-seo-geo.mjs — Automated SEO & GEO verification gate for agent-tokenomics.
// Run after `npm --prefix site run build` via `npm --prefix site run check:seo`.

import { readFileSync, readdirSync, statSync, existsSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const siteDir = path.resolve(scriptDir, "..");
const repoRoot = path.resolve(siteDir, "..");
const distDir = path.join(siteDir, "dist");

const SITE_ORIGIN = (process.env.SITE_URL ?? "https://dotnetpower.github.io").replace(/\/$/, "");
const BASE_PATH = (process.env.BASE_PATH ?? "/agent-tokenomics").replace(/\/$/, "");
const SITE_ROOT = `${SITE_ORIGIN}${BASE_PATH}`;

const REQUIRED_BOTS = [
  "Googlebot",
  "bingbot",
  "Yeti",
  "DuckDuckBot",
  "OAI-SearchBot",
  "ChatGPT-User",
  "Claude-SearchBot",
  "Claude-User",
  "PerplexityBot",
  "GPTBot",
  "ClaudeBot",
  "Google-Extended",
  "Applebot-Extended",
  "CCBot",
];

function walkHtml(dir) {
  return readdirSync(dir)
    .sort()
    .flatMap((name) => {
      const full = path.join(dir, name);
      return statSync(full).isDirectory() ? walkHtml(full) : [full];
    })
    .filter((f) => f.endsWith(".html"));
}

function unescapeHtml(value = "") {
  return value.replace(
    /&#(?:x([0-9a-f]+)|(\d+));|&(amp|quot|lt|gt|apos);/gi,
    (_m, hex, dec, name) =>
      hex || dec
        ? String.fromCodePoint(Math.min(parseInt(hex || dec, hex ? 16 : 10), 0x10ffff))
        : { amp: "&", quot: '"', lt: "<", gt: ">", apos: "'" }[name.toLowerCase()],
  );
}

function extractTags(html, tagName) {
  const re = new RegExp(`<${tagName}\\b(?:[^"'<>]|"[^"]*"|'[^']*')*>`, "gi");
  return [...html.matchAll(re)].map((m) => {
    const attrs = {};
    for (const attr of m[0]
      .replace(/^<\w+/, "")
      .matchAll(/([^\s=/>]+)(?:\s*=\s*(?:"([^"]*)"|'([^']*)'|([^\s"'=<>`]+)))?/g)) {
      attrs[attr[1].toLowerCase()] = unescapeHtml(attr[2] ?? attr[3] ?? attr[4] ?? "");
    }
    return attrs;
  });
}

const failures = [];

if (!existsSync(distDir)) {
  console.error(`[check-seo-geo] dist directory not found: ${distDir}`);
  process.exit(1);
}

// 1. Check OG cover PNG
const ogCoverPath = path.join(distDir, "og-cover.png");
if (!existsSync(ogCoverPath)) {
  failures.push("missing_asset:og-cover.png");
} else {
  const sig = readFileSync(ogCoverPath).subarray(0, 8).toString("hex");
  if (sig !== "89504e470d0a1a0a") {
    failures.push(`invalid_png_signature:og-cover.png:${sig}`);
  }
}

// 2. Check robots.txt
const robotsPath = path.join(distDir, "robots.txt");
if (!existsSync(robotsPath)) {
  failures.push("missing_asset:robots.txt");
} else {
  const robotsText = readFileSync(robotsPath, "utf8");
  for (const bot of REQUIRED_BOTS) {
    if (!robotsText.includes(`User-agent: ${bot}`)) {
      failures.push(`robots_missing_bot:${bot}`);
    }
  }
  if (!robotsText.includes(`Sitemap: ${SITE_ROOT}/sitemap-index.xml`)) {
    failures.push("robots_missing_sitemap_index");
  }
  if (!robotsText.includes(`Sitemap: ${SITE_ROOT}/sitemap.xml`)) {
    failures.push("robots_missing_sitemap_xml");
  }
}

// 3. Check sitemaps & rss.xml
const sitemapIndexPath = path.join(distDir, "sitemap-index.xml");
const sitemap0Path = path.join(distDir, "sitemap-0.xml");
const sitemapXmlPath = path.join(distDir, "sitemap.xml");
const rssXmlPath = path.join(distDir, "rss.xml");
const sitemapUrls = new Set();
const rssUrls = new Set();

if (!existsSync(sitemapIndexPath)) failures.push("missing_asset:sitemap-index.xml");
if (!existsSync(sitemap0Path)) failures.push("missing_asset:sitemap-0.xml");
if (!existsSync(sitemapXmlPath)) {
  failures.push("missing_asset:sitemap.xml");
} else {
  const xml = readFileSync(sitemapXmlPath, "utf8");
  for (const m of xml.matchAll(/<loc>([^<]+)<\/loc>/g)) {
    sitemapUrls.add(m[1].trim());
  }
  if (!xml.includes("<lastmod>")) {
    failures.push("sitemap_missing_lastmod");
  }
}
if (!existsSync(rssXmlPath)) {
  failures.push("missing_asset:rss.xml");
} else {
  const rssText = readFileSync(rssXmlPath, "utf8");
  for (const m of rssText.matchAll(/<guid isPermaLink="true">([^<]+)<\/guid>/g)) {
    rssUrls.add(m[1].trim());
  }
}

// 4. Check llms.txt & llms-full.txt
const distLlmsPath = path.join(distDir, "llms.txt");
const distLlmsFullPath = path.join(distDir, "llms-full.txt");
const rootLlmsPath = path.join(repoRoot, "llms.txt");

if (!existsSync(distLlmsPath)) {
  failures.push("missing_asset:llms.txt");
} else if (existsSync(rootLlmsPath)) {
  if (readFileSync(distLlmsPath, "utf8") !== readFileSync(rootLlmsPath, "utf8")) {
    failures.push("llms_txt_out_of_sync_with_root");
  }
}

if (!existsSync(distLlmsFullPath)) {
  failures.push("missing_asset:llms-full.txt");
} else {
  const fullText = readFileSync(distLlmsFullPath, "utf8");
  if (fullText.split("\n").length < 500) {
    failures.push("llms_full_txt_too_short");
  }
}

function urlResolvesInDist(url) {
  if (!url.startsWith(`${SITE_ROOT}/`) && url !== SITE_ROOT) return true;
  const rel = url.slice(SITE_ROOT.length).replace(/^\/+/, "");
  if (!rel) return existsSync(path.join(distDir, "index.html"));
  const cleanRel = rel.split(/[?#]/)[0];
  if (cleanRel.endsWith("/")) {
    return existsSync(path.join(distDir, cleanRel, "index.html"));
  }
  return (
    existsSync(path.join(distDir, cleanRel)) ||
    existsSync(path.join(distDir, cleanRel, "index.html"))
  );
}

if (existsSync(distLlmsPath)) {
  const llmsText = readFileSync(distLlmsPath, "utf8");
  const urls = [
    ...[...llmsText.matchAll(/\]\((https:\/\/dotnetpower\.github\.io\/agent-tokenomics\/[^)\s]*)\)/g)].map(
      (m) => m[1],
    ),
    ...[...llmsText.matchAll(/<(https:\/\/dotnetpower\.github\.io\/agent-tokenomics\/[^>\s]*)>/g)].map(
      (m) => m[1],
    ),
  ];
  if (urls.length < 20) failures.push("llms_txt_too_few_links");
  for (const u of urls) {
    if (!urlResolvesInDist(u)) {
      failures.push(`llms_txt_broken_url:${u}`);
    }
  }
}

// 5. Inspect every built HTML page
const htmlFiles = walkHtml(distDir);
const seenTitles = new Map();
let indexableCount = 0;

for (const file of htmlFiles) {
  const rel = path.relative(distDir, file).split(path.sep).join("/");
  const html = readFileSync(file, "utf8");
  const is404 = rel === "404.html" || rel.endsWith("/404/index.html");
  const isDemo = rel.startsWith("demo/");
  const isHome = rel === "index.html" || rel === "ko/index.html";

  let expectedCanonical;
  if (rel === "index.html") {
    expectedCanonical = `${SITE_ROOT}/`;
  } else if (rel === "404.html") {
    expectedCanonical = `${SITE_ROOT}/404/`;
  } else if (rel.endsWith("/index.html")) {
    expectedCanonical = `${SITE_ROOT}/${rel.slice(0, -"/index.html".length)}/`;
  } else {
    expectedCanonical = `${SITE_ROOT}/${rel}`;
  }

  const headPos = html.search(/<head\b[^>]*>/i);
  const charsetPos = html.search(/<meta\s+charset=["']?utf-8["']?\s*\/?>/i);
  const titlePos = html.search(/<title\b[^>]*>/i);
  if (charsetPos < 0 || charsetPos > titlePos || (headPos >= 0 && charsetPos - headPos > 256)) {
    failures.push(`${rel}:charset_not_first_in_head`);
  }

  const titleMatches = [...html.matchAll(/<title\b[^>]*>([\s\S]*?)<\/title>/gi)];
  if (titleMatches.length !== 1) {
    failures.push(`${rel}:title_count:${titleMatches.length}`);
  }
  const title = unescapeHtml(titleMatches[0]?.[1] ?? "").replace(/\s+/g, " ").trim();
  if (!title) {
    failures.push(`${rel}:empty_title`);
  }

  const metas = extractTags(html, "meta");
  const links = extractTags(html, "link");
  const getMeta = (key) => metas.find((a) => (a.name || a.property) === key)?.content;

  const description = getMeta("description");
  if (!description || description.length < 10 || description.length > 160) {
    failures.push(`${rel}:invalid_description_length:${description?.length ?? 0}`);
  }
  if (!getMeta("google-site-verification")) {
    failures.push(`${rel}:missing_google_site_verification`);
  }

  const robots = getMeta("robots") ?? "";
  if (is404) {
    if (!/\bnoindex\b/i.test(robots)) failures.push(`${rel}:404_must_be_noindex`);
  } else {
    if (!/\bindex\b/i.test(robots) || !/\bfollow\b/i.test(robots)) {
      failures.push(`${rel}:missing_index_follow_robots`);
    }
    indexableCount++;
    const titleKey = title.toLowerCase();
    if (seenTitles.has(titleKey)) {
      failures.push(`${rel}:duplicate_title_with:${seenTitles.get(titleKey)}`);
    } else {
      seenTitles.set(titleKey, rel);
    }
  }

  const canonicals = links.filter((a) => a.rel?.toLowerCase() === "canonical");
  if (canonicals.length !== 1) {
    failures.push(`${rel}:canonical_count:${canonicals.length}`);
  } else if (canonicals[0].href !== expectedCanonical) {
    failures.push(`${rel}:canonical_mismatch:${canonicals[0].href}!=${expectedCanonical}`);
  }

  if (getMeta("og:title") !== title) failures.push(`${rel}:og_title_mismatch`);
  if (getMeta("og:description") !== description) failures.push(`${rel}:og_description_mismatch`);
  if (getMeta("og:url") !== expectedCanonical) failures.push(`${rel}:og_url_mismatch`);
  if (getMeta("og:image") !== `${SITE_ROOT}/og-cover.png`) failures.push(`${rel}:og_image_mismatch`);
  if (getMeta("twitter:card") !== "summary_large_image") failures.push(`${rel}:twitter_card_mismatch`);
  if (getMeta("twitter:title") !== title) failures.push(`${rel}:twitter_title_mismatch`);
  if (getMeta("twitter:description") !== description) failures.push(`${rel}:twitter_description_mismatch`);
  if (getMeta("twitter:image") !== `${SITE_ROOT}/og-cover.png`) failures.push(`${rel}:twitter_image_mismatch`);

  if (!is404) {
    if (!sitemapUrls.has(expectedCanonical)) {
      failures.push(`${rel}:missing_from_sitemap:${expectedCanonical}`);
    }
    if (!rssUrls.has(expectedCanonical)) {
      failures.push(`${rel}:missing_from_rss:${expectedCanonical}`);
    }
    if (!isDemo) {
      const xDefault = links.find((a) => a.rel === "alternate" && a.hreflang === "x-default");
      if (!xDefault?.href) {
        failures.push(`${rel}:missing_hreflang_x_default`);
      }
      const llmsLink = links.find((a) => a.rel === "alternate" && a.href === `${SITE_ROOT}/llms.txt`);
      if (!llmsLink) {
        failures.push(`${rel}:missing_llms_txt_alternate_link`);
      }
    }
  }

  if (!is404) {
    const ldScripts = [
      ...html.matchAll(/<script\b[^>]*type=["']application\/ld\+json["'][^>]*>([\s\S]*?)<\/script>/gi),
    ];
    if (ldScripts.length === 0) {
      failures.push(`${rel}:missing_json_ld`);
    } else {
      for (const [, rawJson] of ldScripts) {
        if (rawJson.includes("<")) {
          failures.push(`${rel}:unsafe_unescaped_lt_in_json_ld`);
        }
        try {
          const parsed = JSON.parse(rawJson);
          if (parsed["@context"] !== "https://schema.org" || !Array.isArray(parsed["@graph"])) {
            failures.push(`${rel}:invalid_json_ld_root`);
          } else {
            const types = new Set(parsed["@graph"].map((n) => n["@type"]));
            if (!types.has("Organization") || !types.has("WebSite") || !types.has("WebPage")) {
              failures.push(`${rel}:missing_core_json_ld_nodes`);
            }
            if (isHome && (!types.has("SoftwareApplication") || !types.has("FAQPage"))) {
              failures.push(`${rel}:missing_home_faq_or_software_json_ld`);
            }
            if (!isHome && !isDemo && (!types.has("TechArticle") || !types.has("BreadcrumbList"))) {
              failures.push(`${rel}:missing_article_or_breadcrumb_json_ld`);
            }
          }
        } catch (err) {
          failures.push(`${rel}:invalid_json_ld_parse:${err.message}`);
        }
      }
    }
  }
}

const summary = {
  html_files: htmlFiles.length,
  indexable_pages: indexableCount,
  sitemap_urls: sitemapUrls.size,
  rss_items: rssUrls.size,
  failures,
  passed: failures.length === 0,
};

console.log(JSON.stringify(summary, null, 2));
if (!summary.passed) {
  process.exit(1);
}
