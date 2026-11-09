import { beforeEach, describe, expect, it, vi } from "vitest";

// A stand-in for the backend: every command fails by default (the store copes), and the two the
// "open a repository" path depends on are set per test.
const mocks = vi.hoisted(() => ({
  toplevel: vi.fn(async (p: string) => p),
  repoRisks: vi.fn(async (_p: string): Promise<string[]> => []),
}));

vi.mock("./api", () => {
  const fail = async () => {
    throw new Error("not available in tests");
  };
  const api = new Proxy(
    { toplevel: mocks.toplevel, repoRisks: mocks.repoRisks },
    { get: (target, key) => (key in target ? (target as Record<string, unknown>)[key as string] : fail) },
  );
  return { api, errorMessage: (e: unknown) => String(e), baseName: (p: string) => p.split("/").pop() ?? p, joinPath: (a: string, b: string) => `${a}/${b}` };
});

import { useApp } from "./store";

function stubLocalStorage() {
  const data = new Map<string, string>();
  vi.stubGlobal("localStorage", {
    getItem: (k: string) => data.get(k) ?? null,
    setItem: (k: string, v: string) => void data.set(k, v),
    removeItem: (k: string) => void data.delete(k),
  });
  return data;
}

describe("opening a repository whose own settings can run programs", () => {
  let storage: Map<string, string>;
  beforeEach(() => {
    storage = stubLocalStorage();
    mocks.toplevel.mockClear();
    mocks.repoRisks.mockReset();
    mocks.repoRisks.mockResolvedValue([]);
    useApp.setState({ tabs: [], activeId: null, dialog: null });
  });

  it("opens an ordinary repository straight away", async () => {
    await useApp.getState().openRepo("/work/plain");
    expect(useApp.getState().dialog).toBeNull();
    expect(useApp.getState().tabs.map((t) => t.path)).toEqual(["/work/plain"]);
  });

  it("asks first, opens nothing until confirmed, and then remembers the answer", async () => {
    mocks.repoRisks.mockResolvedValue(["core.hooksPath = .husky: git hooks from that folder run when you commit, merge or check out"]);

    await useApp.getState().openRepo("/work/risky");
    const dialog = useApp.getState().dialog;
    expect(dialog?.title).toBe("This repository can run programs");
    expect(dialog?.description).toContain("core.hooksPath = .husky");
    expect(dialog?.danger).toBe(true);
    expect(useApp.getState().tabs).toHaveLength(0);

    await dialog!.onSubmit({});
    expect(useApp.getState().tabs.map((t) => t.path)).toEqual(["/work/risky"]);
    expect(JSON.parse(storage.get("trusted-repos")!)).toEqual(["/work/risky"]);

    // Closed and opened again: the choice was remembered, so there is no second question.
    useApp.setState({ tabs: [], activeId: null, dialog: null });
    await useApp.getState().openRepo("/work/risky");
    expect(useApp.getState().dialog).toBeNull();
    expect(useApp.getState().tabs).toHaveLength(1);
  });

  it("declining (just closing the dialog) leaves the repository closed", async () => {
    mocks.repoRisks.mockResolvedValue(["Hook scripts (pre-commit): these run when you commit, merge, check out or push"]);
    await useApp.getState().openRepo("/work/declined");
    useApp.getState().closeDialog();
    expect(useApp.getState().tabs).toHaveLength(0);
    expect(storage.get("trusted-repos")).toBeUndefined();
  });

  it("asks anyway when the scan itself fails, instead of opening silently", async () => {
    mocks.repoRisks.mockRejectedValue(new Error("boom"));
    await useApp.getState().openRepo("/work/unscannable");
    const dialog = useApp.getState().dialog;
    expect(dialog?.title).toBe("This repository can run programs");
    expect(dialog?.description).toContain("could not check this repository's settings");
    expect(dialog?.description).toContain("boom");
    expect(useApp.getState().tabs).toHaveLength(0);
  });

  it("lists at most eight findings and says how many more there are", async () => {
    mocks.repoRisks.mockResolvedValue(Array.from({ length: 11 }, (_, i) => `merge.d${i}.driver = x: runs a program to merge files`));
    await useApp.getState().openRepo("/work/many");
    expect(useApp.getState().dialog?.description).toContain("…and 3 more");
  });
});
