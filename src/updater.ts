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
