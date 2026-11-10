import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";

/** Asks the update server whether a newer version exists; null means this is the latest. */
export function checkForUpdate(): Promise<Update | null> {
  return check();
}

/**
 * Downloads and installs a version the user has confirmed, then restarts the app.
 * `onProgress` receives 0–100 when the download size is known, otherwise null.
 */
export async function installUpdate(update: Update, onProgress: (percent: number | null) => void): Promise<void> {
  let total = 0;
  let done = 0;
  await update.downloadAndInstall((e) => {
    if (e.event === "Started") total = e.data.contentLength ?? 0;
    else if (e.event === "Progress") {
      done += e.data.chunkLength;
      onProgress(total > 0 ? Math.min(100, Math.round((done / total) * 100)) : null);
    }
  });
  await relaunch();
}

export const UPDATE_CHECK_INTERVAL_MS = 6 * 60 * 60 * 1000;

/**
 * Checks for a newer version in the background. Failures (offline, no update server) are ignored:
 * the Settings dialog's "Check for updates" button reports real errors. Stops looking once one is found.
 */
export function createUpdateWatch(deps: { check: () => Promise<Update | null>; onAvailable: (u: Update) => void }) {
  let found = false;
  let running = false;
  return async function tick(): Promise<boolean> {
    if (found || running) return found;
    running = true;
    try {
      const u = await deps.check();
      if (u) {
        found = true;
        deps.onAvailable(u);
      }
    } catch {
      /* offline or no update server */
    } finally {
      running = false;
    }
    return found;
  };
}
