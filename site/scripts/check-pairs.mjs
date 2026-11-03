// Fails when an English page has no Korean twin under ko/ (or vice versa).
import { readdirSync, statSync } from "node:fs";
import { join, relative } from "node:path";

const root = new URL("../src/content/docs/", import.meta.url).pathname;
const walk = (dir) =>
  readdirSync(dir).flatMap((n) => {
    const p = join(dir, n);
    return statSync(p).isDirectory() ? walk(p) : [p];
  });
const isDoc = (p) => /\.(md|mdx)$/.test(p);
const all = walk(root).filter(isDoc).map((p) => relative(root, p));
const ko = new Set(all.filter((p) => p.startsWith("ko/")).map((p) => p.slice(3)));
const en = new Set(all.filter((p) => !p.startsWith("ko/")));
const problems = [
  ...[...en].filter((p) => !ko.has(p)).map((p) => `missing ko/${p}`),
  ...[...ko].filter((p) => !en.has(p)).map((p) => `missing ${p} (en)`),
];
// Informational until the bulk Korean translation lands; pass --strict to fail.
if (problems.length) {
  console.warn(problems.join("\n"));
  if (process.argv.includes("--strict")) process.exit(1);
}
console.log(`${en.size} English pages, ${ko.size} Korean pages`);
