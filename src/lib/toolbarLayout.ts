// Which toolbar actions stay visible at a given width; the rest go into the "More" menu.

export type OverflowAction = "branch" | "merge" | "stash" | "tag";

/**
 * Daily actions (Commit, Fetch, Pull, Push) always stay. As the toolbar narrows, Stash and Tag move into More first,
 * then Branch and Merge; labels go only on a very narrow window. A width of 0 (not measured yet) keeps the full layout.
 */
export function toolbarLayout(width: number): { overflow: OverflowAction[]; labels: boolean } {
  if (width === 0 || width >= 1150) return { overflow: [], labels: true };
  if (width >= 1000) return { overflow: ["stash", "tag"], labels: true };
  return { overflow: ["branch", "merge", "stash", "tag"], labels: width >= 860 };
}

/** What the platform calls its file manager, for "Show in …". */
export function fileManagerName(userAgent: string = typeof navigator === "undefined" ? "" : navigator.userAgent): string {
  if (/Mac|iPhone|iPad/.test(userAgent)) return "Finder";
  if (/Windows/.test(userAgent)) return "Explorer";
  return "file manager";
}
