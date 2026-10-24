import type {
  ApplyRequest, ApplyResult, CreatorAction, CreatorProject, Daemon, DaemonStatus, Display, Featured, Fixtures, Funding, GalleryQuery,
  InstallRequest, InstallResult, LinkId, PackResult, PerfSettings, PerfSnapshot, PermissionKey, PermissionRequest,
  PresetId, PresetRequest, Settings, WallpaperDetail, WallpaperSummary,
} from "./types";
import fixtureData from "./fixtures.json";
import { COST_RANK } from "../format";

const clone = <T>(v: T): T => structuredClone(v);

const AUDIO: PermissionRequest = {
  key: "audio", icon: "audio", title: "See how loud your music is",
  detail: "Audio levels only, as 64 bars. Never your microphone, never a recording.", tech: "audio",
};
const MOUSE: PermissionRequest = {
  key: "input.mouse", icon: "mouse", title: "Follow your mouse over the desktop",
  detail: "Pointer position only, while the desktop is visible. No clicks, no typing.", tech: "input.mouse",
};

export interface MockOptions {
  /** Jitter live meters on every performance() call. Off gives stable numbers for screenshots. */
  live?: boolean;
}

/** In-memory daemon for browser mode and tests. Serves fixtures and keeps state for the session. */
export class MockDaemon implements Daemon {
  readonly kind = "mock" as const;
  private fx: Fixtures;
  private live: boolean;
  private tick = 0;
  private presets: Record<string, PresetRequest[]> = {};

  constructor(opts: MockOptions = {}, fixtures: Fixtures = fixtureData as Fixtures) {
    this.fx = clone(fixtures);
    this.live = opts.live ?? true;
  }

  async status(): Promise<DaemonStatus> {
    return clone(this.fx.status);
  }

  async featured(): Promise<Featured> {
    return clone(this.fx.featured);
  }

  async gallery(q: GalleryQuery = {}): Promise<WallpaperSummary[]> {
    const text = q.text?.trim().toLowerCase() ?? "";
    let items = this.fx.gallery.filter((w) => {
      if (text && !`${w.title} ${w.author} ${w.category} ${w.kind}`.toLowerCase().includes(text)) return false;
      if (q.lowImpactOnly && w.cost !== "low") return false;
      switch (q.category ?? "all") {
        case "all": return true;
        case "audio": return w.uses.includes("audio");
        case "interactive": return w.uses.includes("mouse");
        default: return w.category === q.category;
      }
    });
    if (q.sort === "likes") items = [...items].sort((a, b) => b.likes - a.likes);
    else if (q.sort === "lightest") items = [...items].sort((a, b) => COST_RANK[a.cost] - COST_RANK[b.cost] || a.gpuPct - b.gpuPct);
    else if (q.sort === "new") items = [...items].reverse();
    return clone(items);
  }

  async wallpaper(id: string): Promise<WallpaperDetail> {
    const full = this.fx.details.find((d) => d.id === id);
    if (full) return clone(full);
    const s = this.summary(id);
    const base = this.fx.details[0]!;
    return clone({
      ...base,
      ...s,
      description: `${s.title} by ${s.author}. A ${s.kind} wallpaper in ${s.category}.`,
      version: "1.0.0",
      usesLabel: s.uses.includes("audio") ? "Audio levels only" : s.uses.includes("mouse") ? "Mouse position only" : "Nothing outside its folder",
      watts: Math.round(s.gpuPct * 2) / 10,
      creator: { handle: s.author, initials: s.author.slice(0, 2).toUpperCase(), wallpapers: 6, installs: s.likes * 2, since: 2026, verified: false },
      moreFromCreator: this.fx.gallery.filter((g) => g.id !== id).slice(0, 5),
    });
  }

  async displays(): Promise<Display[]> {
    return clone(this.fx.displays);
  }

  async apply(req: ApplyRequest): Promise<ApplyResult> {
    const s = this.fx.details.find((d) => d.id === req.wallpaperId) ?? this.summary(req.wallpaperId);
    for (const d of this.fx.displays) if (req.displays.includes(d.id)) { d.wallpaperId = req.wallpaperId; d.art = s.art; }
    this.fx.status.nowPlaying = { id: s.id, title: s.title, art: s.art, displays: req.displays.length };
    return { applied: [...req.displays], estimatedGpuPct: s.gpuPct };
  }

  async savePreset(req: PresetRequest): Promise<number> {
    const name = req.name.trim();
    if (!name) throw new Error("preset name is empty");
    const list = (this.presets[req.wallpaperId] ??= []);
    const existing = list.findIndex((p) => p.name === name);
    if (existing >= 0) list[existing] = clone(req); else list.push(clone(req));
    const detail = this.fx.details.find((d) => d.id === req.wallpaperId);
    const base = detail?.presets ?? 0;
    if (detail) detail.presets = base + (existing >= 0 ? 0 : 1);
    return detail ? detail.presets : list.length;
  }

  async installRequest(id: string): Promise<InstallRequest> {
    const known = this.fx.installRequests.find((r) => r.wallpaperId === id);
    if (known) return clone(known);
    const s = this.summary(id);
    const template = this.fx.installRequests[0]!;
    return clone({
      wallpaperId: s.id, title: s.title, author: s.author, version: "1.0.0",
      kindLabel: `${s.kind[0]!.toUpperCase()}${s.kind.slice(1)} wallpaper`, sizeBytes: 2_400_000, art: s.art,
      signature: { signer: s.author, fingerprint: "3C91…A07D", unchanged: true, listed: true },
      permissions: [...(s.uses.includes("audio") ? [AUDIO] : []), ...(s.uses.includes("mouse") ? [MOUSE] : [])],
      alwaysBlocked: template.alwaysBlocked,
    });
  }

  async install(id: string, granted: PermissionKey[]): Promise<InstallResult> {
    const req = await this.installRequest(id);
    const g = this.fx.gallery.find((w) => w.id === id);
    if (g) g.installed = true;
    this.fx.status.libraryCount += 1;
    const asked = req.permissions.map((p) => p.key);
    return {
      wallpaperId: id,
      granted: asked.filter((k) => granted.includes(k)),
      denied: asked.filter((k) => !granted.includes(k)),
    };
  }

  async performance(): Promise<PerfSnapshot> {
    if (this.live) this.jitter();
    return clone(this.fx.performance);
  }

  async setPreset(id: PresetId): Promise<PerfSnapshot> {
    const p = this.fx.performance;
    p.preset = id;
    p.settings.fpsCap = id === "quality" || id === "balanced" ? 60 : 30;
    return clone(p);
  }

  async setPauseRule(ruleId: string, action: string): Promise<PerfSnapshot> {
    const rule = this.fx.performance.rules.find((r) => r.id === ruleId);
    if (!rule) throw new Error(`unknown pause rule: ${ruleId}`);
    if (!rule.options.includes(action)) throw new Error(`unsupported action for ${ruleId}: ${action}`);
    rule.action = action;
    return clone(this.fx.performance);
  }

  async setPerfSettings(settings: PerfSettings): Promise<PerfSnapshot> {
    this.fx.performance.settings = { ...clone(settings), hwDecodeAvailable: this.fx.performance.settings.hwDecodeAvailable };
    return clone(this.fx.performance);
  }

  async setAppRule(exe: string, action: string): Promise<PerfSnapshot> {
    const name = exe.trim();
    if (!name) throw new Error("app name is empty");
    const rules = this.fx.performance.appRules;
    const existing = rules.find((r) => r.exe.toLowerCase() === name.toLowerCase());
    if (existing) existing.action = action;
    else rules.push({ exe: name, icon: "window", action });
    return clone(this.fx.performance);
  }

  async identifyDisplays(): Promise<void> {
    // The real daemon flashes a number on each display.
  }

  async pauseAll(paused: boolean): Promise<DaemonStatus> {
    const st = this.fx.status;
    st.paused = paused;
    st.displaysLive = paused ? 0 : 2;
    st.cpuPct = paused ? 0 : 1.9;
    for (const d of this.fx.performance.displays) {
      if (paused) { d.state = "paused"; d.fps = 0; d.gpuPct = 0; }
    }
    if (!paused) this.fx.performance.displays = clone((fixtureData as Fixtures).performance.displays);
    return clone(st);
  }

  async creatorProject(): Promise<CreatorProject> {
    return clone(this.fx.creator);
  }

  async declareOrigin(origin: string): Promise<CreatorProject> {
    const c = this.fx.creator;
    if (!c.declared.includes("network")) c.declared.push("network");
    c.issues = c.issues.filter((i) => i.origin !== origin);
    return clone(c);
  }

  async packAndSign(): Promise<PackResult> {
    const c = this.fx.creator;
    return { file: `${c.name}-${c.version}.wwp`, sizeBytes: c.totalBytes, signedBy: c.signer.name };
  }

  async creatorAction(action: CreatorAction): Promise<string> {
    const c = this.fx.creator;
    switch (action) {
      case "openEditor": return `Opened ${c.name} in your editor`;
      case "tryOnDesktop": return "Showing it on your desktop for 30 seconds";
      case "submitToGallery": return "Gallery pull request drafted";
      case "newFile": return "Created untitled.js";
      case "addProperty": return "Added a property to wallpaper.json";
      case "addFiles": return `Copied the files into ${c.name}`;
      default: return `Started a new project from the ${action.replace("template", "").toLowerCase()} template`;
    }
  }

  async funding(): Promise<Funding | null> {
    return clone(this.fx.funding);
  }

  async settings(): Promise<Settings> {
    return clone(this.fx.settings);
  }

  async updateSettings(settings: Settings): Promise<Settings> {
    this.fx.settings = clone(settings);
    return clone(settings);
  }

  async openLink(_id: LinkId, _subject?: string): Promise<void> {
    // Browser mode has no shell to hand links to.
  }

  private summary(id: string): WallpaperSummary {
    const s = this.fx.gallery.find((w) => w.id === id)
      ?? this.fx.details.find((w) => w.id === id)
      ?? this.fx.details.flatMap((d) => d.moreFromCreator).find((w) => w.id === id);
    if (!s) throw new Error(`unknown wallpaper: ${id}`);
    return s;
  }

  private jitter(): void {
    this.tick += 1;
    const p = this.fx.performance;
    const wobble = (base: number, amt: number) => Math.max(0, Math.round((base + Math.sin(this.tick * 1.7) * amt) * 10) / 10);
    for (const m of [p.cpu, p.gpu, p.power]) {
      m.history = [...m.history.slice(1), Math.max(4, Math.min(40, m.history[m.history.length - 1]! + Math.round(Math.sin(this.tick * 2.3) * 3)))];
    }
    p.cpu.value = wobble(1.9, 0.3);
    p.gpu.value = wobble(3.4, 0.4);
    p.power.value = wobble(0.6, 0.1);
    this.fx.status.cpuPct = p.cpu.value;
  }
}
