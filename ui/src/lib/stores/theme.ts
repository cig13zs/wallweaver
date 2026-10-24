import type { MotionPref, ThemePref } from "../daemon/types";

const KEY = "ww.appearance";

export function resolveTheme(pref: ThemePref, systemDark: boolean): "dark" | "light" {
  if (pref === "system") return systemDark ? "dark" : "light";
  return pref;
}

/** Value for <html data-motion>: undefined follows the OS setting. */
export function motionAttr(pref: MotionPref): "reduce" | "full" | undefined {
  return pref === "system" ? undefined : pref;
}

export interface Appearance { theme: ThemePref; motion: MotionPref }

export function loadAppearance(): Appearance {
  try {
    const v = JSON.parse(localStorage.getItem(KEY) ?? "null") as Partial<Appearance> | null;
    return { theme: v?.theme ?? "dark", motion: v?.motion ?? "system" };
  } catch {
    return { theme: "dark", motion: "system" };
  }
}

let mq: MediaQueryList | null = null;
let current: Appearance = { theme: "dark", motion: "system" };

/** Applies theme and motion to <html> and remembers them so the next launch paints without a flash. */
export function applyAppearance(a: Appearance): void {
  current = a;
  const root = document.documentElement;
  mq ??= window.matchMedia("(prefers-color-scheme: dark)");
  root.dataset.theme = resolveTheme(a.theme, mq.matches);
  const m = motionAttr(a.motion);
  if (m) root.dataset.motion = m; else delete root.dataset.motion;
  try { localStorage.setItem(KEY, JSON.stringify(a)); } catch { /* storage can be unavailable */ }
}

export function watchSystemTheme(): () => void {
  mq ??= window.matchMedia("(prefers-color-scheme: dark)");
  const onChange = () => { if (current.theme === "system") applyAppearance(current); };
  mq.addEventListener("change", onChange);
  return () => mq?.removeEventListener("change", onChange);
}
