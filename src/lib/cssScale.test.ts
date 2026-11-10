import { describe, expect, it } from "vitest";
// Same pattern as vite.config.ts: the project has no Node type definitions, and `pnpm build` type-checks tests.
// (A `?raw` import is no alternative: vitest replaces CSS imports with an empty string.)
// @ts-expect-error type error without @types/node package
import { readFileSync } from "node:fs";

// Guard for the design scale in App.css: sizes, radii and text-on-colour come from tokens defined in :root.
// Allowed literals: the token definitions themselves, `0`, `50%` (circles) and the logo's 18px icon radius.

export function scaleViolations(css: string): string[] {
  const body = css.replace(/\/\*[\s\S]*?\*\//g, "");
  const out: string[] = [];
  for (const m of body.matchAll(/font-size:\s*([0-9.]+px)/g)) out.push(`font-size: ${m[1]}`);
  for (const m of body.matchAll(/border(?:-[a-z-]+)?-radius:\s*([^;}]+)/g)) {
    const literal = m[1].match(/\b[0-9.]+px\b/g)?.filter((v) => v !== "0px" && v !== "18px") ?? [];
    for (const v of literal) out.push(`radius: ${v}`);
  }
  for (const m of body.matchAll(/(^|[;{\s])color:\s*(#fff\b|#ffffff\b|white\b)/gi)) out.push(`color: ${m[2]}`);
  return out;
}

describe("App.css design scale", () => {
  it("uses tokens for font sizes, radii and text on solid colours", () => {
    const css: string = readFileSync(new URL("../App.css", import.meta.url), "utf8");
    // Never pass on an empty or wrong read: the stylesheet is large and defines the scale tokens.
    expect(css.length).toBeGreaterThan(10_000);
    expect(css).toContain("--fs-m:");
    expect(css).toContain("--r-m:");
    // The logo mirrors the app icon (white glyph on a gradient); it is the one documented exception.
    const withoutLogo = css.replace(/\.logo \{[^}]*\}/, "");
    expect(scaleViolations(withoutLogo)).toEqual([]);
  });

  it("reports literals it is meant to catch", () => {
    expect(scaleViolations(".a { font-size: 12.5px; border-radius: 5px 5px 0 0; color: #fff; }")).toEqual([
      "font-size: 12.5px",
      "radius: 5px",
      "radius: 5px",
      "color: #fff",
    ]);
  });

  it("allows tokens, zero, circles and the logo radius", () => {
    expect(scaleViolations(".a { font-size: var(--fs-s); border-radius: 0; } .b { border-radius: 50%; } .c { border-radius: 18px; }")).toEqual([]);
  });
});
