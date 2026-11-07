import { WebviewWindow } from "@tauri-apps/api/webviewWindow";

/** Pages of the bundled guide that the app links to directly. */
export const GUIDE = {
  home: "docs/index.html",
  start: "docs/1-before-you-start.html",
  workflow: "docs/5-everyday-workflow.html",
  errors: "docs/13-when-something-goes-wrong.html",
  shortcuts: "docs/14-keyboard-shortcuts.html",
} as const;

const LABEL = "guide";

/**
 * Opens the bundled documentation in its own window (it works offline). If the window is already
 * open it is navigated to the requested page and brought to the front.
 */
export async function openGuide(page: string = GUIDE.home): Promise<void> {
  const url = `/guide/${page}`;
  const existing = await WebviewWindow.getByLabel(LABEL);
  if (existing) {
    // Navigate by closing and re-creating: a window has no navigate API, and this keeps it simple.
    await existing.close();
    // Give the runtime a moment to release the label before re-using it.
    await new Promise((r) => setTimeout(r, 150));
  }
  const win = new WebviewWindow(LABEL, {
    url,
    title: "Guide — APMGIT CLIENT",
    width: 1100,
    height: 820,
    minWidth: 640,
    minHeight: 480,
    center: true,
  });
  await new Promise<void>((resolve, reject) => {
    void win.once("tauri://created", () => resolve());
    void win.once("tauri://error", (e) => reject(new Error(String(e.payload))));
  });
}
