// Prepares a GitHub release: copies the signed installers to space-free file names and writes latest.json,
// the file the app downloads to learn whether a newer version exists.
//
//   pnpm tauri build            (with TAURI_SIGNING_PRIVATE_KEY set, see docs/RELEASING.md)
//   node scripts/make-update-manifest.mjs "What is new in this version"
//
// Upload everything in src-tauri/target/release/bundle/release/ to the GitHub release tagged v<version>.
import { copyFileSync, existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const conf = JSON.parse(readFileSync("src-tauri/tauri.conf.json", "utf8"));
const version = conf.version;
const latestUrl = conf.plugins?.updater?.endpoints?.[0] ?? "";
const repo = /github\.com\/([^/]+\/[^/]+)\/releases/.exec(latestUrl)?.[1];
if (!repo) throw new Error("The updater endpoint in tauri.conf.json is not a GitHub releases URL.");
const notes = process.argv[2] ?? "";

const bundle = "src-tauri/target/release/bundle";
const out = join(bundle, "release");
mkdirSync(out, { recursive: true });

// platform key → [folder, file name as built]
const candidates = {
  "windows-x86_64": [join(bundle, "nsis"), `Treeline_${version}_x64-setup.exe`],
  "linux-x86_64": [join(bundle, "linux"), `Treeline_${version}_amd64.AppImage`],
};

const platforms = {};
for (const [platform, [dir, name]] of Object.entries(candidates)) {
  const file = join(dir, name);
  const sig = `${file}.sig`;
  if (!existsSync(file) || !existsSync(sig)) {
    console.log(`skipped ${platform}: ${name} or its .sig not found`);
    continue;
  }
  const safe = name.replaceAll(" ", "-");
  copyFileSync(file, join(out, safe));
  copyFileSync(sig, join(out, `${safe}.sig`));
  platforms[platform] = {
    signature: readFileSync(sig, "utf8").trim(),
    url: `https://github.com/${repo}/releases/download/v${version}/${safe}`,
  };
  console.log(`added ${platform}: ${safe}`);
}
if (Object.keys(platforms).length === 0) throw new Error("No signed installers found. Build with TAURI_SIGNING_PRIVATE_KEY set.");

writeFileSync(
  join(out, "latest.json"),
  JSON.stringify({ version, notes, pub_date: new Date().toISOString(), platforms }, null, 2) + "\n",
);
console.log(`\nWrote ${join(out, "latest.json")}. Upload the files in ${out} to the release v${version} of ${repo}.`);
