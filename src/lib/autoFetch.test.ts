import { afterEach, describe, expect, it, vi } from "vitest";
import { createAutoFetch, fetchSecondsLabel, loadFetchSeconds, saveFetchSeconds, startAutoFetchTimer } from "./autoFetch";

const make = (over: Partial<Parameters<typeof createAutoFetch>[0]> = {}) => {
  let t = 1_000_000;
  const fetch = vi.fn().mockResolvedValue(undefined);
  const reload = vi.fn().mockResolvedValue(undefined);
  const tick = createAutoFetch({
    activeTab: () => ({ id: "t1", path: "C:/test", hasRemotes: true }),
    fetch,
    reload,
    isHidden: () => false,
    now: () => t,
    seconds: () => 120,
    ...over,
  });
  return { tick, fetch, reload, advance: (ms: number) => (t += ms) };
};

describe("auto fetch", () => {
  it("fetches then reloads, using a background progress id", async () => {
    const { tick, fetch, reload } = make();
    expect(await tick()).toBe(true);
    expect(fetch).toHaveBeenCalledWith("bg:t1", "C:/test");
    expect(reload).toHaveBeenCalledWith("t1");
  });
  it("waits for the interval before fetching again", async () => {
    const { tick, fetch, advance } = make();
    await tick();
    advance(30_000);
    expect(await tick()).toBe(false);
    advance(120_000);
    expect(await tick()).toBe(true);
    expect(fetch).toHaveBeenCalledTimes(2);
  });
  it("supports sub-minute intervals", async () => {
    const { tick, advance, fetch } = make({ seconds: () => 15 });
    await tick();
    advance(10_000);
    expect(await tick()).toBe(false);
    advance(5_000);
    expect(await tick()).toBe(true);
    expect(fetch).toHaveBeenCalledTimes(2);
  });
  it("skips hidden windows, no tab, and repos without remotes", async () => {
    expect(await make({ isHidden: () => true }).tick()).toBe(false);
    expect(await make({ activeTab: () => null }).tick()).toBe(false);
    expect(await make({ activeTab: () => ({ id: "a", path: "p", hasRemotes: false }) }).tick()).toBe(false);
  });
  it("swallows fetch failures and does not reload", async () => {
    const { tick, reload } = make({ fetch: vi.fn().mockRejectedValue(new Error("offline")) });
    expect(await tick()).toBe(false);
    expect(reload).not.toHaveBeenCalled();
  });
  it("does not overlap runs", async () => {
    let release!: () => void;
    const slow = vi.fn(() => new Promise<void>((r) => (release = r)));
    const { tick } = make({ fetch: slow });
    const first = tick();
    expect(await tick()).toBe(false);
    release();
    await first;
    expect(slow).toHaveBeenCalledTimes(1);
  });
  it("never fetches when the setting is Off (0)", async () => {
    const { tick, fetch } = make({ seconds: () => 0 });
    expect(await tick()).toBe(false);
    expect(fetch).not.toHaveBeenCalled();
  });
  it("applies a changed setting immediately", async () => {
    let s = 1800;
    const { tick, advance, fetch } = make({ seconds: () => s });
    await tick();
    advance(5 * 60_000);
    expect(await tick()).toBe(false); // 30 minutes not reached
    s = 300;
    expect(await tick()).toBe(true); // 5 minutes is now due
    expect(fetch).toHaveBeenCalledTimes(2);
  });
});

describe("fetch interval setting", () => {
  const store = new Map<string, string>();
  vi.stubGlobal("localStorage", {
    getItem: (k: string) => store.get(k) ?? null,
    setItem: (k: string, v: string) => void store.set(k, v),
  });
  it("defaults to 2 minutes, round-trips (including sub-minute values), and rejects junk", () => {
    store.clear();
    expect(loadFetchSeconds()).toBe(120);
    for (const v of [0, 15, 30, 60, 180, 600]) {
      saveFetchSeconds(v);
      expect(loadFetchSeconds()).toBe(v);
    }
    store.set("auto-fetch-seconds", "7");
    expect(loadFetchSeconds()).toBe(120);
    store.set("auto-fetch-seconds", "abc");
    expect(loadFetchSeconds()).toBe(120);
  });
  it("labels read naturally", () => {
    expect(fetchSecondsLabel(15)).toBe("every 15 seconds");
    expect(fetchSecondsLabel(60)).toBe("every minute");
    expect(fetchSecondsLabel(180)).toBe("every 3 minutes");
    expect(fetchSecondsLabel(0)).toContain("Never");
  });
});

describe("timer follows the setting", () => {
  afterEach(() => vi.useRealTimers());
  it("restarts with the new period, ticks at once, and stops when Off", () => {
    vi.useFakeTimers();
    let s = 60;
    const tick = vi.fn().mockResolvedValue(true);
    const stop = startAutoFetchTimer(tick, () => s);
    expect(tick).toHaveBeenCalledTimes(1); // on start
    vi.advanceTimersByTime(60_000);
    expect(tick).toHaveBeenCalledTimes(2); // 1-minute period

    s = 15;
    saveFetchSeconds(15); // the user picks 15 seconds
    expect(tick).toHaveBeenCalledTimes(3); // immediate re-check
    vi.advanceTimersByTime(45_000);
    expect(tick).toHaveBeenCalledTimes(6); // now every 15 s; the old 60 s timer is gone

    s = 0;
    saveFetchSeconds(0); // Off: no timer any more
    const afterOff = tick.mock.calls.length;
    vi.advanceTimersByTime(10 * 60_000);
    expect(tick).toHaveBeenCalledTimes(afterOff);

    stop();
    s = 30;
    saveFetchSeconds(30); // stopped: later changes are ignored
    vi.advanceTimersByTime(120_000);
    expect(tick).toHaveBeenCalledTimes(afterOff);
  });
});
