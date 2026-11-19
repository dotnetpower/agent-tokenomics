import { readFileSync } from "node:fs";
import path from "node:path";
import { pathToFileURL } from "node:url";
import { distDir, root, indexNowKey } from "./site-contract.mjs";

export function indexNowPayloads(urls, siteRoot = root) {
  const site = new URL(siteRoot);
  if (!urls.length || urls.some((url) => new URL(url).origin !== site.origin || !url.startsWith(site.href)))
    throw new Error("IndexNow URLs must be nonempty and belong to this site's canonical base path");
  return Array.from({ length: Math.ceil(urls.length / 10000) }, (_, index) => ({
    host: site.hostname, key: indexNowKey, keyLocation: `${site.href}${indexNowKey}.txt`,
    urlList: urls.slice(index * 10000, (index + 1) * 10000),
  }));
}

export async function submitIndex({ urls, token, property = root, fetcher = fetch }) {
  if (!token) throw new Error("GSC_ACCESS_TOKEN is required; no search engine submissions were performed");
  const payloads = indexNowPayloads(urls);
  const request = (url, options) => fetcher(url, { ...options, signal: AbortSignal.timeout(30000) });
  const verification = await request(`${root}${indexNowKey}.txt`);
  if (verification.status !== 200 || (await verification.text()).trim() !== indexNowKey)
    throw new Error("The deployed IndexNow key does not match; refusing to submit unpublished URLs");
  for (const payload of payloads) {
    const response = await request("https://api.indexnow.org/IndexNow", {
      method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify(payload),
    });
    if (![200, 202].includes(response.status)) throw new Error(`IndexNow submission failed: HTTP ${response.status}`);
  }
  const response = await request(`https://www.googleapis.com/webmasters/v3/sites/${encodeURIComponent(property)}/sitemaps/${encodeURIComponent(`${root}sitemap.xml`)}`, {
    method: "PUT", headers: { Authorization: `Bearer ${token}` },
  });
  if (!response.ok) throw new Error(`GSC sitemap submission failed: HTTP ${response.status}`);
  console.log("[submit-index] IndexNow accepted and GSC sitemap submitted; indexing is not guaranteed");
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const { pages } = JSON.parse(readFileSync(path.join(distDir, "api/pages.json"), "utf8"));
  await submitIndex({ urls: pages.map((page) => page.url), token: process.env.GSC_ACCESS_TOKEN, property: process.env.GSC_SITE_URL || root });
}
