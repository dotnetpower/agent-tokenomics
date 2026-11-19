import { readdirSync, readFileSync } from "node:fs";
import { join, relative } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

export function dateProblems(text, today = new Date().toISOString().slice(0, 10)) {
  const frontmatter = text.match(/^---\r?\n([\s\S]*?)\r?\n---(?:\r?\n|$)/)?.[1] ?? "";
  const fields = Object.fromEntries([...frontmatter.matchAll(/^(created|updated|verified|dateSource):[ \t]*([^\r\n]*)$/gm)]
    .map(([, key, value]) => [key, value.trim().replace(/^(['"])(.*)\1$/, "$2")]));
  const errors = [];
  for (const key of ["created", "updated", ...(fields.verified !== undefined ? ["verified"] : [])]) {
    const value = fields[key];
    if (!/^\d{4}-\d{2}-\d{2}$/.test(value ?? "") ||
        !Number.isFinite(Date.parse(value)) || new Date(value).toISOString().slice(0, 10) !== value) {
      errors.push(`${key}: expected a real YYYY-MM-DD date`);
    } else if (value > today) {
      errors.push(`${key}: must not be in the future`);
    }
  }
  if (fields.created > fields.updated) errors.push("updated: must not precede created");
  if (!["git", "recorded"].includes(fields.dateSource)) errors.push("dateSource: expected git or recorded");
  return errors;
}

export function inspectDates(root) {
  const errors = [];
  let pages = 0;
  const visit = (dir) => {
    for (const entry of readdirSync(dir, { withFileTypes: true })) {
      const file = join(dir, entry.name);
      if (entry.isDirectory()) visit(file);
      else if (/\.mdx?$/.test(entry.name)) {
        pages++;
        errors.push(...dateProblems(readFileSync(file, "utf8")).map((error) => `${relative(root, file)}: ${error}`));
      }
    }
  };
  visit(root);
  return { pages, errors };
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const result = inspectDates(fileURLToPath(new URL("../src/content/docs/", import.meta.url)));
  console.log(`[check-dates] ${result.pages} documents; ${result.errors.length} errors`);
  if (result.errors.length) {
    console.error(result.errors.join("\n"));
    process.exitCode = 1;
  }
}
