import { describe, expect, it } from "vitest";
import { href, parseHash } from "./router";

describe("parseHash", () => {
  it("defaults to discover", () => {
    expect(parseHash("")).toEqual({ name: "discover", params: [], query: {} });
    expect(parseHash("#/")).toEqual({ name: "discover", params: [], query: {} });
    expect(parseHash("#/nowhere")).toEqual({ name: "discover", params: [], query: {} });
  });
  it("reads params and query", () => {
    expect(parseHash("#/wallpaper/aurora-loom?install=1")).toEqual({ name: "wallpaper", params: ["aurora-loom"], query: { install: "1" } });
    expect(parseHash("#/discover?install=weather-glass")).toEqual({ name: "discover", params: [], query: { install: "weather-glass" } });
    expect(parseHash("#/performance")).toEqual({ name: "performance", params: [], query: {} });
  });
  it("needs an id for the detail screen", () => {
    expect(parseHash("#/wallpaper").name).toBe("discover");
  });
  it("decodes ids and survives bad escapes", () => {
    expect(parseHash("#/wallpaper/night%20drive").params).toEqual(["night drive"]);
    expect(parseHash("#/wallpaper/%E0%A4%A").params).toEqual(["%E0%A4%A"]);
  });
  it("round-trips with href", () => {
    const h = href("wallpaper", ["a b"], { install: "1" });
    expect(h).toBe("#/wallpaper/a%20b?install=1");
    expect(parseHash(h)).toEqual({ name: "wallpaper", params: ["a b"], query: { install: "1" } });
  });
});
