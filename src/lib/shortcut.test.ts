import { describe, expect, it } from "vitest";
import { detectPlatform, formatShortcut, shortcutKeys } from "./shortcut";

const MAC_UA = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko)";
const WIN_UA = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0 Safari/537.36";
const LINUX_UA = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/605.1.15 (KHTML, like Gecko)";

describe("detectPlatform", () => {
  it("recognises macOS", () => expect(detectPlatform(MAC_UA)).toBe("mac"));
  it("treats Windows and Linux as other", () => {
    expect(detectPlatform(WIN_UA)).toBe("other");
    expect(detectPlatform(LINUX_UA)).toBe("other");
  });
});

describe("formatShortcut", () => {
  it("Mod is Command on macOS and Ctrl elsewhere", () => {
    expect(formatShortcut("Mod+K", "mac")).toBe("⌘K");
    expect(formatShortcut("Mod+K", "other")).toBe("Ctrl+K");
  });
  it("Ctrl stays the Control key: Command+Tab belongs to macOS", () => {
    expect(formatShortcut("Ctrl+Tab", "mac")).toBe("⌃Tab");
    expect(formatShortcut("Ctrl+Tab", "other")).toBe("Ctrl+Tab");
  });
  it("uses Apple symbols for Shift, Option and Return on macOS", () => {
    expect(formatShortcut("Mod+Shift+P", "mac")).toBe("⌘⇧P");
    expect(formatShortcut("Alt+Mod+Enter", "mac")).toBe("⌥⌘↩");
    expect(formatShortcut("Mod+Shift+P", "other")).toBe("Ctrl+Shift+P");
    expect(formatShortcut("Mod+Enter", "other")).toBe("Ctrl+Enter");
  });
  it("leaves keys without a modifier alone and upper-cases single letters", () => {
    expect(formatShortcut("F1", "mac")).toBe("F1");
    expect(formatShortcut("mod+o", "other")).toBe("Ctrl+O");
  });
});

describe("shortcutKeys", () => {
  it("splits a shortcut into the keys to draw", () => {
    expect(shortcutKeys("Mod+K", "mac")).toEqual(["⌘", "K"]);
    expect(shortcutKeys("Mod+K", "other")).toEqual(["Ctrl", "K"]);
    expect(shortcutKeys("Ctrl+Shift+Tab", "mac")).toEqual(["⌃", "⇧", "Tab"]);
  });
});
