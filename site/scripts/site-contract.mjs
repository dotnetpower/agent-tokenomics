import { readdirSync, statSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

export const siteDir = fileURLToPath(new URL("../", import.meta.url));
export const distDir = path.join(siteDir, "dist");
export const origin = new URL(process.env.SITE_URL ?? "https://dotnetpower.github.io").origin;
export const base = `/${(process.env.BASE_PATH ?? "/agent-tokenomics").replace(/^\/|\/$/g, "")}`.replace(/\/$/, "");
export const root = `${origin}${base}/`;
export const indexNowKey = "4f8b9c2d1e6a3b7c8d9e0f1a2b3c4d5e";
export const searchBots = ["Googlebot", "bingbot", "Yeti", "DuckDuckBot", "OAI-SearchBot", "Claude-SearchBot", "PerplexityBot"];
export const trainingBots = ["GPTBot", "ClaudeBot", "Google-Extended", "Applebot-Extended", "CCBot", "Claude-Web", "anthropic-ai", "cohere-ai", "Bytespider"];
export const limits = "Guidance is not a savings guarantee. Pricing and product behavior change; check each page's sources and verification date. Third-party benchmarks are reported, not independently verified here. Measure cost alongside task success. Examples are illustrative, not customer data.";
export const discoveryLink = [
  '</.well-known/api-catalog>; rel="api-catalog"; type="application/linkset+json"',
  '</openapi.json>; rel="service-desc"; type="application/openapi+json"',
  '</llms.txt>; rel="service-doc"; type="text/plain"',
  '</auth.md>; rel="describedby"; type="text/markdown"',
].join(", ");

export function walk(dir, extension) {
  return readdirSync(dir).sort().flatMap((name) => {
    const full = path.join(dir, name);
    return statSync(full).isDirectory() ? walk(full, extension) : full.endsWith(extension) ? [full] : [];
  });
}

export function canonicalFor(file) {
  const rel = path.relative(distDir, file).split(path.sep).join("/");
  return `${root}${rel === "index.html" ? "" : rel.replace(/index\.html$/, "")}`;
}

export function localFile(url) {
  const parsed = new URL(url, root);
  if (parsed.origin !== origin || !parsed.pathname.startsWith(`${base}/`)) return undefined;
  const rel = decodeURIComponent(parsed.pathname.slice(base.length + 1));
  const resolved = path.resolve(distDir, rel.endsWith("/") || !rel ? `${rel}index.html` : rel);
  return resolved.startsWith(`${distDir}${path.sep}`) ? resolved : undefined;
}

export function xmlEscape(text) {
  return String(text).replace(/[&<>"']/g, (char) => ({
    "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&apos;",
  })[char]);
}

export function robotsText() {
  return `# Search/retrieval allowed; training-only crawlers disallowed (voluntary policy).\nUser-agent: *\nAllow: /\n\n` +
    [...searchBots, "ChatGPT-User", "Claude-User", "Perplexity-User"].map((bot) => `User-agent: ${bot}\nAllow: /\n`).join("\n") +
    "\n# Training is separate from search. This is not access control.\n" +
    trainingBots.map((bot) => `User-agent: ${bot}\nDisallow: /\n`).join("\n") +
    `\nSitemap: ${root}sitemap.xml\n`;
}
