import { describe, expect, it } from "vitest";
import { motionAttr, resolveTheme } from "./theme";

describe("appearance", () => {
  it("resolves the theme", () => {
    expect(resolveTheme("dark", false)).toBe("dark");
    expect(resolveTheme("light", true)).toBe("light");
    expect(resolveTheme("system", true)).toBe("dark");
    expect(resolveTheme("system", false)).toBe("light");
  });
  it("maps the motion preference to the html attribute", () => {
    expect(motionAttr("system")).toBeUndefined();
    expect(motionAttr("reduce")).toBe("reduce");
    expect(motionAttr("full")).toBe("full");
  });
});
