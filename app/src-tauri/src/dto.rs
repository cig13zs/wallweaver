//! Data shapes exchanged with the UI. They mirror `ui/src/lib/daemon/types.ts`
//! field for field (camelCase on the wire) and reject unknown fields, so drift
//! between the two sides fails loudly in tests instead of silently in the UI.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

pub type Values = BTreeMap<String, serde_json::Value>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Cost {
    Low,
    Med,
    High,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WallpaperKind {
    Shader,
    Web,
    Video,
    Image,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Capability {
    Audio,
    Mouse,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ArtSpec {
    pub kind: String,
    pub seed: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variant: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pal: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WallpaperSummary {
    pub id: String,
    pub title: String,
    pub author: String,
    pub kind: WallpaperKind,
    pub category: String,
    pub cost: Cost,
    pub gpu_pct: f64,
    pub uses: Vec<Capability>,
    pub likes: u64,
    pub installed: bool,
    pub art: ArtSpec,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Creator {
    pub handle: String,
    pub initials: String,
    pub wallpapers: u32,
    pub installs: u64,
    pub since: u32,
    pub verified: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SliderUnit {
    #[serde(rename = "x")]
    Times,
    #[serde(rename = "%")]
    Percent,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ComboOption {
    pub value: String,
    pub label: String,
}

/// One user-adjustable setting declared in a wallpaper's manifest.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "lowercase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum Property {
    Slider {
        key: String,
        label: String,
        min: f64,
        max: f64,
        step: f64,
        value: f64,
        unit: SliderUnit,
    },
    Color {
        key: String,
        label: String,
        value: String,
    },
    Bool {
        key: String,
        label: String,
        value: bool,
        icon: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        needs: Option<String>,
    },
    Combo {
        key: String,
        label: String,
        value: String,
        options: Vec<ComboOption>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Palette {
    pub id: String,
    pub css: String,
    pub colors: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WallpaperDetail {
    pub id: String,
    pub title: String,
    pub author: String,
    pub kind: WallpaperKind,
    pub category: String,
    pub cost: Cost,
    pub gpu_pct: f64,
    pub uses: Vec<Capability>,
    pub likes: u64,
    pub installed: bool,
    pub art: ArtSpec,
    pub description: String,
    pub version: String,
    pub license: String,
    pub remix_allowed: bool,
    pub signed: bool,
    pub size_bytes: u64,
    pub updated: String,
    pub watts: f64,
    pub uses_label: String,
    pub presets: u32,
    pub creator: Creator,
    pub palettes: Vec<Palette>,
    pub palette_id: String,
    pub properties: Vec<Property>,
    pub more_from_creator: Vec<WallpaperSummary>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HeroSlide {
    pub id: String,
    pub blurb: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SideFeature {
    pub id: String,
    pub badge: String,
    pub art: ArtSpec,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Featured {
    pub hero: Vec<HeroSlide>,
    pub side: Vec<SideFeature>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Display {
    pub id: u32,
    pub label: String,
    pub width: u32,
    pub height: u32,
    pub main: bool,
    pub wallpaper_id: String,
    pub art: ArtSpec,
    pub layout: Rect,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NowPlaying {
    pub id: String,
    pub title: String,
    pub art: ArtSpec,
    pub displays: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DaemonStatus {
    pub version: String,
    pub displays_live: u32,
    pub displays_total: u32,
    pub cpu_pct: f64,
    pub paused: bool,
    pub library_count: u32,
    pub playlist_count: u32,
    pub now_playing: Option<NowPlaying>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GallerySort {
    #[default]
    Trending,
    New,
    Likes,
    Lightest,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GalleryQuery {
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub low_impact_only: Option<bool>,
    #[serde(default)]
    pub sort: Option<GallerySort>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SpanMode {
    Each,
    Span,
    Same,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ApplyRequest {
    pub wallpaper_id: String,
    pub displays: Vec<u32>,
    pub mode: SpanMode,
    pub values: Values,
    pub palette_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ApplyResult {
    pub applied: Vec<u32>,
    pub estimated_gpu_pct: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PresetRequest {
    pub wallpaper_id: String,
    pub name: String,
    pub values: Values,
    pub palette_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum PermissionKey {
    #[serde(rename = "audio")]
    Audio,
    #[serde(rename = "network")]
    Network,
    #[serde(rename = "system.stats")]
    SystemStats,
    #[serde(rename = "input.mouse")]
    InputMouse,
    #[serde(rename = "weather")]
    Weather,
    #[serde(rename = "calendar")]
    Calendar,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PermissionRequest {
    pub key: PermissionKey,
    pub icon: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    pub detail: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub if_denied: Option<String>,
    pub tech: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Signature {
    pub signer: String,
    pub fingerprint: String,
    pub unchanged: bool,
    pub listed: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BlockedItem {
    pub icon: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InstallRequest {
    pub wallpaper_id: String,
    pub title: String,
    pub author: String,
    pub version: String,
    pub kind_label: String,
    pub size_bytes: u64,
    pub art: ArtSpec,
    pub signature: Option<Signature>,
    pub permissions: Vec<PermissionRequest>,
    pub always_blocked: Vec<BlockedItem>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InstallResult {
    pub wallpaper_id: String,
    pub granted: Vec<PermissionKey>,
    pub denied: Vec<PermissionKey>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RunState {
    Running,
    Paused,
    Stopped,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DisplayPerf {
    pub display_id: u32,
    pub wallpaper_title: String,
    pub art: ArtSpec,
    pub detail: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    pub state: RunState,
    pub fps: u32,
    pub gpu_pct: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CpuMeter {
    pub value: f64,
    pub history: Vec<f64>,
    pub system_pct: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GpuMeter {
    pub value: f64,
    pub history: Vec<f64>,
    pub adapter: String,
    pub shaders: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MemoryMeter {
    pub value: f64,
    pub history: Vec<f64>,
    pub budget_mb: u32,
    pub webviews: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PowerMeter {
    pub value: f64,
    pub history: Vec<f64>,
    pub plugged_in: bool,
    pub battery_pct: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PauseRule {
    pub id: String,
    pub icon: String,
    pub title: String,
    pub detail: String,
    pub action: String,
    pub options: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AppRule {
    pub exe: String,
    pub icon: String,
    pub action: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PresetId {
    Quality,
    Balanced,
    Saver,
    Ultra,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PowerPreset {
    pub id: PresetId,
    pub icon: String,
    pub title: String,
    pub detail: String,
    pub recommended: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PerfSettings {
    /// Frames per second; 0 means uncapped.
    pub fps_cap: u32,
    pub render_scale: u32,
    pub hw_decode: bool,
    pub hw_decode_available: bool,
    pub away_minutes: u32,
    pub show_cost_badges: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PerfSnapshot {
    pub cpu: CpuMeter,
    pub gpu: GpuMeter,
    pub memory: MemoryMeter,
    pub power: PowerMeter,
    pub displays: Vec<DisplayPerf>,
    pub check_interval_ms: u32,
    pub rules: Vec<PauseRule>,
    pub app_rules: Vec<AppRule>,
    pub preset: PresetId,
    pub presets: Vec<PowerPreset>,
    pub settings: PerfSettings,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FileNode {
    pub name: String,
    pub icon: String,
    pub depth: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modified: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LintIssue {
    pub file: String,
    pub line: u32,
    pub call: String,
    pub missing: String,
    pub origin: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Signer {
    pub name: String,
    pub fingerprint: String,
    pub store: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreatorProject {
    pub name: String,
    pub version: String,
    pub hot_reload: bool,
    pub saved_seconds_ago: u32,
    pub files: Vec<FileNode>,
    pub file_count: u32,
    pub total_bytes: u64,
    pub limit_bytes: u64,
    pub art: ArtSpec,
    pub properties: Vec<Property>,
    pub declared: Vec<String>,
    pub issues: Vec<LintIssue>,
    pub signer: Signer,
    pub release_note: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PackResult {
    pub file: String,
    pub size_bytes: u64,
    pub signed_by: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CreatorAction {
    OpenEditor,
    TryOnDesktop,
    SubmitToGallery,
    NewFile,
    AddProperty,
    AddFiles,
    TemplateVideo,
    TemplateWeb,
    TemplateShader,
    TemplateVisualizer,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Funding {
    pub month: String,
    pub raised: u32,
    pub target: u32,
    pub supporters: u32,
    pub covers: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemePref {
    System,
    Dark,
    Light,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MotionPref {
    System,
    Reduce,
    Full,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Settings {
    pub theme: ThemePref,
    pub motion: MotionPref,
    pub start_with_system: bool,
    pub tray_icon: bool,
    pub pause_hotkey: String,
    pub check_gallery_updates: bool,
    pub developer_mode: bool,
    pub gallery_sources: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LinkId {
    Source,
    Funding,
    Sponsor,
    Tip,
    Store,
    Report,
}

/// Everything the stub daemon serves, loaded from the fixture file the browser mock also uses.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Fixtures {
    pub status: DaemonStatus,
    pub featured: Featured,
    pub gallery: Vec<WallpaperSummary>,
    pub details: Vec<WallpaperDetail>,
    pub displays: Vec<Display>,
    pub install_requests: Vec<InstallRequest>,
    pub performance: PerfSnapshot,
    pub creator: CreatorProject,
    pub settings: Settings,
    pub funding: Option<Funding>,
}
