// Copies the static site (docs, overview, presentation) into public/guide so Vite bundles it into
// the app. The in-app Help window then works offline. public/guide is generated and git-ignored.
import { cpSync, existsSync, mkdirSync, rmSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const from = join(root, "site");
const to = join(root, "public", "guide");

if (!existsSync(from)) {
  console.error("site/ not found; cannot bundle the guide.");
  process.exit(1);
}
rmSync(to, { recursive: true, force: true });
mkdirSync(to, { recursive: true });
cpSync(from, to, { recursive: true });
console.log("Guide bundled into public/guide");
