import { readable, type Readable } from "svelte/store";

export type ScreenName =
  | "discover" | "wallpaper" | "performance" | "creator" | "supporter" | "settings"
  | "library" | "playlists" | "permissions" | "themes";

export interface Route {
  name: ScreenName;
  /** Path segments after the screen name, e.g. the wallpaper id. */
  params: string[];
  query: Record<string, string>;
}

const SCREENS: readonly ScreenName[] = [
  "discover", "wallpaper", "performance", "creator", "supporter", "settings", "library", "playlists", "permissions", "themes",
];

/** "#/wallpaper/aurora-loom?tab=presets" → { name: "wallpaper", params: ["aurora-loom"], query: { tab: "presets" } }. */
export function parseHash(hash: string): Route {
  const raw = hash.replace(/^#\/?/, "");
  const [path = "", qs = ""] = raw.split("?", 2);
  const parts = path.split("/").filter(Boolean).map(safeDecode);
  const head = parts[0] as ScreenName | undefined;
  const query: Record<string, string> = {};
  for (const [k, v] of new URLSearchParams(qs)) query[k] = v;
  if (!head || !SCREENS.includes(head)) return { name: "discover", params: [], query };
  if (head === "wallpaper" && !parts[1]) return { name: "discover", params: [], query };
  return { name: head, params: parts.slice(1), query };
}

export function href(name: ScreenName, params: string[] = [], query: Record<string, string> = {}): string {
  const path = [name, ...params.map(encodeURIComponent)].join("/");
  const qs = new URLSearchParams(query).toString();
  return `#/${path}${qs ? `?${qs}` : ""}`;
}

function safeDecode(s: string): string {
  try { return decodeURIComponent(s); } catch { return s; }
}

export const route: Readable<Route> = readable(parseHash(typeof location === "undefined" ? "" : location.hash), (set) => {
  if (typeof window === "undefined") return;
  const update = () => set(parseHash(location.hash));
  window.addEventListener("hashchange", update);
  return () => window.removeEventListener("hashchange", update);
});

export function navigate(to: string): void {
  if (location.hash !== to) location.hash = to;
}
