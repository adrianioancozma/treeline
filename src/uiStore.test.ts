import { beforeEach, describe, expect, it } from "vitest";
import { forgetSections, setSectionOpen, useUi } from "./uiStore";

describe("sidebar sections are remembered per tab", () => {
  beforeEach(() => useUi.setState({ sections: {} }));

  it("keeps each tab's choice separate", () => {
    setSectionOpen("a", "Branches", false);
    setSectionOpen("b", "Branches", true);
    const s = useUi.getState().sections;
    expect(s.a.Branches).toBe(false);
    expect(s.b.Branches).toBe(true);
  });

  it("leaves sections the user never touched undecided", () => {
    setSectionOpen("a", "Tags", true);
    expect(useUi.getState().sections.b?.Tags).toBeUndefined();
    expect(useUi.getState().sections.a.Stashes).toBeUndefined();
  });

  it("forgets a closed tab and nothing else", () => {
    setSectionOpen("a", "Tags", true);
    setSectionOpen("b", "Tags", false);
    forgetSections("a");
    expect(useUi.getState().sections.a).toBeUndefined();
    expect(useUi.getState().sections.b.Tags).toBe(false);
  });
});
