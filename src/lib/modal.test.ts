import { beforeEach, describe, expect, it } from "vitest";
import { closeModal, isTopModal, modalKeyAction, nextFocusIndex, openModal, resetModals } from "./modal";

describe("modal stack", () => {
  beforeEach(() => resetModals());

  it("only the most recently opened dialog is on top", () => {
    openModal("a");
    openModal("b");
    expect(isTopModal("b")).toBe(true);
    expect(isTopModal("a")).toBe(false);
  });

  it("closing the top dialog gives the one below back the keyboard", () => {
    openModal("a");
    openModal("b");
    closeModal("b");
    expect(isTopModal("a")).toBe(true);
  });

  it("closing a dialog below the top leaves the top alone", () => {
    openModal("a");
    openModal("b");
    closeModal("a");
    expect(isTopModal("b")).toBe(true);
    expect(isTopModal("a")).toBe(false);
  });

  it("closing an unknown dialog changes nothing", () => {
    openModal("a");
    closeModal("zzz");
    expect(isTopModal("a")).toBe(true);
  });
});

describe("modalKeyAction", () => {
  it("Escape closes the top dialog", () => {
    expect(modalKeyAction("Escape", { top: true, busy: false })).toBe("close");
  });
  it("Escape does nothing while an operation runs", () => {
    expect(modalKeyAction("Escape", { top: true, busy: true })).toBeNull();
  });
  it("Escape does nothing for a dialog that is not on top", () => {
    expect(modalKeyAction("Escape", { top: false, busy: false })).toBeNull();
  });
  it("Tab is kept inside the top dialog, also while busy", () => {
    expect(modalKeyAction("Tab", { top: true, busy: false })).toBe("trap");
    expect(modalKeyAction("Tab", { top: true, busy: true })).toBe("trap");
    expect(modalKeyAction("Tab", { top: false, busy: false })).toBeNull();
  });
  it("other keys are left to the dialog's own content", () => {
    expect(modalKeyAction("Enter", { top: true, busy: false })).toBeNull();
    expect(modalKeyAction("a", { top: true, busy: false })).toBeNull();
  });
});

describe("nextFocusIndex", () => {
  it("moves forward and wraps from the last to the first", () => {
    expect(nextFocusIndex(3, 0, false)).toBe(1);
    expect(nextFocusIndex(3, 2, false)).toBe(0);
  });
  it("moves backward and wraps from the first to the last", () => {
    expect(nextFocusIndex(3, 1, true)).toBe(0);
    expect(nextFocusIndex(3, 0, true)).toBe(2);
  });
  it("starts at the first (or last, going back) when focus is outside the dialog", () => {
    expect(nextFocusIndex(4, -1, false)).toBe(0);
    expect(nextFocusIndex(4, -1, true)).toBe(3);
  });
  it("returns -1 when there is nothing to focus", () => {
    expect(nextFocusIndex(0, -1, false)).toBe(-1);
    expect(nextFocusIndex(0, 0, true)).toBe(-1);
  });
});
