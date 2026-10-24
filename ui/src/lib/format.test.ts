import { describe, expect, it } from "vitest";
import {
  avatarGradient, costLabel, displayList, formatBytes, formatCount, formatDate, formatPct, formatResolution,
  formatSliderValue, fpsLabel, initials, percentOf, pluralize, sparkPath,
} from "./format";

describe("formatCount", () => {
  it.each([
    [0, "0"], [950, "950"], [2900, "2.9k"], [8000, "8.0k"], [9800, "9.8k"],
    [12400, "12.4k"], [21000, "21k"], [32000, "32k"], [1_250_000, "1.3M"], [-5, "0"], [Number.NaN, "0"],
  ])("%d → %s", (n, out) => expect(formatCount(n)).toBe(out));
});

describe("formatBytes", () => {
  it.each([
    [820, "0.8 KB"], [2150, "2.1 KB"], [14336, "14 KB"], [217088, "212 KB"],
    [1887437, "1.8 MB"], [4404019, "4.2 MB"], [52428800, "50 MB"], [3 * 1024 ** 3, "3.0 GB"],
  ])("%d → %s", (n, out) => expect(formatBytes(n)).toBe(out));
});

describe("small formatters", () => {
  it("percent", () => { expect(formatPct(1.94)).toBe("1.9%"); expect(formatPct(2, 0)).toBe("2%"); });
  it("cost labels", () => { expect(costLabel("low")).toBe("Light"); expect(costLabel("med")).toBe("Medium"); expect(costLabel("high")).toBe("Heavy"); });
  it("dates ignore the system locale", () => { expect(formatDate("2026-10-18")).toBe("Oct 18, 2026"); expect(formatDate("nope")).toBe("nope"); expect(formatDate("2026-13-01")).toBe("2026-13-01"); });
  it("resolution", () => expect(formatResolution(2560, 1440)).toBe("2560×1440"));
  it("slider values", () => { expect(formatSliderValue(0.6, "x")).toBe("0.6×"); expect(formatSliderValue(2, "x")).toBe("2×"); expect(formatSliderValue(0.7, "%")).toBe("70%"); });
  it("percentOf clamps", () => { expect(percentOf(0.6, 0.1, 3)).toBeCloseTo(17.24, 1); expect(percentOf(9, 0, 1)).toBe(100); expect(percentOf(1, 1, 1)).toBe(0); });
  it("fps labels", () => { expect(fpsLabel(60)).toBe("60 fps"); expect(fpsLabel(0)).toBe("Max"); });
  it("pluralize", () => { expect(pluralize(1, "shader")).toBe("1 shader"); expect(pluralize(3, "webview")).toBe("3 webviews"); });
  it("initials", () => { expect(initials("mira")).toBe("MI"); expect(initials("soft.systems")).toBe("SO"); expect(initials("..")).toBe("?"); });
  it("avatar gradients are stable per handle", () => { expect(avatarGradient("mira")).toBe(avatarGradient("mira")); expect(avatarGradient("mira")).toMatch(/^linear-gradient/); });
});

describe("displayList", () => {
  it("reads naturally", () => {
    expect(displayList([])).toBe("no displays");
    expect(displayList([2])).toBe("display 2");
    expect(displayList([3, 2])).toBe("displays 2 and 3");
    expect(displayList([1, 2, 3])).toBe("displays 1, 2 and 3");
  });
});

describe("sparkPath", () => {
  it("maps higher values higher up", () => {
    expect(sparkPath([0, 44], 44, 128, 44)).toBe("M0 44 L128 0");
    expect(sparkPath([22], 44)).toBe("M0 22");
    expect(sparkPath([])).toBe("");
    expect(sparkPath([100, -5], 44, 10, 44)).toBe("M0 0 L10 44");
  });
});
