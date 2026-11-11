// Keyboard shortcut labels per platform. "Mod" is the primary modifier (Command on macOS, Ctrl elsewhere);
// "Ctrl" always means the physical Control key, because macOS keeps Command+Tab for its app switcher.

export type Platform = "mac" | "other";

export function detectPlatform(userAgent: string = typeof navigator === "undefined" ? "" : navigator.userAgent): Platform {
  return /Mac|iPhone|iPad/.test(userAgent) ? "mac" : "other";
}

export const platform: Platform = detectPlatform();

const MAC: Record<string, string> = { mod: "⌘", ctrl: "⌃", alt: "⌥", shift: "⇧", enter: "↩" };
const OTHER: Record<string, string> = { mod: "Ctrl", ctrl: "Ctrl", alt: "Alt", shift: "Shift", enter: "Enter" };

/** The keys of a shortcut such as "Mod+Shift+P", as they should be drawn on this platform. */
export function shortcutKeys(spec: string, p: Platform = platform): string[] {
  const names = p === "mac" ? MAC : OTHER;
  return spec.split("+").map((part) => names[part.toLowerCase()] ?? (part.length === 1 ? part.toUpperCase() : part));
}

/** One string for tooltips and placeholders: "⌘K" on macOS, "Ctrl+K" elsewhere. */
export function formatShortcut(spec: string, p: Platform = platform): string {
  return shortcutKeys(spec, p).join(p === "mac" ? "" : "+");
}
