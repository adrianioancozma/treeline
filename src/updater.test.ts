import { describe, expect, it, vi } from "vitest";
import type { Update } from "@tauri-apps/plugin-updater";

vi.mock("@tauri-apps/plugin-updater", () => ({ check: vi.fn() }));
vi.mock("@tauri-apps/plugin-process", () => ({ relaunch: vi.fn() }));
import { check } from "@tauri-apps/plugin-updater";
import { checkForUpdate, createUpdateWatch, UPDATES_ENABLED } from "./updater";

const upd = { version: "9.9.9" } as Update;

describe("background update watch", () => {
  it("reports a newer version once and then stops asking", async () => {
    const check = vi.fn().mockResolvedValue(upd);
    const onAvailable = vi.fn();
    const tick = createUpdateWatch({ check, onAvailable });
    expect(await tick()).toBe(true);
    expect(await tick()).toBe(true);
    expect(check).toHaveBeenCalledTimes(1);
    expect(onAvailable).toHaveBeenCalledWith(upd);
  });
  it("keeps quiet when already on the latest version", async () => {
    const onAvailable = vi.fn();
    const tick = createUpdateWatch({ check: vi.fn().mockResolvedValue(null), onAvailable });
    expect(await tick()).toBe(false);
    expect(onAvailable).not.toHaveBeenCalled();
  });
  it("ignores errors and tries again next time", async () => {
    const check = vi.fn().mockRejectedValueOnce(new Error("offline")).mockResolvedValueOnce(upd);
    const onAvailable = vi.fn();
    const tick = createUpdateWatch({ check, onAvailable });
    expect(await tick()).toBe(false);
    expect(await tick()).toBe(true);
    expect(onAvailable).toHaveBeenCalledTimes(1);
  });
});

describe("updates in this build", () => {
  // This fork has no update server of its own yet; the upstream one would replace it with a build without remote support.
  it("are turned off: no request to an update server, never an update", async () => {
    expect(UPDATES_ENABLED).toBe(false);
    await expect(checkForUpdate()).resolves.toBeNull();
    expect(check).not.toHaveBeenCalled();
  });
});
