import type { ArtSpec } from "../daemon/types";

/** Stable cache key for one rendered thumbnail at one pixel size. */
export function artKey(spec: ArtSpec, w: number, h: number, dpr: number): string {
  const pal = spec.pal ? spec.pal.join(",") : "";
  return `${spec.kind}|${spec.seed}|${spec.variant ?? ""}|${pal}|${Math.round(w)}x${Math.round(h)}@${dpr}`;
}

/** Small LRU so the same artwork at the same size is drawn exactly once. */
export class LruCache<V> {
  private map = new Map<string, V>();
  constructor(private readonly limit: number) {}
  get size(): number { return this.map.size; }
  get(key: string): V | undefined {
    const v = this.map.get(key);
    if (v !== undefined) { this.map.delete(key); this.map.set(key, v); }
    return v;
  }
  set(key: string, value: V): void {
    this.map.delete(key);
    this.map.set(key, value);
    while (this.map.size > this.limit) {
      const oldest = this.map.keys().next().value as string;
      this.map.delete(oldest);
    }
  }
}

const cache = new LruCache<HTMLCanvasElement>(96);
let generators: Promise<typeof import("./generators")> | null = null;

/** Renders (or reuses) the artwork for spec at w×h CSS pixels. Generators load on first use. */
export async function renderArt(spec: ArtSpec, w: number, h: number, dpr: number): Promise<HTMLCanvasElement> {
  const key = artKey(spec, w, h, dpr);
  const hit = cache.get(key);
  if (hit) return hit;
  generators ??= import("./generators");
  const { drawArt } = await generators;
  const again = cache.get(key);
  if (again) return again;
  const cv = document.createElement("canvas");
  cv.width = Math.max(1, Math.round(w * dpr));
  cv.height = Math.max(1, Math.round(h * dpr));
  const ctx = cv.getContext("2d");
  if (ctx) {
    ctx.scale(dpr, dpr);
    drawArt(ctx, spec.kind, spec.seed, w, h, dpr, { variant: spec.variant, pal: spec.pal ?? null });
  }
  cache.set(key, cv);
  return cv;
}
