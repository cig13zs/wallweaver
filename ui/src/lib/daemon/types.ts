// Data shapes the manager UI exchanges with the wallpaper daemon.
// The Rust side mirrors these in app/src-tauri/src/dto.rs (serde, camelCase).

export type Cost = "low" | "med" | "high";
export type WallpaperKind = "shader" | "web" | "video" | "image";
export type Capability = "audio" | "mouse";

export interface ArtSpec {
  kind: string;
  seed: number;
  variant?: string;
  pal?: string[];
}

export interface WallpaperSummary {
  id: string;
  title: string;
  author: string;
  kind: WallpaperKind;
  category: string;
  cost: Cost;
  gpuPct: number;
  uses: Capability[];
  likes: number;
  installed: boolean;
  art: ArtSpec;
}

export interface Creator {
  handle: string;
  initials: string;
  wallpapers: number;
  installs: number;
  since: number;
  verified: boolean;
}

export type Property =
  | { key: string; label: string; type: "slider"; min: number; max: number; step: number; value: number; unit: "x" | "%" }
  | { key: string; label: string; type: "color"; value: string }
  | { key: string; label: string; type: "bool"; value: boolean; icon: string; needs?: string }
  | { key: string; label: string; type: "combo"; value: string; options: { value: string; label: string }[] };

export interface Palette {
  id: string;
  css: string;
  colors: string[];
}

export interface WallpaperDetail extends WallpaperSummary {
  description: string;
  version: string;
  license: string;
  remixAllowed: boolean;
  signed: boolean;
  sizeBytes: number;
  updated: string;
  watts: number;
  usesLabel: string;
  presets: number;
  creator: Creator;
  palettes: Palette[];
  paletteId: string;
  properties: Property[];
  moreFromCreator: WallpaperSummary[];
}

export interface Featured {
  hero: { id: string; blurb: string }[];
  side: { id: string; badge: string; art: ArtSpec }[];
}

export interface Display {
  id: number;
  label: string;
  width: number;
  height: number;
  main: boolean;
  wallpaperId: string;
  /** Artwork of what is showing on this display now. */
  art: ArtSpec;
  /** Position in the arrangement diagram, in diagram pixels. */
  layout: { x: number; y: number; w: number; h: number };
}

export interface NowPlaying {
  id: string;
  title: string;
  art: ArtSpec;
  displays: number;
}

export interface DaemonStatus {
  version: string;
  displaysLive: number;
  displaysTotal: number;
  cpuPct: number;
  paused: boolean;
  libraryCount: number;
  playlistCount: number;
  nowPlaying: NowPlaying | null;
}

export type GallerySort = "trending" | "new" | "likes" | "lightest";

export interface GalleryQuery {
  text?: string;
  category?: string;
  lowImpactOnly?: boolean;
  sort?: GallerySort;
}

export type SpanMode = "each" | "span" | "same";

export interface ApplyRequest {
  wallpaperId: string;
  displays: number[];
  mode: SpanMode;
  values: Record<string, number | string | boolean>;
  paletteId: string;
}

export interface ApplyResult {
  applied: number[];
  estimatedGpuPct: number;
}

export type PermissionKey = "audio" | "network" | "system.stats" | "input.mouse" | "weather" | "calendar";

export interface PermissionRequest {
  key: PermissionKey;
  icon: string;
  title: string;
  /** Optional emphasised part of the title, such as a network origin. */
  target?: string;
  detail: string;
  ifDenied?: string;
  tech: string;
}

export interface InstallRequest {
  wallpaperId: string;
  title: string;
  author: string;
  version: string;
  kindLabel: string;
  sizeBytes: number;
  art: ArtSpec;
  signature: { signer: string; fingerprint: string; unchanged: boolean; listed: boolean } | null;
  permissions: PermissionRequest[];
  alwaysBlocked: { icon: string; label: string }[];
}

export interface InstallResult {
  wallpaperId: string;
  granted: PermissionKey[];
  denied: PermissionKey[];
}

export type RunState = "running" | "paused" | "stopped";

export interface DisplayPerf {
  displayId: number;
  wallpaperTitle: string;
  art: ArtSpec;
  detail: string;
  reason?: string;
  state: RunState;
  fps: number;
  gpuPct: number;
}

export interface Meter {
  value: number;
  history: number[];
}

export interface PauseRule {
  id: string;
  icon: string;
  title: string;
  detail: string;
  action: string;
  options: string[];
}

export interface AppRule {
  exe: string;
  icon: string;
  action: string;
}

export type PresetId = "quality" | "balanced" | "saver" | "ultra";

export interface PowerPreset {
  id: PresetId;
  icon: string;
  title: string;
  detail: string;
  recommended: boolean;
}

export interface PerfSettings {
  fpsCap: number;
  renderScale: number;
  hwDecode: boolean;
  hwDecodeAvailable: boolean;
  awayMinutes: number;
  showCostBadges: boolean;
}

export interface PerfSnapshot {
  cpu: Meter & { systemPct: number };
  gpu: Meter & { adapter: string; shaders: number };
  memory: Meter & { budgetMb: number; webviews: number };
  power: Meter & { pluggedIn: boolean; batteryPct: number };
  displays: DisplayPerf[];
  checkIntervalMs: number;
  rules: PauseRule[];
  appRules: AppRule[];
  preset: PresetId;
  presets: PowerPreset[];
  settings: PerfSettings;
}

export interface FileNode {
  name: string;
  icon: string;
  depth: number;
  sizeBytes?: number;
  modified?: boolean;
}

export type StepState = "done" | "warn" | "current" | "todo";

export interface LintIssue {
  file: string;
  line: number;
  call: string;
  missing: string;
  origin: string;
}

export interface CreatorProject {
  name: string;
  version: string;
  hotReload: boolean;
  savedSecondsAgo: number;
  files: FileNode[];
  fileCount: number;
  totalBytes: number;
  limitBytes: number;
  art: ArtSpec;
  properties: Property[];
  declared: string[];
  issues: LintIssue[];
  signer: { name: string; fingerprint: string; store: string };
  releaseNote: string;
}

export interface PackResult {
  file: string;
  sizeBytes: number;
  signedBy: string;
}

export type ThemePref = "system" | "dark" | "light";
export type MotionPref = "system" | "reduce" | "full";

export interface Settings {
  theme: ThemePref;
  motion: MotionPref;
  startWithSystem: boolean;
  trayIcon: boolean;
  pauseHotkey: string;
  checkGalleryUpdates: boolean;
  developerMode: boolean;
  gallerySources: string[];
}

export type CreatorAction =
  | "openEditor" | "tryOnDesktop" | "submitToGallery" | "newFile" | "addProperty" | "addFiles"
  | "templateVideo" | "templateWeb" | "templateShader" | "templateVisualizer";

export interface PresetRequest {
  wallpaperId: string;
  name: string;
  values: Record<string, number | string | boolean>;
  paletteId: string;
}

export interface Funding {
  month: string;
  raised: number;
  target: number;
  supporters: number;
  covers: string[];
}

export type LinkId = "source" | "funding" | "sponsor" | "tip" | "store" | "report";

export interface Fixtures {
  status: DaemonStatus;
  featured: Featured;
  gallery: WallpaperSummary[];
  details: WallpaperDetail[];
  displays: Display[];
  installRequests: InstallRequest[];
  performance: PerfSnapshot;
  creator: CreatorProject;
  settings: Settings;
  funding: Funding | null;
}

/** Everything the UI asks of the daemon. MockDaemon serves fixtures; TauriDaemon goes over IPC. */
export interface Daemon {
  readonly kind: "mock" | "tauri";
  status(): Promise<DaemonStatus>;
  featured(): Promise<Featured>;
  gallery(query: GalleryQuery): Promise<WallpaperSummary[]>;
  wallpaper(id: string): Promise<WallpaperDetail>;
  displays(): Promise<Display[]>;
  apply(req: ApplyRequest): Promise<ApplyResult>;
  /** Saves the current customization as a named preset; returns how many presets the wallpaper now has. */
  savePreset(req: PresetRequest): Promise<number>;
  installRequest(id: string): Promise<InstallRequest>;
  install(id: string, granted: PermissionKey[]): Promise<InstallResult>;
  performance(): Promise<PerfSnapshot>;
  setPreset(id: PresetId): Promise<PerfSnapshot>;
  setPauseRule(ruleId: string, action: string): Promise<PerfSnapshot>;
  setPerfSettings(settings: PerfSettings): Promise<PerfSnapshot>;
  setAppRule(exe: string, action: string): Promise<PerfSnapshot>;
  identifyDisplays(): Promise<void>;
  pauseAll(paused: boolean): Promise<DaemonStatus>;
  creatorProject(): Promise<CreatorProject>;
  declareOrigin(origin: string): Promise<CreatorProject>;
  packAndSign(): Promise<PackResult>;
  /** Runs a Creator Studio action and returns a one-line result to show the user. */
  creatorAction(action: CreatorAction): Promise<string>;
  /** Monthly funding goal, if the project publishes one. */
  funding(): Promise<Funding | null>;
  settings(): Promise<Settings>;
  updateSettings(settings: Settings): Promise<Settings>;
  /** Opens a fixed, daemon-side link. subject names the creator or wallpaper it is about, if any. */
  openLink(id: LinkId, subject?: string): Promise<void>;
}
