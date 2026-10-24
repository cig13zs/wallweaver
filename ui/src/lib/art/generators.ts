// Procedural placeholder wallpapers. Same kind + seed always draws the same picture,
// so every screen shows the same artwork without shipping any images.

export type Ctx = CanvasRenderingContext2D;
export type Rand = () => number;
type Pt = [number, number];
type Stop = string | [number, string];
export interface ArtOpts { variant?: string; pal?: string[] | null }
type Gen = (ctx: Ctx, w: number, h: number, R: Rand, o: ArtOpts, dpr: number) => void;

function makeCanvas(): HTMLCanvasElement { return document.createElement("canvas"); }

// ---------- helpers ----------
export function rng(seed: number): () => number {
  let a = (seed * 2654435761) >>> 0 || 1;
  return function () {
    a |= 0; a = (a + 0x6d2b79f5) | 0;
    let t = Math.imul(a ^ (a >>> 15), 1 | a);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}
function hash(x: number, y: number, s: number): number { let h = x * 374761393 + y * 668265263 + s * 982451653; h = (h ^ (h >>> 13)) * 1274126177; return ((h ^ (h >>> 16)) >>> 0) / 4294967296; }
function smooth(t: number): number { return t * t * (3 - 2 * t); }
function vnoise(x: number, y: number, s: number): number {
  const xi = Math.floor(x), yi = Math.floor(y), xf = x - xi, yf = y - yi;
  const a = hash(xi, yi, s), b = hash(xi + 1, yi, s), c = hash(xi, yi + 1, s), d = hash(xi + 1, yi + 1, s);
  const u = smooth(xf), v = smooth(yf);
  return a + (b - a) * u + (c - a) * v + (a - b - c + d) * u * v;
}
function fbm(x: number, y: number, s: number, o = 4): number { let v = 0, amp = 0.5, f = 1; for (let i = 0; i < o; i++) { v += amp * vnoise(x * f, y * f, s + i * 17); f *= 2; amp *= 0.5; } return v; }
function hex(c: string): number[] { c = c.replace("#", ""); return [parseInt(c.slice(0, 2), 16), parseInt(c.slice(2, 4), 16), parseInt(c.slice(4, 6), 16)]; }
function mix(c1: string, c2: string, t: number): string { const a = hex(c1), b = hex(c2); return `rgb(${a.map((v, i) => Math.round(v + (b[i] - v) * t)).join(",")})`; }
function ramp(stops: string[], t: number): string { t = Math.max(0, Math.min(0.9999, t)); const n = stops.length - 1, i = Math.floor(t * n); return mix(stops[i], stops[i + 1], t * n - i); }
function lin(ctx: Ctx, x0: number, y0: number, x1: number, y1: number, stops: Stop[]): CanvasGradient { const g = ctx.createLinearGradient(x0, y0, x1, y1); stops.forEach((c, i) => g.addColorStop(Array.isArray(c) ? c[0] : i / (stops.length - 1), Array.isArray(c) ? c[1] : c)); return g; }
function rad(ctx: Ctx, x: number, y: number, r: number, stops: Stop[]): CanvasGradient { const g = ctx.createRadialGradient(x, y, 0, x, y, r); stops.forEach((c, i) => g.addColorStop(Array.isArray(c) ? c[0] : i / (stops.length - 1), Array.isArray(c) ? c[1] : c)); return g; }
function stars(ctx: Ctx, w: number, h: number, R: Rand, n: number, maxY = 1, alpha = 1): void {
  for (let i = 0; i < n; i++) {
    const x = R() * w, y = R() * h * maxY, r = R() < 0.92 ? R() * 0.9 + 0.2 : R() * 1.4 + 0.8;
    ctx.fillStyle = `rgba(255,255,255,${(0.25 + R() * 0.75) * alpha})`;
    ctx.beginPath(); ctx.arc(x, y, r, 0, 7); ctx.fill();
  }
}
function ridge(R: Rand, w: number, base: number, amp: number, rough: number, n = 64): Pt[] {
  // midpoint displacement on [0,w]
  let arr: number[] = [base + (R() - 0.5) * amp, base + (R() - 0.5) * amp];
  let a = amp;
  while (arr.length < n) {
    const next: number[] = [];
    for (let i = 0; i < arr.length - 1; i++) { next.push(arr[i], (arr[i] + arr[i + 1]) / 2 + (R() - 0.5) * a); }
    next.push(arr[arr.length - 1]); arr = next; a *= rough;
  }
  return arr.map((y, i): Pt => [i / (arr.length - 1) * w, y]);
}
function fillRidge(ctx: Ctx, pts: Pt[], w: number, h: number, style: string | CanvasGradient): void { ctx.fillStyle = style; ctx.beginPath(); ctx.moveTo(0, h); pts.forEach(p => ctx.lineTo(p[0], p[1])); ctx.lineTo(w, h); ctx.closePath(); ctx.fill(); }
function grain(ctx: Ctx, w: number, h: number, R: Rand, alpha = 0.06): void {
  const g = makeCanvas(); g.width = g.height = 128; const gx = g.getContext("2d")!; const id = gx.createImageData(128, 128);
  for (let i = 0; i < id.data.length; i += 4) { const v = R() * 255; id.data[i] = id.data[i + 1] = id.data[i + 2] = v; id.data[i + 3] = 255 * alpha; }
  gx.putImageData(id, 0, 0); ctx.fillStyle = ctx.createPattern(g, "repeat") ?? "transparent"; ctx.fillRect(0, 0, w, h);
}
function pixelField(ctx: Ctx, w: number, h: number, dpr: number, scale: number, fn: (u: number, v: number, W: number, H: number) => number[]): void {
  const W = Math.max(8, Math.round(w * dpr / scale)), H = Math.max(8, Math.round(h * dpr / scale));
  const c = makeCanvas(); c.width = W; c.height = H; const x = c.getContext("2d")!; const id = x.createImageData(W, H);
  for (let j = 0; j < H; j++) for (let i = 0; i < W; i++) { const col = fn(i / W, j / H, W, H); const k = (j * W + i) * 4; id.data[k] = col[0]; id.data[k + 1] = col[1]; id.data[k + 2] = col[2]; id.data[k + 3] = 255; }
  x.putImageData(id, 0, 0); ctx.imageSmoothingEnabled = true; ctx.drawImage(c, 0, 0, w, h);
}

// Blur without ctx.filter, which some webviews lack: draw small, then scale up in 2x steps.
function soft(ctx: Ctx, w: number, h: number, radius: number, draw: (c: Ctx) => void): void {
  const k = Math.max(1, radius * 0.8);
  let cw = Math.max(2, Math.round(w / k)), ch = Math.max(2, Math.round(h / k));
  let src = makeCanvas(); src.width = cw; src.height = ch;
  const sx = src.getContext("2d")!; sx.scale(cw / w, ch / h); draw(sx);
  while (cw * 2 < w) {
    const n = makeCanvas(); n.width = cw * 2; n.height = ch * 2;
    const nx = n.getContext("2d")!; nx.imageSmoothingEnabled = true; nx.imageSmoothingQuality = "high";
    nx.drawImage(src, 0, 0, n.width, n.height);
    src = n; cw *= 2; ch *= 2;
  }
  ctx.imageSmoothingEnabled = true; ctx.imageSmoothingQuality = "high"; ctx.drawImage(src, 0, 0, w, h);
}

// ---------- generators ----------
const G: Record<string, Gen> = {};

G.aurora = (ctx, w, h, R) => {
  ctx.fillStyle = lin(ctx, 0, 0, 0, h, ["#040713", "#08142c", "#0e2340"]); ctx.fillRect(0, 0, w, h);
  stars(ctx, w, h, R, w * h / 900, 0.7);
  // curtains drawn as 1px columns on a narrow offscreen canvas, then stretched (no seams)
  const CW = 420, CH = Math.round(h), off = makeCanvas(); off.width = CW; off.height = CH; const ox = off.getContext("2d")!;
  ox.globalCompositeOperation = "lighter";
  const cols = [["#5effc4", "#3aa7ff"], ["#9a8cff", "#ff8fd2"], ["#62f0d8", "#a596ff"]];
  for (let k = 0; k < 3; k++) {
    const base = h * (0.30 + k * 0.12), A = h * (0.06 + R() * 0.06), f = (1.5 + R() * 2) / CW * Math.PI * 2, ph = R() * 6, f2 = (4 + R() * 3) / CW * 6.28;
    const [c1, c2] = cols[k % cols.length];
    for (let x = 0; x < CW; x++) {
      const y = base + A * Math.sin(x * f + ph) + A * 0.4 * Math.sin(x * f2 + k);
      const len = h * (0.18 + 0.14 * (0.5 + 0.5 * Math.sin(x * f * 1.7 + ph * 2))) * (0.85 + 0.3 * vnoise(x * 0.15, k, 3));
      const g = ox.createLinearGradient(0, y - len, 0, y + h * 0.02);
      const c = mix(c1, c2, x / CW).replace("rgb", "rgba").replace(")", "");
      g.addColorStop(0, c + ",0)"); g.addColorStop(0.72, c + ",0.5)"); g.addColorStop(1, c + ",0)");
      ox.fillStyle = g; ox.fillRect(x, y - len, 1, len + h * 0.02);
    }
  }
  ctx.globalCompositeOperation = "lighter"; ctx.imageSmoothingEnabled = true; ctx.drawImage(off, 0, 0, w, h);
  ctx.globalCompositeOperation = "source-over";
  const r1 = ridge(R, w, h * 0.84, h * 0.12, 0.55); fillRidge(ctx, r1, w, h, "#0a1426");
  const r2 = ridge(R, w, h * 0.92, h * 0.08, 0.5); fillRidge(ctx, r2, w, h, "#050a15");
};

G.nebula = (ctx, w, h, R) => {
  ctx.fillStyle = "#06050d"; ctx.fillRect(0, 0, w, h);
  ctx.globalCompositeOperation = "lighter";
  const cols = ["#ff6fb5", "#7b6cff", "#3ec7ff", "#ffb36b", "#b06bff"];
  for (let i = 0; i < 14; i++) {
    const t = R(); const x = w * (0.1 + t * 0.8) + (R() - 0.5) * w * 0.2, y = h * (0.75 - t * 0.5) + (R() - 0.5) * h * 0.3;
    const r = Math.max(w, h) * (0.12 + R() * 0.22); const c = cols[i % cols.length];
    ctx.fillStyle = rad(ctx, x, y, r, [c + "50", c + "18", c + "00"]); ctx.fillRect(0, 0, w, h);
  }
  ctx.globalCompositeOperation = "source-over";
  stars(ctx, w, h, R, w * h / 400);
  for (let i = 0; i < 6; i++) { const x = R() * w, y = R() * h, r = 6 + R() * 10; ctx.fillStyle = rad(ctx, x, y, r, ["rgba(255,255,255,0.9)", "rgba(200,190,255,0.25)", "rgba(0,0,0,0)"]); ctx.fillRect(x - r, y - r, r * 2, r * 2); }
};

G.planet = (ctx, w, h, R) => {
  ctx.fillStyle = lin(ctx, 0, 0, w, h, ["#05060f", "#0b0a1e", "#140b26"]); ctx.fillRect(0, 0, w, h);
  stars(ctx, w, h, R, w * h / 500);
  const cx = w * 0.68, cy = h * 0.62, r = Math.min(w, h) * 0.42;
  ctx.fillStyle = rad(ctx, cx, cy, r * 1.6, ["rgba(140,120,255,0.35)", "rgba(80,200,220,0.08)", "rgba(0,0,0,0)"]); ctx.fillRect(0, 0, w, h);
  const ring = (front: boolean) => { ctx.save(); ctx.translate(cx, cy); ctx.rotate(-0.32); ctx.beginPath(); ctx.ellipse(0, 0, r * 1.75, r * 0.38, 0, front ? 0 : Math.PI, front ? Math.PI : Math.PI * 2); ctx.lineWidth = r * 0.12; ctx.strokeStyle = lin(ctx, -r * 1.7, 0, r * 1.7, 0, ["rgba(255,200,170,0.1)", "rgba(255,210,190,0.75)", "rgba(190,170,255,0.6)", "rgba(190,170,255,0.08)"]); ctx.stroke(); ctx.lineWidth = r * 0.025; ctx.strokeStyle = "rgba(255,255,255,0.35)"; ctx.beginPath(); ctx.ellipse(0, 0, r * 1.95, r * 0.43, 0, front ? 0 : Math.PI, front ? Math.PI : Math.PI * 2); ctx.stroke(); ctx.restore(); };
  ring(false);
  ctx.fillStyle = rad(ctx, cx - r * 0.45, cy - r * 0.5, r * 1.9, ["#b9a8ff", "#6b58d6", "#2a1d66", "#0c0820"]);
  ctx.beginPath(); ctx.arc(cx, cy, r, 0, 7); ctx.fill();
  ctx.save(); ctx.beginPath(); ctx.arc(cx, cy, r, 0, 7); ctx.clip(); ctx.globalAlpha = 0.18;
  for (let i = 0; i < 9; i++) { ctx.fillStyle = i % 2 ? "#ffd2c2" : "#8fe3d3"; ctx.fillRect(cx - r, cy - r + i * r * 0.23 + R() * 6, r * 2, r * 0.06 + R() * r * 0.05); }
  ctx.restore();
  ring(true);
};

G.lowpoly = (ctx, w, h, R, o) => {
  const pal = o.pal || ["#1a1240", "#4b2f9a", "#b25fc4", "#ff9aa2", "#ffd6a5"];
  const cols = 14, rows = 9, pts: Pt[][] = [];
  for (let j = 0; j <= rows; j++) { pts.push([]); for (let i = 0; i <= cols; i++) { const jx = (i > 0 && i < cols) ? (R() - 0.5) * w / cols * 0.8 : 0, jy = (j > 0 && j < rows) ? (R() - 0.5) * h / rows * 0.8 : 0; pts[j].push([i / cols * w + jx, j / rows * h + jy]); } }
  const tri = (a: Pt, b: Pt, c: Pt) => { const mx = (a[0] + b[0] + c[0]) / 3, my = (a[1] + b[1] + c[1]) / 3; const t = (mx / w) * 0.55 + (1 - my / h) * 0.45 + (R() - 0.5) * 0.12; ctx.fillStyle = ramp(pal, t); ctx.strokeStyle = ctx.fillStyle; ctx.lineWidth = 0.6; ctx.beginPath(); ctx.moveTo(...a); ctx.lineTo(...b); ctx.lineTo(...c); ctx.closePath(); ctx.fill(); ctx.stroke(); };
  for (let j = 0; j < rows; j++) for (let i = 0; i < cols; i++) { const p = pts[j][i], q = pts[j][i + 1], r = pts[j + 1][i], s = pts[j + 1][i + 1]; if ((i + j) % 2) { tri(p, q, s); tri(p, s, r); } else { tri(p, q, r); tri(q, s, r); } }
  ctx.fillStyle = lin(ctx, 0, 0, 0, h, ["rgba(255,255,255,0.08)", "rgba(0,0,0,0.25)"]); ctx.fillRect(0, 0, w, h);
};

G.synthwave = (ctx, w, h, R) => {
  const hz = h * 0.6;
  ctx.fillStyle = lin(ctx, 0, 0, 0, hz, ["#0d0221", "#2a0845", "#7a1b6e", "#ff5e8a"]); ctx.fillRect(0, 0, w, hz);
  stars(ctx, w, hz * 0.6, R, w * h / 1400, 1, 0.8);
  // sun with stripes
  const sr = h * 0.24, sx = w * 0.5, sy = hz - sr * 0.15;
  const off = makeCanvas(); off.width = Math.ceil(sr * 2 * 2); off.height = Math.ceil(sr * 2 * 2); const ox = off.getContext("2d")!; ox.scale(2, 2);
  ox.fillStyle = lin(ox, 0, 0, 0, sr * 2, ["#fff27a", "#ffb35c", "#ff4f9a"]); ox.beginPath(); ox.arc(sr, sr, sr, 0, 7); ox.fill();
  ox.globalCompositeOperation = "destination-out"; for (let i = 0; i < 7; i++) { const y = sr * 1.05 + i * sr * 0.14; ox.fillRect(0, y, sr * 2, 1.5 + i * 1.6); }
  ctx.save(); ctx.shadowColor = "#ff5fa2"; ctx.shadowBlur = 40; ctx.drawImage(off, sx - sr, sy - sr, sr * 2, sr * 2); ctx.restore();
  const m1 = ridge(R, w, hz - h * 0.05, h * 0.16, 0.55); fillRidge(ctx, m1.map(p => [p[0], Math.min(p[1], hz)]), w, hz + 1, "#2b0b47");
  ctx.fillStyle = lin(ctx, 0, hz, 0, h, ["#1a0533", "#0a0118"]); ctx.fillRect(0, hz, w, h - hz);
  ctx.strokeStyle = "rgba(255,80,200,0.85)"; ctx.lineWidth = 1.2; ctx.shadowColor = "#ff4fd8"; ctx.shadowBlur = 8;
  for (let i = 0; i < 14; i++) { const t = i / 13, y = hz + (h - hz) * t * t; ctx.globalAlpha = 0.25 + t * 0.75; ctx.beginPath(); ctx.moveTo(0, y); ctx.lineTo(w, y); ctx.stroke(); }
  for (let i = -16; i <= 16; i++) { ctx.globalAlpha = 0.7; ctx.beginPath(); ctx.moveTo(w / 2 + i * w * 0.012, hz); ctx.lineTo(w / 2 + i * w * 0.14, h); ctx.stroke(); }
  ctx.globalAlpha = 1; ctx.shadowBlur = 0;
  ctx.fillStyle = lin(ctx, 0, hz - 6, 0, hz + 16, ["rgba(255,120,180,0)", "rgba(255,140,200,0.55)", "rgba(255,120,180,0)"]); ctx.fillRect(0, hz - 6, w, 22);
};

G.mountains = (ctx, w, h, R) => {
  ctx.fillStyle = lin(ctx, 0, 0, 0, h, ["#93a9f5", "#c9b5ec", "#f6c3d4", "#ffe0c6"]); ctx.fillRect(0, 0, w, h);
  const sx = w * 0.63, sy = h * 0.46; ctx.fillStyle = rad(ctx, sx, sy, h * 0.5, ["rgba(255,250,235,0.95)", "rgba(255,230,210,0.45)", "rgba(255,220,200,0)"]); ctx.fillRect(0, 0, w, h);
  ctx.fillStyle = "rgba(255,252,240,0.95)"; ctx.beginPath(); ctx.arc(sx, sy, h * 0.07, 0, 7); ctx.fill();
  const layers = ["#d9b8d6", "#b99ac9", "#8f78b3", "#5f4f8c", "#342a57"];
  layers.forEach((c, i) => {
    const pts = ridge(R, w, h * (0.5 + i * 0.1), h * (0.28 - i * 0.03), 0.52);
    fillRidge(ctx, pts, w, h, c);
    ctx.fillStyle = lin(ctx, 0, h * (0.55 + i * 0.1), 0, h, ["rgba(255,236,240,0.32)", "rgba(255,236,240,0)"]); ctx.fillRect(0, h * (0.55 + i * 0.1), w, h);
  });
  ctx.strokeStyle = "rgba(60,40,90,0.6)"; ctx.lineWidth = 1.2; for (let i = 0; i < 3; i++) { const bx = w * (0.2 + R() * 0.3), by = h * (0.22 + R() * 0.1), s = 4 + R() * 3; ctx.beginPath(); ctx.moveTo(bx - s, by); ctx.quadraticCurveTo(bx - s / 2, by - s / 2, bx, by); ctx.quadraticCurveTo(bx + s / 2, by - s / 2, bx + s, by); ctx.stroke(); }
};

G.sakura = (ctx, w, h, R) => {
  ctx.fillStyle = lin(ctx, 0, 0, 0, h, ["#7cc6ff", "#b4dcff", "#ffd9ea"]); ctx.fillRect(0, 0, w, h);
  ctx.fillStyle = rad(ctx, w * 0.78, h * 0.25, h * 0.45, ["rgba(255,255,240,0.95)", "rgba(255,245,220,0.35)", "rgba(255,255,255,0)"]); ctx.fillRect(0, 0, w, h);
  const cloud = (x: number, y: number, s: number) => { ctx.fillStyle = "rgba(255,255,255,0.95)"; for (let i = 0; i < 7; i++) { ctx.beginPath(); ctx.arc(x + (i - 3) * s * 0.55, y - Math.sin(i / 6 * Math.PI) * s * 0.55, s * (0.45 + Math.sin(i / 6 * Math.PI) * 0.3), 0, 7); ctx.fill(); } ctx.fillRect(x - s * 1.8, y - s * 0.05, s * 3.6, s * 0.4); };
  cloud(w * 0.25, h * 0.32, h * 0.09); cloud(w * 0.62, h * 0.48, h * 0.065); cloud(w * 0.9, h * 0.6, h * 0.05);
  // distant hills + town
  fillRidge(ctx, ridge(R, w, h * 0.72, h * 0.06, 0.5), w, h, "#9fd8c9");
  ctx.fillStyle = "#6fbfa6"; ctx.beginPath(); ctx.ellipse(w * 0.62, h * 1.05, w * 0.75, h * 0.28, 0, 0, 7); ctx.fill();
  ctx.fillStyle = "#58a98f"; ctx.beginPath(); ctx.ellipse(w * 0.05, h * 1.08, w * 0.5, h * 0.22, 0, 0, 7); ctx.fill();
  // tree
  ctx.strokeStyle = "#4a3040"; ctx.lineCap = "round"; ctx.lineWidth = w * 0.02; ctx.beginPath(); ctx.moveTo(w * 0.14, h); ctx.quadraticCurveTo(w * 0.16, h * 0.6, w * 0.24, h * 0.32); ctx.stroke();
  ctx.lineWidth = w * 0.008; ctx.beginPath(); ctx.moveTo(w * 0.18, h * 0.55); ctx.quadraticCurveTo(w * 0.28, h * 0.45, w * 0.36, h * 0.38); ctx.stroke();
  const pinks = ["#ffb3cf", "#ff9cc0", "#ffd0e2", "#ffc2d8", "#f98bb4"];
  for (let i = 0; i < 160; i++) { const a = R() * 6.28, d = Math.sqrt(R()); const x = w * 0.24 + Math.cos(a) * d * w * 0.17, y = h * 0.28 + Math.sin(a) * d * h * 0.2; ctx.fillStyle = pinks[i % 5]; ctx.globalAlpha = 0.85; ctx.beginPath(); ctx.arc(x, y, h * (0.018 + R() * 0.03), 0, 7); ctx.fill(); }
  ctx.globalAlpha = 1;
  for (let i = 0; i < 40; i++) { ctx.save(); ctx.translate(w * (0.25 + R() * 0.75), R() * h); ctx.rotate(R() * 6); ctx.fillStyle = pinks[i % 5]; ctx.beginPath(); ctx.ellipse(0, 0, h * 0.009, h * 0.005, 0, 0, 7); ctx.fill(); ctx.restore(); }
};

G.pixelcity = (ctx, w, h, R, o, dpr) => {
  const p = Math.max(2.5, w / 150); const W = Math.ceil(w / p), H = Math.ceil(h / p);
  const c = makeCanvas(); c.width = W; c.height = H; const x = c.getContext("2d")!;
  const sky = ["#140b33", "#26145a", "#4a1f7a", "#8a3a8f", "#e0607e", "#ffa36c"];
  for (let j = 0; j < H; j++) { x.fillStyle = sky[Math.min(5, Math.floor(Math.pow(j / H, 1.4) * 6.4))]; x.fillRect(0, j, W, 1); }
  for (let i = 0; i < W * H / 60; i++) { x.fillStyle = R() > 0.7 ? "#ffffff" : "#bfb3ff"; x.fillRect(Math.floor(R() * W), Math.floor(R() * H * 0.45), 1, 1); }
  x.fillStyle = "#fff3d6"; const mx = Math.floor(W * 0.78), my = Math.floor(H * 0.22), mr = Math.max(3, Math.floor(H * 0.08)); for (let j = -mr; j <= mr; j++) for (let i = -mr; i <= mr; i++) if (i * i + j * j <= mr * mr) x.fillRect(mx + i, my + j, 1, 1);
  const layer = (col: string, win: string[], hmin: number, hmax: number, wmin: number, wmax: number) => { let px = 0; while (px < W) { const bw = Math.floor(wmin + R() * (wmax - wmin)), bh = Math.floor(H * (hmin + R() * (hmax - hmin))); x.fillStyle = col; x.fillRect(px, H - bh, bw, bh); for (let j = H - bh + 2; j < H - 1; j += 2) for (let i = px + 1; i < px + bw - 1; i += 2) if (R() < 0.3) { x.fillStyle = win[Math.floor(R() * win.length)]; x.fillRect(i, j, 1, 1); } px += bw + (R() < 0.3 ? 1 : 0); } };
  layer("#2a1650", ["#6b4fb0", "#8c66d0"], 0.25, 0.5, 4, 9);
  layer("#120a26", ["#ffd36b", "#ffb347", "#7ef0ff", "#ff7ab8"], 0.15, 0.38, 3, 8);
  ctx.imageSmoothingEnabled = false; ctx.drawImage(c, 0, 0, W * p, H * p);
};

G.cyber = (ctx, w, h, R) => {
  ctx.fillStyle = lin(ctx, 0, 0, 0, h, ["#04030b", "#0d0820", "#2a0b3a"]); ctx.fillRect(0, 0, w, h);
  const hz = h * 0.74;
  ctx.fillStyle = rad(ctx, w * 0.5, hz, w * 0.6, ["rgba(255,60,170,0.55)", "rgba(80,40,200,0.2)", "rgba(0,0,0,0)"]); ctx.fillRect(0, 0, w, h);
  let x = -10; while (x < w) { const bw = w * (0.03 + R() * 0.05), bh = h * (0.25 + R() * 0.35); ctx.fillStyle = "#120d2a"; ctx.fillRect(x, hz - bh, bw, bh); for (let i = 0; i < bh / 6; i++) if (R() < 0.35) { ctx.fillStyle = R() < 0.5 ? "rgba(90,220,255,0.5)" : "rgba(255,120,220,0.4)"; ctx.fillRect(x + 2 + R() * (bw - 6), hz - bh + 4 + i * 6, 2, 1.5); } x += bw + 2; }
  x = -20; while (x < w) { const bw = w * (0.06 + R() * 0.08), bh = h * (0.45 + R() * 0.45); ctx.fillStyle = "#07051a"; ctx.fillRect(x, hz - bh, bw, bh);
    if (R() < 0.85) { const c = R() < 0.5 ? "#36e6ff" : "#ff4fd8"; ctx.save(); ctx.shadowColor = c; ctx.shadowBlur = 14; ctx.strokeStyle = c; ctx.lineWidth = 1.6; const sx = x + bw * 0.25, sy = hz - bh * (0.4 + R() * 0.4); ctx.strokeRect(sx, sy, bw * 0.18, bh * 0.22); ctx.fillStyle = c; for (let k = 0; k < 4; k++) ctx.fillRect(sx + bw * 0.05, sy + 6 + k * bh * 0.045, bw * 0.08, 2); ctx.restore(); }
    x += bw + w * 0.02; }
  ctx.fillStyle = "#05040c"; ctx.fillRect(0, hz, w, h - hz);
  ctx.save(); ctx.globalAlpha = 0.35; ctx.translate(0, hz * 2); ctx.scale(1, -1); ctx.filter = `blur(${Math.max(1, w / 400)}px)`; ctx.drawImage(ctx.canvas, 0, 0, ctx.canvas.width, hz * (ctx.canvas.width / w), 0, 0, w, hz); ctx.restore(); ctx.filter = "none";
  ctx.strokeStyle = "rgba(180,200,255,0.18)"; ctx.lineWidth = 0.8; for (let i = 0; i < w * h / 1800; i++) { const rx = R() * w, ry = R() * h, l = 6 + R() * 14; ctx.beginPath(); ctx.moveTo(rx, ry); ctx.lineTo(rx - l * 0.25, ry + l); ctx.stroke(); }
};

G.bars = (ctx, w, h, R) => {
  ctx.fillStyle = rad(ctx, w * 0.5, h * 0.7, w * 0.7, ["#1d1438", "#0d0a1c", "#07060f"]); ctx.fillRect(0, 0, w, h);
  const n = 56, gap = w * 0.004, bw = (w * 0.8 - gap * (n - 1)) / n, base = h * 0.66, x0 = w * 0.1;
  for (let i = 0; i < n; i++) {
    const t = i / (n - 1); const v = Math.pow(Math.sin(t * Math.PI), 0.7) * (0.35 + 0.65 * fbm(i * 0.22, 3, 7)) * (1.1 - t * 0.35);
    const bh = Math.max(4, v * h * 0.5); const x = x0 + i * (bw + gap);
    const g = ctx.createLinearGradient(0, base - bh, 0, base); g.addColorStop(0, "#ffb7a8"); g.addColorStop(0.5, "#b8a2ff"); g.addColorStop(1, "#7fe0d0");
    ctx.fillStyle = g; ctx.beginPath(); ctx.roundRect(x, base - bh, bw, bh, [bw / 2, bw / 2, 0, 0]); ctx.fill();
    ctx.globalAlpha = 0.16; ctx.beginPath(); ctx.roundRect(x, base + 3, bw, bh * 0.45, [0, 0, bw / 2, bw / 2]); ctx.fill(); ctx.globalAlpha = 1;
  }
  ctx.fillStyle = lin(ctx, 0, base, 0, h, ["rgba(7,6,15,0)", "rgba(7,6,15,0.9)"]); ctx.fillRect(0, base, w, h - base);
  ctx.fillStyle = "rgba(255,255,255,0.5)"; ctx.fillRect(x0, base + 1, w * 0.8, 1);
};

G.waves = (ctx, w, h, R, o) => {
  const light = o.variant === "light";
  ctx.fillStyle = light ? lin(ctx, 0, 0, w, h, ["#f7f3ee", "#efe9f6"]) : lin(ctx, 0, 0, w, h, ["#101018", "#17141f"]); ctx.fillRect(0, 0, w, h);
  const n = 34; const f1 = (1 + R()) / w * 6.28, f2 = (2 + R() * 2) / w * 6.28, ph = R() * 6;
  for (let i = 0; i < n; i++) {
    const t = i / (n - 1); ctx.beginPath();
    for (let x = 0; x <= w; x += 3) { const y = h * (0.15 + t * 0.7) + Math.sin(x * f1 + ph + t * 2.2) * h * 0.08 * (1 - Math.abs(t - 0.5)) + Math.sin(x * f2 + t * 5) * h * 0.02; x ? ctx.lineTo(x, y) : ctx.moveTo(x, y); }
    const accent = i === 17;
    ctx.strokeStyle = accent ? (light ? "#ff8a73" : "#ffb7a8") : (light ? `rgba(40,30,60,${0.1 + t * 0.25})` : `rgba(255,255,255,${0.06 + (1 - Math.abs(t - 0.5) * 2) * 0.35})`);
    ctx.lineWidth = accent ? 1.8 : 1; ctx.stroke();
  }
};

G.mesh = (ctx, w, h, R, o) => {
  const pal = o.pal || ["#ffd1c2", "#d6c4ff", "#a8ecdf", "#ffe7a8", "#f7b8d9"];
  soft(ctx, w, h, Math.min(w, h) * 0.12, (c) => {
    c.fillStyle = "#f3ecfb"; c.fillRect(0, 0, w, h);
    for (let i = 0; i < 7; i++) { c.fillStyle = pal[i % pal.length]; c.beginPath(); c.ellipse(R() * w, R() * h, w * (0.18 + R() * 0.22), h * (0.25 + R() * 0.3), R() * 3, 0, 7); c.fill(); }
  });
  grain(ctx, w, h, R, 0.07);
};

G.plasma = (ctx, w, h, R, o, dpr) => {
  const s = R() * 10, k = 0.7 + R() * 0.6;
  pixelField(ctx, w, h, dpr, 3, (u, v) => {
    const x = u * 6 * k, y = v * 6 * k * h / w;
    let t = Math.sin(x * 1.3 + s) + Math.sin(y * 1.7 - s) + Math.sin((x + y) * 0.9 + s * 0.5) + Math.sin(Math.hypot(x - 3, y - 2) * 1.6);
    t = t * 0.125 + 0.5;
    const c = [0.5 + 0.5 * Math.cos(6.283 * (t + 0.0)), 0.5 + 0.5 * Math.cos(6.283 * (t + 0.33)), 0.5 + 0.5 * Math.cos(6.283 * (t + 0.67))];
    const d = 0.55 + 0.45 * Math.pow(Math.abs(Math.sin(t * 18)), 0.4);
    return [Math.round((c[0] * 0.6 + 0.25) * 255 * d), Math.round((c[1] * 0.45 + 0.15) * 255 * d), Math.round((c[2] * 0.6 + 0.35) * 255 * d)];
  });
};

G.topo = (ctx, w, h, R, o, dpr) => {
  const s = Math.floor(R() * 1000), L = 11;
  pixelField(ctx, w, h, dpr, 1, (u, v, W, H) => {
    const N = (uu: number, vv: number) => fbm(uu * 2.4, vv * 2.4 * h / w, s, 3) * L;
    const n = N(u, v), gx = N(u + 1 / W, v) - n, gy = N(u, v + 1 / H) - n;
    const f = n % 1, dd = Math.min(f, 1 - f) / Math.max(1e-4, Math.hypot(gx, gy));
    const a = Math.max(0, Math.min(1, 1.3 - dd)) * (0.35 + (n / L) * 0.75);
    const bg = [14, 16, 24], t = n / L; const lc = [Math.round(110 + t * 150), Math.round(150 + (1 - t) * 90), 235];
    return [bg[0] + (lc[0] - bg[0]) * a, bg[1] + (lc[1] - bg[1]) * a, bg[2] + (lc[2] - bg[2]) * a];
  });
};

G.particles = (ctx, w, h, R) => {
  ctx.fillStyle = lin(ctx, 0, 0, w, h, ["#0a0f1f", "#111735", "#1a1030"]); ctx.fillRect(0, 0, w, h);
  const pts: Pt[] = []; for (let i = 0; i < 70; i++) pts.push([R() * w, R() * h]);
  const cur: Pt = [w * 0.62, h * 0.45]; pts.push(cur);
  const md = Math.min(w, h) * 0.28;
  for (let i = 0; i < pts.length; i++) for (let j = i + 1; j < pts.length; j++) { const d = Math.hypot(pts[i][0] - pts[j][0], pts[i][1] - pts[j][1]); if (d < md) { const near = (pts[i] === cur || pts[j] === cur); ctx.strokeStyle = near ? `rgba(143,227,211,${0.9 - d / md * 0.8})` : `rgba(165,150,255,${0.45 - d / md * 0.45})`; ctx.lineWidth = near ? 1.2 : 0.8; ctx.beginPath(); ctx.moveTo(...pts[i]); ctx.lineTo(...pts[j]); ctx.stroke(); } }
  pts.forEach(p => { ctx.fillStyle = "rgba(220,215,255,0.9)"; ctx.beginPath(); ctx.arc(p[0], p[1], 1.6, 0, 7); ctx.fill(); });
  ctx.strokeStyle = "rgba(143,227,211,0.9)"; ctx.lineWidth = 1.5; ctx.beginPath(); ctx.arc(cur[0], cur[1], 9, 0, 7); ctx.stroke();
  ctx.fillStyle = rad(ctx, cur[0], cur[1], md * 0.6, ["rgba(143,227,211,0.18)", "rgba(0,0,0,0)"]); ctx.fillRect(0, 0, w, h);
};

G.ocean = (ctx, w, h, R) => {
  const hz = h * 0.58;
  ctx.fillStyle = lin(ctx, 0, 0, 0, hz, ["#0a1430", "#24376b", "#7d6aa3", "#e3a7b8"]); ctx.fillRect(0, 0, w, hz);
  stars(ctx, w, hz, R, w * h / 1000, 0.6);
  const mx = w * 0.5, my = hz * 0.42, mr = h * 0.075;
  ctx.fillStyle = rad(ctx, mx, my, mr * 5, ["rgba(255,240,220,0.4)", "rgba(255,230,220,0)"]); ctx.fillRect(0, 0, w, hz);
  ctx.fillStyle = "#fff4e0"; ctx.beginPath(); ctx.arc(mx, my, mr, 0, 7); ctx.fill();
  ctx.fillStyle = lin(ctx, 0, hz, 0, h, ["#3a3d6e", "#141a3d", "#070b1f"]); ctx.fillRect(0, hz, w, h - hz);
  for (let i = 0; i < 260; i++) { const t = R(); const y = hz + Math.pow(t, 1.6) * (h - hz); const spread = w * (0.03 + t * 0.2); const x = mx + (R() - 0.5) * 2 * spread * (R() < 0.75 ? 0.5 : 1.6); const len = w * (0.006 + t * 0.03) * (0.5 + R()); const a = (1 - Math.abs(x - mx) / (spread * 1.6)) * 0.8; if (a <= 0) continue; ctx.fillStyle = `rgba(255,236,215,${a})`; ctx.fillRect(x - len / 2, y, len, Math.max(1, t * 2)); }
  ctx.fillStyle = "rgba(255,220,220,0.25)"; ctx.fillRect(0, hz, w, 1);
};

G.plain = (ctx, w, h) => {
  ctx.fillStyle = lin(ctx, 0, 0, w, h, ["#3d5a80", "#4f6d93", "#6b86a8"]); ctx.fillRect(0, 0, w, h);
  ctx.fillStyle = "rgba(255,255,255,0.05)"; ctx.beginPath(); ctx.ellipse(w * 0.7, h * 1.1, w * 0.6, h * 0.5, 0, 0, 7); ctx.fill();
};

export const ART_KINDS = Object.keys(G);

/** Draws one generator into ctx, which must already be scaled to CSS pixels. */
export function drawArt(ctx: Ctx, kind: string, seed: number, w: number, h: number, dpr: number, opts: ArtOpts = {}): void {
  (G[kind] ?? G.mesh)(ctx, w, h, rng(seed), opts, dpr);
}
