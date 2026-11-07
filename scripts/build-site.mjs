// Builds the documentation pages (site/docs/*.html) from docs/USER_GUIDE.md.
// The guide stays the single source of truth: edit the markdown, run `pnpm site:build`.
// The landing page (site/index.html) and the slide deck (site/present.html) are hand-written.
import { readFileSync, writeFileSync, mkdirSync, readdirSync, unlinkSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { marked } from "marked";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const outDir = join(root, "site", "docs");
mkdirSync(outDir, { recursive: true });

const md = readFileSync(join(root, "docs", "USER_GUIDE.md"), "utf8");

const slugify = (text) =>
  text
    .replace(/<[^>]+>/g, "")
    .toLowerCase()
    .replace(/&[a-z]+;/g, "")
    .replace(/[^\p{L}\p{N}\s-]/gu, "")
    .trim()
    .replace(/\s+/g, "-");
const escapeHtml = (s) => s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");

// ---- split the guide into an intro and numbered sections ------------------------------------
const lines = md.split("\n");
const title = (lines.find((l) => l.startsWith("# ")) ?? "# Guide").slice(2).trim();
const firstSection = lines.findIndex((l) => l.startsWith("## "));
const intro = lines
  .slice(0, firstSection)
  .filter((l) => !l.startsWith("# "))
  .join("\n");
// The in-page "Contents" list is replaced by the sidebar.
const introClean = intro.replace(/\*\*Contents\*\*[\s\S]*?(?=\n---|\s*$)/, "").replace(/\n---\s*$/, "").trim();

const sections = [];
for (let i = firstSection; i < lines.length; i++) {
  if (lines[i].startsWith("## ")) {
    const heading = lines[i].slice(3).trim();
    sections.push({ heading, id: slugify(heading), body: [] });
  } else if (sections.length) sections[sections.length - 1].body.push(lines[i]);
}
for (const s of sections) {
  s.body = s.body.join("\n").replace(/\n---\s*$/, "").trim();
  s.label = s.heading.replace(/^\d+\.\s*/, "");
  s.num = (s.heading.match(/^(\d+)\./) ?? [])[1] ?? "";
  s.h3 = [...s.body.matchAll(/^### (.+)$/gm)].map((m) => ({ text: m[1].replace(/`/g, ""), id: slugify(m[1]) }));
}

// ---- cross-links: "#section-id" → "section-id.html", "#sub-heading" → "section.html#sub-heading" ----
const sectionIds = new Set(sections.map((s) => s.id));
const subToSection = new Map();
for (const s of sections) for (const h of s.h3) subToSection.set(h.id, s.id);
// "Signing in" is an h3 in the guide's first section.
const fixLinks = (text) =>
  text.replace(/\]\(#([^)\s]+)\)/g, (m, id) => {
    if (sectionIds.has(id)) return `](${id}.html)`;
    if (subToSection.has(id)) return `](${subToSection.get(id)}.html#${id})`;
    return m;
  });

const render = (source) => {
  let html = marked.parse(fixLinks(source), { gfm: true, breaks: false });
  // anchors on headings
  html = html.replace(/<h([23])>([\s\S]*?)<\/h\1>/g, (m, lvl, inner) => `<h${lvl} id="${slugify(inner)}">${inner}</h${lvl}>`);
  // wide tables scroll instead of overflowing the page
  html = html.replace(/<table>/g, '<div class="table-wrap"><table>').replace(/<\/table>/g, "</table></div>");
  return html;
};

// ---- page template --------------------------------------------------------------------------
const head = (pageTitle, rel) => `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>${escapeHtml(pageTitle)} · APMGIT CLIENT</title>
<meta name="description" content="APMGIT CLIENT documentation: a fast Git client that works with any Git provider.">
<link rel="icon" href="${rel}assets/icon.png">
<link rel="stylesheet" href="${rel}assets/site.css">
<script src="${rel}assets/site.js" defer></script>
</head>`;

const header = (rel, active) => `<header class="topbar">
  <a class="brand" href="${rel}index.html"><img src="${rel}assets/icon.png" alt="" width="28" height="28"><span>APMGIT CLIENT</span></a>
  <nav class="top-links" aria-label="Main">
    <a href="${rel}index.html"${active === "home" ? ' aria-current="page"' : ""}>Overview</a>
    <a href="${rel}docs/index.html"${active === "docs" ? ' aria-current="page"' : ""}>Docs</a>
    <a href="${rel}present.html">Presentation</a>
    <button class="theme-toggle" type="button" aria-label="Toggle light or dark theme">◐</button>
  </nav>
</header>`;

const sidebar = (currentId) => `<nav class="doc-nav" aria-label="Documentation">
  <button class="nav-toggle" type="button" aria-expanded="false">Contents</button>
  <ol>
    <li><a href="index.html"${currentId === "index" ? ' aria-current="page"' : ""}>Introduction</a></li>
    ${sections
      .map(
        (s) =>
          `<li><a href="${s.id}.html"${s.id === currentId ? ' aria-current="page"' : ""}><span class="num">${s.num}</span>${escapeHtml(s.label)}</a></li>`,
      )
      .join("\n    ")}
  </ol>
</nav>`;

const footer = `<footer class="footer"><p>APMGIT CLIENT works with any Git server. Documentation is generated from <code>docs/USER_GUIDE.md</code>.</p></footer>`;

const page = ({ pageTitle, id, article, prev, next, toc }) => `${head(pageTitle, "../")}
<body class="docs">
${header("../", "docs")}
<div class="doc-layout">
${sidebar(id)}
<main class="doc-main" id="main">
<article class="prose">
${article}
</article>
${toc}
<nav class="pager" aria-label="Previous and next page">
  ${prev ? `<a class="prev" href="${prev.href}"><small>Previous</small>${escapeHtml(prev.label)}</a>` : "<span></span>"}
  ${next ? `<a class="next" href="${next.href}"><small>Next</small>${escapeHtml(next.label)}</a>` : "<span></span>"}
</nav>
${footer}
</main>
</div>
</body>
</html>
`;

// remove stale pages from earlier builds
for (const f of readdirSync(outDir)) if (f.endsWith(".html")) unlinkSync(join(outDir, f));

// ---- introduction page ----------------------------------------------------------------------
const cards = sections
  .map(
    (s) =>
      `<a class="card-link" href="${s.id}.html"><span class="num">${s.num}</span><strong>${escapeHtml(s.label)}</strong></a>`,
  )
  .join("\n");
writeFileSync(
  join(outDir, "index.html"),
  page({
    pageTitle: "Documentation",
    id: "index",
    article: `<h1>${escapeHtml(title)}</h1>\n${render(introClean)}\n<h2 id="sections">What's in the guide</h2>\n<div class="card-grid">\n${cards}\n</div>`,
    prev: null,
    next: { href: `${sections[0].id}.html`, label: sections[0].heading },
    toc: "",
  }),
);

// ---- one page per section -------------------------------------------------------------------
sections.forEach((s, i) => {
  const toc = s.h3.length
    ? `<aside class="on-page" aria-label="On this page"><strong>On this page</strong><ul>${s.h3
        .map((h) => `<li><a href="#${h.id}">${escapeHtml(h.text)}</a></li>`)
        .join("")}</ul></aside>`
    : "";
  writeFileSync(
    join(outDir, `${s.id}.html`),
    page({
      pageTitle: s.heading,
      id: s.id,
      article: `<h1>${escapeHtml(s.heading)}</h1>\n${render(s.body)}`,
      prev: i === 0 ? { href: "index.html", label: "Introduction" } : { href: `${sections[i - 1].id}.html`, label: sections[i - 1].heading },
      next: i === sections.length - 1 ? null : { href: `${sections[i + 1].id}.html`, label: sections[i + 1].heading },
      toc,
    }),
  );
});

console.log(`Built ${sections.length + 1} documentation pages in site/docs/`);
