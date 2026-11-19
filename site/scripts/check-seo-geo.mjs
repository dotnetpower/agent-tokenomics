import { readFileSync, existsSync } from "node:fs";
import { pathToFileURL } from "node:url";
import path from "node:path";
import { load } from "cheerio";
import sharp from "sharp";
import { dateProblems } from "./check-dates.mjs";
import {
  distDir, root, origin, base, walk, canonicalFor, localFile, searchBots, trainingBots, indexNowKey,
} from "./site-contract.mjs";

export async function inspectBuild() {
  const errors = [];
  const check = (condition, message) => { if (!condition) errors.push(message); };
  const json = (rel) => JSON.parse(readFileSync(path.join(distDir, rel), "utf8"));
  const pages = new Map();
  const documents = new Map();
  const titles = new Map();
  const descriptions = new Set();
  const socialImages = new Map();
  for (const file of walk(distDir, ".html")) {
    const $ = load(readFileSync(file, "utf8"));
    const url = canonicalFor(file);
    documents.set(url, $);
    const meta = (key) => $(`meta[name="${key}"],meta[property="${key}"]`);
    const get = (key) => meta(key).attr("content");
    if (/(^|\/)404(?:\.html|\/)$/.test(url)) {
      check(/\bnoindex\b/.test(get("robots") ?? ""), `${url}: 404 must be noindex`);
      continue;
    }
    if (/\bnoindex\b/.test(get("robots") ?? "")) continue;
    check(!!get("google-site-verification"), `${url}: Google verification token missing`);
    if (process.env.PUBLIC_NAVER_SITE_VERIFICATION && !url.includes("/demo/"))
      check(get("naver-site-verification") === process.env.PUBLIC_NAVER_SITE_VERIFICATION, `${url}: Naver verification token not preserved`);
    pages.set(url, $);
    const title = $("title").text().trim();
    const description = get("description");
    check($("title").length === 1 && !!title, `${url}: expected one nonempty title`);
    check(!titles.has(title.toLowerCase()), `${url}: duplicate title: ${title}`);
    titles.set(title.toLowerCase(), url);
    check(meta("description").length === 1 && !!description && [...description].length <= 160, `${url}: description must be 1..160 characters`);
    check(!descriptions.has(description), `${url}: duplicate description`);
    descriptions.add(description);
    check($("h1").length === 1 && !!$("h1").text().trim(), `${url}: expected one nonempty h1`);
    if (url === root || url === `${root}ko/`)
      check($(".answer-capsule dl, dl.answer-capsule").length > 0, `${url}: initial HTML summary missing`);
    else if (!url.includes("/demo/"))
      check($(".answer-capsule").length === 0, `${url}: generic article summary must not be injected`);
    if (!url.includes("/demo/")) {
      const created = get("date.created");
      const updated = get("date.updated");
      const dateBox = $(".document-dates");
      for (const error of dateProblems(`---\ncreated: ${created}\nupdated: ${updated}\ndateSource: git\n---\n`))
        errors.push(`${url}: ${error}`);
      check(dateBox.length === 1, `${url}: expected one visible document date block`);
      for (const value of [created, updated])
        check(dateBox.find("time").toArray().some((el) => $(el).attr("datetime") === value && $(el).text() === value),
          `${url}: visible date mismatch: ${value}`);
    }
    check($('link[rel="canonical"]').length === 1 && $('link[rel="canonical"]').attr("href") === url, `${url}: canonical must be self-referencing`);
    for (const [key, expected] of Object.entries({
      "og:title": title, "twitter:title": title,
      "og:description": description, "twitter:description": description,
      "og:url": url, "twitter:card": "summary_large_image",
      "twitter:image": get("og:image"), "og:image:width": "1200",
      "og:image:height": "630",
    })) {
      check(meta(key).length === 1 && get(key) === expected && !!expected, `${url}: mismatched/missing ${key}`);
    }
    check(["image/png", "image/webp"].includes(get("og:image:type")), `${url}: social image must be PNG/WebP`);
    const image = get("og:image");
    if (!socialImages.has(image)) socialImages.set(image, new Set());
    socialImages.get(image).add(get("og:image:type"));
    $("img").each((_i, image) => {
      check(!!$(image).attr("alt")?.trim(), `${url}: empty image alt: ${$(image).attr("src")}`);
    });
    for (const match of readFileSync(file, "utf8").matchAll(/<img\b[^>]*\balt="([^"]*)"/gi)) {
      check(!/[<>]/.test(match[1]), `${url}: unescaped angle bracket in image alt`);
    }
    const scripts = $('script[type="application/ld+json"]');
    const visibleBody = $("body").clone();
    visibleBody.find("script,style,template,nav,footer,[hidden]").remove();
    const visibleText = visibleBody.text().replace(/\s+/g, " ");
    check(scripts.length > 0, `${url}: missing JSON-LD`);
    scripts.each((_i, element) => {
      try {
        const graph = JSON.parse($(element).text());
        check(graph["@context"] === "https://schema.org" && Array.isArray(graph["@graph"]), `${url}: JSON-LD must use @graph`);
        const nodes = graph["@graph"] ?? [];
        for (const type of ["Organization", "WebSite", "WebPage"])
          check(nodes.some((node) => node["@type"] === type), `${url}: missing ${type}`);
        const page = nodes.find((node) => node["@type"] === "WebPage");
        check(page?.url === url && page?.["@id"] === url, `${url}: WebPage URL/@id mismatch`);
        if (!url.includes("/demo/")) {
          for (const node of nodes.filter((node) => ["WebPage", "TechArticle"].includes(node["@type"])))
            check(node.datePublished === get("date.created") && node.dateModified === get("date.updated"),
              `${url}: structured dates differ from visible dates`);
        }
        check(!nodes.some((node) => node.aggregateRating || node.offers || ["Product", "Place"].includes(node["@type"])), `${url}: unsupported commercial/rating claims`);
        for (const faq of nodes.filter((node) => node["@type"] === "FAQPage")) {
          for (const question of faq.mainEntity ?? []) {
            check(visibleText.includes(question.name) && visibleText.includes(question.acceptedAnswer?.text), `${url}: FAQ schema is not visible content`);
          }
        }
      } catch (error) {
        errors.push(`${url}: JSON-LD parse error: ${error.message}`);
      }
    });
    for (const name of ["llms.txt", "llms-full.txt"])
      check($(`link[rel="alternate"][href="${root}${name}"]`).length === 1, `${url}: missing ${name} head link`);
  }
  for (const [url, $] of documents) {
    $("a[href], link[href], img[src], script[src]").each((_i, element) => {
      const href = $(element).attr("href") ?? $(element).attr("src");
      if (!href || /^(mailto:|tel:|data:|javascript:)/i.test(href)) return;
      const target = new URL(href, url);
      if (target.origin !== origin || !target.pathname.startsWith(`${base}/`)) return;
      const file = localFile(target.href);
      check(!!file && existsSync(file), `${url}: broken internal URL: ${href}`);
      if (file && existsSync(path.join(file, "index.html")))
        errors.push(`${url}: redirecting directory link: ${href}`);
      if (target.hash && documents.has(`${target.origin}${target.pathname}`)) {
        const destination = documents.get(`${target.origin}${target.pathname}`);
        const fragment = decodeURIComponent(target.hash.slice(1));
        check(destination("[id],a[name]").toArray().some((el) =>
          destination(el).attr("id") === fragment || destination(el).attr("name") === fragment,
        ), `${url}: missing anchor: ${href}`);
      }
    });
  }
  const readXml = (name) => load(readFileSync(path.join(distDir, name), "utf8"), { xml: true });
  const sitemap = readXml("sitemap.xml");
  const checkSitemapDates = (xml) => xml("url").each((_i, element) => {
    const url = xml(element).find("loc").text();
    const updated = pages.get(url)?.('meta[name="date.updated"]').attr("content");
    if (updated) check(xml(element).find("lastmod").text() === updated, `${url}: sitemap date mismatch`);
  });
  checkSitemapDates(sitemap);
  let sitemapUrls = sitemap("url > loc").map((_i, el) => sitemap(el).text()).get();
  if (sitemap("sitemapindex").length) {
    sitemapUrls = sitemap("sitemap > loc").map((_i, el) => sitemap(el).text()).get().flatMap((url) => {
      const shard = load(readFileSync(localFile(url), "utf8"), { xml: true });
      checkSitemapDates(shard);
      check(shard("url").length <= 4000, `${url}: more than 4000 URLs`);
      return shard("url > loc").map((_i, el) => shard(el).text()).get();
    });
  }
  const sameSet = (actual, expected, label) => {
    check(new Set(actual).size === actual.length, `${label}: duplicate URLs`);
    for (const url of expected) check(actual.includes(url), `${label}: missing ${url}`);
    for (const url of actual) check(expected.includes(url), `${label}: noncanonical ${url}`);
  };
  sameSet(sitemapUrls, [...pages.keys()], "sitemap");
  const rss = readXml("rss.xml");
  rss("item").each((_i, element) => {
    const url = rss(element).find("link").text();
    const created = pages.get(url)?.('meta[name="date.created"]').attr("content");
    if (created) check(rss(element).find("pubDate").text() === new Date(`${created}T00:00:00Z`).toUTCString(),
      `${url}: RSS publication date mismatch`);
  });
  sameSet(rss("item > link").map((_i, el) => rss(el).text()).get(), [...pages.keys()], "RSS");
  check(new Set(rss("item > title").map((_i, el) => rss(el).text()).get()).size === pages.size, "RSS: titles are not unique");
  check(new Set(rss("item > description").map((_i, el) => rss(el).text()).get()).size === pages.size, "RSS: summaries are not unique");
  for (const [url, $] of pages) {
    const alternates = $('link[rel="alternate"][hreflang]');
    if (!alternates.length) continue; // A standalone report has no invented translation.
    const links = Object.fromEntries(alternates.toArray().map((el) => [$(el).attr("hreflang"), $(el).attr("href")]));
    check(Object.keys(links).sort().join() === "en,ko,x-default", `${url}: expected en/ko/x-default`);
    check(links.en === links["x-default"], `${url}: x-default must target English`);
    for (const [lang, href] of Object.entries(links)) {
      const target = pages.get(href);
      check(!!target, `${url}: nonexistent translation ${href}`);
      check(target?.(`link[hreflang="${lang}"]`).attr("href") === href, `${url}: alternate is not self-referencing`);
      check(target?.(`link[hreflang="${$("html").attr("lang")?.startsWith("ko") ? "ko" : "en"}"]`).attr("href") === url, `${url}: alternate not reciprocal`);
    }
  }
  for (const [url, declaredTypes] of socialImages) {
    const file = url && localFile(url);
    check(file && existsSync(file), `social image not found: ${url}`);
    if (!file || !existsSync(file)) continue;
    const buffer = readFileSync(file);
    const isPng = buffer.subarray(0, 8).toString("hex") === "89504e470d0a1a0a";
    const isWebp = buffer.toString("ascii", 0, 4) === "RIFF" && buffer.toString("ascii", 8, 12) === "WEBP";
    check(isPng || isWebp, `invalid social image magic bytes: ${url}`);
    check(declaredTypes.size === 1 && declaredTypes.has(isPng ? "image/png" : "image/webp"), `social image declared MIME does not match bytes: ${url}`);
    const metadata = await sharp(buffer).metadata();
    check(metadata.width === 1200 && metadata.height === 630, `social image dimensions: ${url}`);
    await sharp(buffer).raw().toBuffer(); // Decode the entire image, not just its signature.
  }
  const robots = readFileSync(path.join(distDir, "robots.txt"), "utf8");
  for (const bot of searchBots) check(robots.includes(`User-agent: ${bot}\nAllow: /`), `robots disallows/misses ${bot}`);
  for (const bot of trainingBots) check(robots.includes(`User-agent: ${bot}\nDisallow: /`), `robots training policy missing ${bot}`);
  check(robots.includes(`Sitemap: ${root}sitemap.xml`), "robots: canonical sitemap missing");
  check(readFileSync(path.join(distDir, `${indexNowKey}.txt`), "utf8").trim() === indexNowKey, "IndexNow key mismatch");
  const api = json("api/pages.json");
  sameSet(api.pages.map((page) => page.url), [...pages.keys()], "API");
  for (const page of api.pages) {
    const text = readFileSync(localFile(page.markdown), "utf8");
    check(text.startsWith(`# ${page.title}\n`) && text.includes(page.url), `Markdown identity mismatch: ${page.url}`);
    if (!page.url.includes("/demo/")) {
      const html = pages.get(page.url);
      check(page.created === html('meta[name="date.created"]').attr("content") &&
        page.updated === html('meta[name="date.updated"]').attr("content"), `API dates mismatch: ${page.url}`);
      check(text.includes(page.created) && text.includes(page.updated), `Markdown dates missing: ${page.url}`);
    }
    check(!/<(script|style|nav|footer)\b/i.test(text), `Markdown contains chrome: ${page.url}`);
    check(readFileSync(path.join(distDir, "llms-full.txt"), "utf8").includes(text), `corpus missing ${page.url}`);
  }
  const openapi = json("openapi.json");
  check(openapi.openapi === "3.1.0" && openapi.servers?.[0]?.url === root.replace(/\/$/, ""), "OpenAPI: version/server mismatch");
  check(JSON.stringify(Object.keys(openapi.paths)) === '["/api/pages.json"]' &&
    JSON.stringify(Object.keys(openapi.paths["/api/pages.json"])) === '["get"]' &&
    openapi.security?.length === 0 && !openapi.components?.securitySchemes, "OpenAPI must describe only the anonymous GET endpoint");
  const catalog = json(".well-known/api-catalog");
  check(catalog.linkset?.[0]?.anchor === `${root}api/pages.json`, "catalog: real API anchor missing");
  check(catalog.linkset?.[0]?.["service-desc"]?.[0]?.href === `${root}openapi.json`, "catalog: OpenAPI link missing");
  const auth = readFileSync(path.join(distDir, "auth.md"), "utf8");
  check(auth.startsWith("# auth.md\n") && auth.includes("anonymous") && auth.includes("GET"), "auth.md: anonymous GET policy missing");
  const llms = readFileSync(path.join(distDir, "llms.txt"), "utf8");
  check(llms.startsWith("# ") && llms.includes("## Sources and limitations"), "llms.txt: sources/limitations missing");
  for (const match of llms.matchAll(/https?:\/\/[^\s<>)`]+/g)) {
    const file = localFile(match[0]);
    if (file) check(existsSync(file), `llms.txt: broken link ${match[0]}`);
  }
  return { errors, pages: pages.size, html: documents.size };
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const result = await inspectBuild();
  console.log(`[check-seo-geo] ${result.pages} indexable pages; ${result.html} HTML files; ${result.errors.length} errors`);
  if (result.errors.length) {
    console.error(result.errors.slice(0, 80).join("\n"));
    process.exitCode = 1;
  }
}
