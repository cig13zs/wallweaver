import type { Cost } from "./daemon/types";

export const COST_RANK: Record<Cost, number> = { low: 0, med: 1, high: 2 };
const COST_LABEL: Record<Cost, string> = { low: "Light", med: "Medium", high: "Heavy" };
const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

export function costLabel(c: Cost): string {
  return COST_LABEL[c];
}

/** 950 → "950", 9800 → "9.8k", 12400 → "12.4k", 21000 → "21k", 1250000 → "1.3M". */
export function formatCount(n: number): string {
  if (!Number.isFinite(n) || n < 1000) return String(Math.max(0, Math.round(n || 0)));
  if (n >= 1_000_000) return `${trim1(n / 1_000_000)}M`;
  const k = n / 1000;
  return k < 10 ? `${k.toFixed(1)}k` : `${trim1(k)}k`;
}

function trim1(x: number): string {
  const s = (Math.round(x * 10) / 10).toFixed(1);
  return s.endsWith(".0") ? s.slice(0, -2) : s;
}

/** Binary units, one decimal under 10: 820 → "0.8 KB", 14336 → "14 KB", 1887437 → "1.8 MB". */
export function formatBytes(bytes: number): string {
  const units = ["KB", "MB", "GB"];
  let v = Math.max(0, bytes) / 1024;
  let i = 0;
  while (v >= 1024 && i < units.length - 1) { v /= 1024; i++; }
  const num = v < 10 ? v.toFixed(1) : String(Math.round(v));
  return `${num} ${units[i]}`;
}

/** 1.94 → "1.9%". Values are already percentages. */
export function formatPct(v: number, digits = 1): string {
  return `${v.toFixed(digits)}%`;
}

/** "2026-10-18" → "Oct 18, 2026", independent of the system locale. */
export function formatDate(iso: string): string {
  const m = /^(\d{4})-(\d{2})-(\d{2})/.exec(iso);
  if (!m) return iso;
  const month = MONTHS[Number(m[2]) - 1];
  return month ? `${month} ${Number(m[3])}, ${m[1]}` : iso;
}

export function formatResolution(w: number, h: number): string {
  return `${w}×${h}`;
}

/** [2] → "display 2", [2, 3] → "displays 2 and 3", [1, 2, 3] → "displays 1, 2 and 3". */
export function displayList(ids: number[]): string {
  const sorted = [...ids].sort((a, b) => a - b);
  if (sorted.length === 0) return "no displays";
  if (sorted.length === 1) return `display ${sorted[0]}`;
  const last = sorted[sorted.length - 1];
  return `displays ${sorted.slice(0, -1).join(", ")} and ${last}`;
}

/** Slider readout: multipliers as "0.6×", fractions as "70%". */
export function formatSliderValue(value: number, unit: "x" | "%"): string {
  if (unit === "%") return `${Math.round(value * 100)}%`;
  return `${trim1(value)}×`;
}

/** Position of value within [min, max] as 0–100, clamped. */
export function percentOf(value: number, min: number, max: number): number {
  if (max <= min) return 0;
  return Math.min(100, Math.max(0, ((value - min) / (max - min)) * 100));
}

export function pluralize(n: number, one: string, many = `${one}s`): string {
  return `${n} ${n === 1 ? one : many}`;
}

/** Frame-rate cap stops on the performance slider. 0 means uncapped. */
export const FPS_STOPS = [15, 24, 30, 60, 120, 0] as const;

export function fpsLabel(fps: number): string {
  return fps === 0 ? "Max" : `${fps} fps`;
}

/** Two-letter initials for an avatar: "mira" → "MI", "soft.systems" → "SO". */
export function initials(handle: string): string {
  return handle.replace(/[^a-z0-9]/gi, "").slice(0, 2).toUpperCase() || "?";
}

const AVATARS = ["#ffb7a8,#c6a8ff", "#8fe3d3,#86c5ff", "#f5c56e,#ff8b7e", "#c6a8ff,#8fe3d3", "#ff9cc0,#f5c56e"];

/** Stable avatar gradient per handle. */
export function avatarGradient(handle: string): string {
  let h = 0;
  for (const ch of handle) h = (h * 31 + ch.charCodeAt(0)) >>> 0;
  return `linear-gradient(135deg,${AVATARS[h % AVATARS.length]})`;
}

/** SVG path for a sparkline in a width×height box; history values run 0..max, higher draws higher. */
export function sparkPath(history: number[], max = 44, width = 128, height = 44): string {
  if (history.length === 0 || max <= 0) return "";
  const step = history.length > 1 ? width / (history.length - 1) : 0;
  return history
    .map((v, i) => `${i ? "L" : "M"}${round2(i * step)} ${round2(height - (Math.min(max, Math.max(0, v)) / max) * height)}`)
    .join(" ");
}

function round2(n: number): number {
  return Math.round(n * 100) / 100;
}
