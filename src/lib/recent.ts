// The "Recent" list on the Home screen: repositories opened lately that are not bookmarked.

export interface RecentRepo {
  path: string;
  name: string;
}

export const RECENT_MAX = 8;

/** Puts `repo` first, drops an older entry for the same folder and keeps at most `max` entries. */
export function addRecent(recent: RecentRepo[], repo: RecentRepo, max = RECENT_MAX): RecentRepo[] {
  return [repo, ...recent.filter((r) => r.path !== repo.path)].slice(0, max);
}

/** What Home shows: the recent list without the folders already shown as bookmarks. */
export function visibleRecent(recent: RecentRepo[], bookmarks: { path: string }[]): RecentRepo[] {
  const marked = new Set(bookmarks.map((b) => b.path));
  return recent.filter((r) => !marked.has(r.path));
}
