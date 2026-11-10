/** Background fetch for the active tab. Silent: failures (offline, auth needed) are ignored. */
export interface AutoFetchDeps {
  /** The active tab, or null when none is open. */
  activeTab: () => { id: string; path: string; hasRemotes: boolean | null } | null;
  fetch: (progressId: string, path: string) => Promise<unknown>;
  reload: (tabId: string) => Promise<void>;
  isHidden: () => boolean;
  /** Seconds between fetches (0 = off). Read on every call, so a changed setting applies at once. */
  seconds?: () => number;
  now?: () => number;
}

/** Seconds between background fetches; 0 turns it off. */
export const FETCH_SECOND_CHOICES = [0, 15, 30, 60, 120, 180, 300, 600, 1800] as const;
export const DEFAULT_FETCH_SECONDS = 120;
const KEY = "auto-fetch-seconds";

export function loadFetchSeconds(): number {
  try {
    const raw = localStorage.getItem(KEY);
    if (raw === null) return DEFAULT_FETCH_SECONDS;
    const n = Number(raw);
    return (FETCH_SECOND_CHOICES as readonly number[]).includes(n) ? n : DEFAULT_FETCH_SECONDS;
  } catch {
    return DEFAULT_FETCH_SECONDS;
  }
}

const listeners = new Set<() => void>();
/** Calls `cb` whenever the interval is changed in Settings. Returns an unsubscribe function. */
export function onFetchSecondsChange(cb: () => void): () => void {
  listeners.add(cb);
  return () => void listeners.delete(cb);
}

export function saveFetchSeconds(seconds: number): void {
  try {
    localStorage.setItem(KEY, String(seconds));
  } catch {
    /* storage unavailable: the choice lasts until the app closes */
  }
  listeners.forEach((l) => l());
}

/** "Never", "every 15 seconds", "every minute", "every 5 minutes"… */
export function fetchSecondsLabel(s: number): string {
  if (s === 0) return "Never (use the Fetch button)";
  if (s < 60) return `every ${s} seconds`;
  if (s === 60) return "every minute";
  return `every ${s / 60} minutes`;
}

/**
 * Runs `tick` on the chosen interval. When the setting changes the old timer is cancelled, a new one is
 * started with the new period, and `tick` runs once straight away (it fetches only if one is now due).
 * Returns a function that stops everything.
 */
export function startAutoFetchTimer(
  tick: () => Promise<unknown>,
  seconds: () => number = loadFetchSeconds,
): () => void {
  let timer: ReturnType<typeof setInterval> | undefined;
  const restart = () => {
    if (timer !== undefined) clearInterval(timer);
    timer = undefined;
    const s = seconds();
    if (s > 0) timer = setInterval(() => void tick(), s * 1000);
    void tick();
  };
  restart();
  const off = onFetchSecondsChange(restart);
  return () => {
    off();
    if (timer !== undefined) clearInterval(timer);
  };
}

export function createAutoFetch(deps: AutoFetchDeps) {
  const now = deps.now ?? Date.now;
  const lastFetch = new Map<string, number>();
  let running = false;
  return async function tick(): Promise<boolean> {
    const tab = deps.activeTab();
    const seconds = (deps.seconds ?? loadFetchSeconds)();
    if (seconds <= 0 || running || !tab?.path || deps.isHidden()) return false;
    if (now() - (lastFetch.get(tab.id) ?? -Infinity) < seconds * 1000 - 1000) return false;
    // Repos with no remote have nothing to fetch (null = not loaded yet: try anyway).
    if (tab.hasRemotes === false) return false;
    running = true;
    lastFetch.set(tab.id, now());
    try {
      await deps.fetch(`bg:${tab.id}`, tab.path);
      await deps.reload(tab.id);
      return true;
    } catch {
      return false;
    } finally {
      running = false;
    }
  };
}
