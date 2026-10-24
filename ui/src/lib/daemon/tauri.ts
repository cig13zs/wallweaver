import { invoke } from "@tauri-apps/api/core";
import type {
  ApplyRequest, ApplyResult, CreatorAction, CreatorProject, Daemon, DaemonStatus, Display, Featured, Funding, GalleryQuery, InstallRequest,
  InstallResult, LinkId, PackResult, PerfSettings, PerfSnapshot, PermissionKey, PresetId, PresetRequest, Settings, WallpaperDetail,
  WallpaperSummary,
} from "./types";

/** Talks to the desktop shell, which forwards each call to the daemon. Command names match app/src-tauri/src/commands.rs. */
export class TauriDaemon implements Daemon {
  readonly kind = "tauri" as const;

  status(): Promise<DaemonStatus> { return invoke("status"); }
  featured(): Promise<Featured> { return invoke("featured"); }
  gallery(query: GalleryQuery): Promise<WallpaperSummary[]> { return invoke("gallery", { query }); }
  wallpaper(id: string): Promise<WallpaperDetail> { return invoke("wallpaper", { id }); }
  displays(): Promise<Display[]> { return invoke("displays"); }
  apply(req: ApplyRequest): Promise<ApplyResult> { return invoke("apply", { req }); }
  savePreset(req: PresetRequest): Promise<number> { return invoke("save_preset", { req }); }
  installRequest(id: string): Promise<InstallRequest> { return invoke("install_request", { id }); }
  install(id: string, granted: PermissionKey[]): Promise<InstallResult> { return invoke("install", { id, granted }); }
  performance(): Promise<PerfSnapshot> { return invoke("performance"); }
  setPreset(id: PresetId): Promise<PerfSnapshot> { return invoke("set_preset", { id }); }
  setPauseRule(ruleId: string, action: string): Promise<PerfSnapshot> { return invoke("set_pause_rule", { ruleId, action }); }
  setPerfSettings(settings: PerfSettings): Promise<PerfSnapshot> { return invoke("set_perf_settings", { settings }); }
  setAppRule(exe: string, action: string): Promise<PerfSnapshot> { return invoke("set_app_rule", { exe, action }); }
  identifyDisplays(): Promise<void> { return invoke("identify_displays"); }
  pauseAll(paused: boolean): Promise<DaemonStatus> { return invoke("pause_all", { paused }); }
  creatorProject(): Promise<CreatorProject> { return invoke("creator_project"); }
  declareOrigin(origin: string): Promise<CreatorProject> { return invoke("declare_origin", { origin }); }
  packAndSign(): Promise<PackResult> { return invoke("pack_and_sign"); }
  creatorAction(action: CreatorAction): Promise<string> { return invoke("creator_action", { action }); }
  funding(): Promise<Funding | null> { return invoke("funding"); }
  settings(): Promise<Settings> { return invoke("settings"); }
  updateSettings(settings: Settings): Promise<Settings> { return invoke("update_settings", { settings }); }
  openLink(id: LinkId, subject?: string): Promise<void> { return invoke("open_link", { id, subject: subject ?? null }); }
}
