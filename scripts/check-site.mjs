// Checks every local link, image and #anchor in site/**/*.html. Exits non-zero on any broken one.
import { existsSync, readFileSync, readdirSync, statSync } from "node:fs";
import { dirname, join, normalize, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const siteDir = resolve(dirname(fileURLToPath(import.meta.url)), "..", "site");
const pages = [];
(function walk(dir) {
  for (const f of readdirSync(dir)) {
    const p = join(dir, f);
    if (statSync(p).isDirectory()) walk(p);
    else if (f.endsWith(".html")) pages.push(p);
  }
})(siteDir);

const idsOf = new Map();
const idsIn = (file) => {
  if (!idsOf.has(file)) {
    const html = readFileSync(file, "utf8");
    idsOf.set(file, new Set([...html.matchAll(/\sid="([^"]+)"/g)].map((m) => m[1])));
  }
  return idsOf.get(file);
};

let broken = 0;
for (const page of pages) {
  const html = readFileSync(page, "utf8");
  for (const m of html.matchAll(/\s(?:href|src)="([^"]+)"/g)) {
    const target = m[1];
    if (/^(https?:|mailto:|data:|javascript:)/.test(target)) continue;
    const [pathPart, frag] = target.split("#");
    const file = pathPart ? normalize(join(dirname(page), pathPart)) : page;
    if (!existsSync(file)) {
      console.error(`BROKEN  ${page.replace(siteDir, "site")} → ${target} (missing file)`);
      broken++;
    } else if (frag && file.endsWith(".html") && !idsIn(file).has(frag)) {
      console.error(`BROKEN  ${page.replace(siteDir, "site")} → ${target} (no #${frag})`);
      broken++;
    }
  }
}
console.log(`Checked ${pages.length} pages: ${broken === 0 ? "all links OK" : broken + " broken"}`);
process.exit(broken === 0 ? 0 : 1);
