import { daemon } from "./daemon";
import type { WallpaperSummary } from "./daemon/types";
import { displayList } from "./format";
import { refreshStatus } from "./stores/status";
import { attempt, toast } from "./stores/toast";

/** Applies an installed wallpaper to every display with its saved settings. */
export async function applyEverywhere(w: Pick<WallpaperSummary, "id" | "title">): Promise<void> {
  await attempt(async () => {
    const d = await daemon();
    const displays = await d.displays();
    const detail = await d.wallpaper(w.id);
    const values = Object.fromEntries(detail.properties.map((p) => [p.key, p.value]));
    const res = await d.apply({ wallpaperId: w.id, displays: displays.map((x) => x.id), mode: "same", values, paletteId: detail.paletteId });
    await refreshStatus();
    toast(`${w.title} is on ${displayList(res.applied)}`);
  }, "Couldn't set the wallpaper");
}
