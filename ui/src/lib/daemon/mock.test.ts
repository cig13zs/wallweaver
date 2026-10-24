import { describe, expect, it } from "vitest";
import { MockDaemon } from "./mock";
import { inShell } from "./index";

const d = () => new MockDaemon({ live: false });

describe("MockDaemon gallery", () => {
  it("returns the whole gallery by default", async () => {
    expect((await d().gallery({})).length).toBe(16);
  });
  it("filters by category, capability, cost and text", async () => {
    const m = d();
    expect((await m.gallery({ category: "space" })).map((w) => w.id)).toEqual(["constellation", "deep-field", "saturn-hours"]);
    expect((await m.gallery({ category: "audio" })).every((w) => w.uses.includes("audio"))).toBe(true);
    expect((await m.gallery({ category: "interactive" })).every((w) => w.uses.includes("mouse"))).toBe(true);
    expect((await m.gallery({ lowImpactOnly: true })).every((w) => w.cost === "low")).toBe(true);
    expect((await m.gallery({ text: "  PETAL " })).map((w) => w.id)).toEqual(["petal-drift"]);
    expect(await m.gallery({ category: "space", lowImpactOnly: true })).toEqual([]);
  });
  it("sorts", async () => {
    const m = d();
    const likes = (await m.gallery({ sort: "likes" })).map((w) => w.likes);
    expect(likes).toEqual([...likes].sort((a, b) => b - a));
    expect((await m.gallery({ sort: "lightest" }))[0]?.cost).toBe("low");
    expect((await m.gallery({ sort: "lightest" })).at(-1)?.cost).toBe("high");
  });
  it("hands out copies, not its own state", async () => {
    const m = d();
    const a = await m.gallery({});
    a[0]!.title = "changed";
    expect((await m.gallery({}))[0]!.title).not.toBe("changed");
  });
});

describe("MockDaemon install", () => {
  it("records exactly the permissions the user allowed", async () => {
    const m = d();
    const req = await m.installRequest("weather-glass");
    expect(req.permissions.map((p) => p.key)).toEqual(["audio", "network", "system.stats", "input.mouse"]);
    const res = await m.install("weather-glass", ["audio", "network", "input.mouse", "calendar"]);
    expect(res.granted).toEqual(["audio", "network", "input.mouse"]);
    expect(res.denied).toEqual(["system.stats"]);
    expect((await m.gallery({ text: "weather" }))[0]?.installed).toBe(true);
  });
  it("derives a request from what a wallpaper uses", async () => {
    const req = await d().installRequest("constellation");
    expect(req.permissions.map((p) => p.key)).toEqual(["input.mouse"]);
    expect(req.alwaysBlocked.length).toBeGreaterThan(0);
  });
  it("rejects unknown wallpapers", async () => {
    await expect(d().installRequest("nope")).rejects.toThrow(/unknown wallpaper/);
  });
});

describe("MockDaemon wallpapers and displays", () => {
  it("serves full details and synthesizes the rest", async () => {
    const m = d();
    expect((await m.wallpaper("aurora-loom")).properties.length).toBeGreaterThan(0);
    const s = await m.wallpaper("contour");
    expect(s.title).toBe("Contour");
    expect(s.creator.handle).toBe("fieldnotes");
    expect(s.moreFromCreator.some((w) => w.id === "contour")).toBe(false);
  });
  it("applies to the chosen displays and updates now playing", async () => {
    const m = d();
    const res = await m.apply({ wallpaperId: "contour", displays: [1, 3], mode: "each", values: {}, paletteId: "" });
    expect(res.applied).toEqual([1, 3]);
    const ds = await m.displays();
    expect(ds.filter((x) => x.wallpaperId === "contour").map((x) => x.id)).toEqual([1, 3]);
    expect((await m.status()).nowPlaying?.title).toBe("Contour");
  });
  it("counts saved presets", async () => {
    const m = d();
    const n = await m.savePreset({ wallpaperId: "aurora-loom", name: "Mine", values: { speed: 1 }, paletteId: "ember" });
    expect(n).toBe(4);
    expect(await m.savePreset({ wallpaperId: "aurora-loom", name: "Mine", values: { speed: 2 }, paletteId: "ember" })).toBe(4);
    await expect(m.savePreset({ wallpaperId: "aurora-loom", name: " ", values: {}, paletteId: "" })).rejects.toThrow();
  });
});

describe("MockDaemon power", () => {
  it("validates pause rule actions", async () => {
    const m = d();
    const p = await m.setPauseRule("battery", "Pause");
    expect(p.rules.find((r) => r.id === "battery")?.action).toBe("Pause");
    await expect(m.setPauseRule("battery", "Explode")).rejects.toThrow(/unsupported/);
    await expect(m.setPauseRule("nope", "Pause")).rejects.toThrow(/unknown/);
  });
  it("presets change the frame cap", async () => {
    const m = d();
    expect((await m.setPreset("saver")).settings.fpsCap).toBe(30);
    expect((await m.setPreset("quality")).settings.fpsCap).toBe(60);
  });
  it("keeps hardware decode availability out of the user's hands", async () => {
    const m = d();
    const cur = (await m.performance()).settings;
    const next = await m.setPerfSettings({ ...cur, hwDecodeAvailable: false, fpsCap: 120 });
    expect(next.settings.hwDecodeAvailable).toBe(true);
    expect(next.settings.fpsCap).toBe(120);
  });
  it("pause all stops every display and resume restores them", async () => {
    const m = d();
    expect((await m.pauseAll(true)).displaysLive).toBe(0);
    expect((await m.performance()).displays.every((x) => x.fps === 0)).toBe(true);
    expect((await m.pauseAll(false)).displaysLive).toBe(2);
    expect((await m.performance()).displays.some((x) => x.fps > 0)).toBe(true);
  });
  it("adds and updates app rules", async () => {
    const m = d();
    expect((await m.setAppRule("editor.exe", "Pause")).appRules.at(-1)).toEqual({ exe: "editor.exe", icon: "window", action: "Pause" });
    expect((await m.setAppRule("GAME.exe", "Pause")).appRules.find((r) => r.exe === "game.exe")?.action).toBe("Pause");
    await expect(m.setAppRule("  ", "Pause")).rejects.toThrow();
  });
  it("live mode moves the meters, static mode does not", async () => {
    const live = new MockDaemon({ live: true });
    const a = (await live.performance()).cpu.history;
    const b = (await live.performance()).cpu.history;
    expect(a).not.toEqual(b);
    const still = d();
    expect((await still.performance()).cpu).toEqual((await still.performance()).cpu);
  });
});

describe("MockDaemon creator", () => {
  it("declaring an origin clears the lint issue", async () => {
    const m = d();
    const before = await m.creatorProject();
    expect(before.issues).toHaveLength(1);
    const after = await m.declareOrigin(before.issues[0]!.origin);
    expect(after.issues).toHaveLength(0);
    expect(after.declared).toContain("network");
  });
  it("packs to name-version.wwp", async () => {
    expect((await d().packAndSign()).file).toBe("aurora-loom-1.2.0.wwp");
  });
});

describe("daemon selection", () => {
  it("detects the desktop shell", () => {
    expect(inShell({})).toBe(false);
    expect(inShell({ __TAURI_INTERNALS__: {} })).toBe(true);
  });
});
