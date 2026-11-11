import { beforeEach, describe, expect, it, vi } from "vitest";

// A stand-in for the backend: every command fails by default; the remote ones are spies.
const mocks = vi.hoisted(() => {
  const order: string[] = [];
  return {
    order,
    remoteDisconnect: vi.fn(async (_host: string) => undefined),
    remoteReconnect: vi.fn(async (_host: string) => {
      await new Promise((r) => setTimeout(r, 20)); // reconnecting takes a moment
      order.push("reconnect");
    }),
    status: vi.fn(async (_path: string) => {
      order.push("status");
      throw new Error("not available in tests");
    }),
  };
});

vi.mock("./api", () => {
  const fail = async () => {
    throw new Error("not available in tests");
  };
  const api = new Proxy(mocks as unknown as Record<string, unknown>, {
    get: (target, key) => (key in target ? target[key as string] : fail),
  });
  return {
    api,
    setRemoteHost: () => undefined,
    errorMessage: (e: unknown) => String(e),
    baseName: (p: string) => p.split("/").pop() ?? p,
    joinPath: (a: string, b: string) => `${a}/${b}`,
  };
});

import { useApp } from "./store";

const remoteTab = (id: string, host: string, path: string) => ({ id, host, path, name: path.split("/").pop()! });

beforeEach(() => {
  vi.stubGlobal("localStorage", { getItem: () => null, setItem: () => undefined, removeItem: () => undefined });
  mocks.remoteDisconnect.mockClear();
  mocks.remoteReconnect.mockClear();
  mocks.status.mockClear();
  mocks.order.length = 0;
  useApp.setState({ tabs: [], activeId: null, dialog: null, data: {}, hosts: {}, browsingHost: null });
});

describe("closing remote tabs", () => {
  it("closes the connection when the last tab on that computer closes", () => {
    useApp.setState({ tabs: [remoteTab("a", "linux-box", "/r/one"), remoteTab("b", "linux-box", "/r/two")], activeId: "a" });
    useApp.getState().closeTab("a");
    expect(mocks.remoteDisconnect).not.toHaveBeenCalled(); // another tab still reads from linux-box
    useApp.getState().closeTab("b");
    expect(mocks.remoteDisconnect).toHaveBeenCalledTimes(1);
    expect(mocks.remoteDisconnect).toHaveBeenCalledWith("linux-box");
  });
});

describe("the browser keeps the computer it is on", () => {
  it("does not close the connection the browser is using", () => {
    useApp.setState({ tabs: [remoteTab("a", "linux-box", "/r/one")], activeId: "a" });
    useApp.getState().setBrowsingHost("linux-box");
    useApp.getState().closeTab("a");
    expect(mocks.remoteDisconnect).not.toHaveBeenCalled();
  });

  it("closes a computer it leaves when no tab is on it", () => {
    useApp.setState({ tabs: [remoteTab("a", "build-box", "/r/one")], activeId: "a" });
    useApp.getState().setBrowsingHost("linux-box");
    useApp.getState().setBrowsingHost("build-box"); // picked another computer
    expect(mocks.remoteDisconnect).toHaveBeenCalledWith("linux-box");
    useApp.getState().setBrowsingHost(null); // closed; build-box still has a tab
    expect(mocks.remoteDisconnect).toHaveBeenCalledTimes(1);
  });
});

describe("connection states", () => {
  it("keeps the newest report per computer", () => {
    const { applyHostState } = useApp.getState();
    applyHostState({ host: "linux-box", gen: 2, state: "alive" });
    applyHostState({ host: "linux-box", gen: 1, state: "failed" }); // about the old connection, arrived late
    applyHostState({ host: "build-box", gen: 1, state: "failed" });
    expect(useApp.getState().hosts["linux-box"]).toMatchObject({ gen: 2, state: "alive" });
    expect(useApp.getState().hosts["build-box"]).toMatchObject({ gen: 1, state: "failed" });
    applyHostState({ host: "linux-box", gen: 2, state: "idle" });
    expect(useApp.getState().hosts["linux-box"]).toMatchObject({ gen: 2, state: "idle" });
  });
});

describe("Refresh on a remote tab", () => {
  it("reconnects that computer first and reloads only once it is done", async () => {
    useApp.setState({ tabs: [remoteTab("a", "linux-box", "/r/one")], activeId: "a" });
    await useApp.getState().refresh("a");
    expect(mocks.remoteReconnect).toHaveBeenCalledWith("linux-box");
    expect(mocks.order).toEqual(["reconnect", "status"]);
  });

  it("shows why reconnecting failed and does not reload", async () => {
    mocks.remoteReconnect.mockRejectedValueOnce("Treeline could not connect to “linux-box”: ssh: No route to host");
    useApp.setState({ tabs: [remoteTab("a", "linux-box", "/r/one")], activeId: "a" });
    await useApp.getState().refresh("a");
    expect(mocks.status).not.toHaveBeenCalled();
    expect(useApp.getState().data.a?.error).toBe("Treeline could not connect to “linux-box”: ssh: No route to host");
  });

  it("does not reconnect anything for a local tab", async () => {
    useApp.setState({ tabs: [{ id: "a", path: "/local/repo", name: "repo" }], activeId: "a" });
    await useApp.getState().refresh("a");
    expect(mocks.remoteReconnect).not.toHaveBeenCalled();
    expect(mocks.status).toHaveBeenCalledTimes(1);
  });

  it("Reconnect reloads the active tab only when it is on that computer", async () => {
    useApp.setState({ tabs: [remoteTab("a", "linux-box", "/r/one"), remoteTab("b", "build-box", "/r/two")], activeId: "b" });
    await useApp.getState().reconnectHost("linux-box");
    expect(mocks.status).not.toHaveBeenCalled();
    useApp.setState({ activeId: "a" });
    await useApp.getState().reconnectHost("linux-box");
    expect(mocks.status).toHaveBeenCalledTimes(1);
  });
});

describe("the computer badge", () => {
  it("stays grey from a failure until a connection succeeds", () => {
    const { applyHostState } = useApp.getState();
    const down = () => useApp.getState().hosts["linux-box"]?.down;
    applyHostState({ host: "linux-box", gen: 1, state: "alive" });
    expect(down()).toBe(false);
    applyHostState({ host: "linux-box", gen: 2, state: "failed" });
    expect(down()).toBe(true);
    applyHostState({ host: "linux-box", gen: 3, state: "connecting" }); // Reconnect under way
    expect(down()).toBe(true);
    applyHostState({ host: "linux-box", gen: 3, state: "alive" });
    expect(down()).toBe(false);
    applyHostState({ host: "linux-box", gen: 3, state: "idle" }); // closed: not a failure
    expect(down()).toBe(false);
  });
});
