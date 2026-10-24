//! The manager's view of the wallpaper daemon.
//!
//! The window never renders wallpapers itself: it asks the daemon, which keeps
//! running after the window closes. [`Daemon`] is that contract. [`StubDaemon`]
//! answers from the shared fixture set (the same file browser mode uses) and keeps
//! changes in memory, so the shell runs end to end before the daemon's IPC client
//! is wired in behind the same trait.

use std::sync::Mutex;

use crate::dto::{
    ApplyRequest, ApplyResult, Capability, CreatorAction, CreatorProject, DaemonStatus, Display,
    Featured, Fixtures, Funding, GalleryQuery, GallerySort, InstallRequest, InstallResult,
    NowPlaying, PackResult, PerfSettings, PerfSnapshot, PermissionKey, PermissionRequest, PresetId,
    PresetRequest, RunState, Settings, Signature, WallpaperDetail, WallpaperSummary,
};

/// Fixture data shared with the browser-mode mock in the UI.
pub const FIXTURES_JSON: &str = include_str!("../../../ui/src/lib/daemon/fixtures.json");

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum DaemonError {
    #[error("unknown wallpaper: {0}")]
    UnknownWallpaper(String),
    #[error("unknown pause rule: {0}")]
    UnknownRule(String),
    #[error("unsupported action for {rule}: {action}")]
    UnsupportedAction { rule: String, action: String },
    #[error("{0} is empty")]
    Empty(&'static str),
    #[error("unknown display: {0}")]
    UnknownDisplay(u32),
    #[error("the wallpaper service state is unavailable")]
    Poisoned,
    #[error("fixture data is invalid: {0}")]
    Fixtures(String),
}

pub type Result<T> = std::result::Result<T, DaemonError>;

/// Everything the manager window asks of the daemon.
pub trait Daemon: Send + Sync {
    fn status(&self) -> Result<DaemonStatus>;
    fn featured(&self) -> Result<Featured>;
    fn gallery(&self, query: &GalleryQuery) -> Result<Vec<WallpaperSummary>>;
    fn wallpaper(&self, id: &str) -> Result<WallpaperDetail>;
    fn displays(&self) -> Result<Vec<Display>>;
    fn apply(&self, req: &ApplyRequest) -> Result<ApplyResult>;
    fn save_preset(&self, req: &PresetRequest) -> Result<u32>;
    fn install_request(&self, id: &str) -> Result<InstallRequest>;
    fn install(&self, id: &str, granted: &[PermissionKey]) -> Result<InstallResult>;
    fn performance(&self) -> Result<PerfSnapshot>;
    fn set_preset(&self, id: PresetId) -> Result<PerfSnapshot>;
    fn set_pause_rule(&self, rule_id: &str, action: &str) -> Result<PerfSnapshot>;
    fn set_perf_settings(&self, settings: &PerfSettings) -> Result<PerfSnapshot>;
    fn set_app_rule(&self, exe: &str, action: &str) -> Result<PerfSnapshot>;
    fn identify_displays(&self) -> Result<()>;
    fn pause_all(&self, paused: bool) -> Result<DaemonStatus>;
    fn creator_project(&self) -> Result<CreatorProject>;
    fn declare_origin(&self, origin: &str) -> Result<CreatorProject>;
    fn pack_and_sign(&self) -> Result<PackResult>;
    fn creator_action(&self, action: CreatorAction) -> Result<String>;
    fn funding(&self) -> Result<Option<Funding>>;
    fn settings(&self) -> Result<Settings>;
    fn update_settings(&self, settings: &Settings) -> Result<Settings>;
}

/// In-memory daemon backed by the fixture set.
#[derive(Debug)]
pub struct StubDaemon {
    state: Mutex<State>,
}

#[derive(Debug)]
struct State {
    fx: Fixtures,
    initial_displays: Vec<crate::dto::DisplayPerf>,
    saved_presets: Vec<PresetRequest>,
}

impl StubDaemon {
    pub fn from_fixtures(fx: Fixtures) -> Self {
        let initial_displays = fx.performance.displays.clone();
        Self {
            state: Mutex::new(State {
                fx,
                initial_displays,
                saved_presets: Vec::new(),
            }),
        }
    }

    pub fn from_json(json: &str) -> Result<Self> {
        let fx = serde_json::from_str(json).map_err(|e| DaemonError::Fixtures(e.to_string()))?;
        Ok(Self::from_fixtures(fx))
    }

    pub fn bundled() -> Result<Self> {
        Self::from_json(FIXTURES_JSON)
    }

    fn with<T>(&self, f: impl FnOnce(&mut State) -> Result<T>) -> Result<T> {
        let mut guard = self.state.lock().map_err(|_| DaemonError::Poisoned)?;
        f(&mut guard)
    }
}

impl State {
    fn summary(&self, id: &str) -> Result<WallpaperSummary> {
        if let Some(s) = self.fx.gallery.iter().find(|w| w.id == id) {
            return Ok(s.clone());
        }
        if let Some(d) = self.fx.details.iter().find(|w| w.id == id) {
            return Ok(summary_of(d));
        }
        self.fx
            .details
            .iter()
            .flat_map(|d| d.more_from_creator.iter())
            .find(|w| w.id == id)
            .cloned()
            .ok_or_else(|| DaemonError::UnknownWallpaper(id.to_owned()))
    }
}

fn summary_of(d: &WallpaperDetail) -> WallpaperSummary {
    WallpaperSummary {
        id: d.id.clone(),
        title: d.title.clone(),
        author: d.author.clone(),
        kind: d.kind,
        category: d.category.clone(),
        cost: d.cost,
        gpu_pct: d.gpu_pct,
        uses: d.uses.clone(),
        likes: d.likes,
        installed: d.installed,
        art: d.art.clone(),
    }
}

fn cost_rank(c: crate::dto::Cost) -> u8 {
    match c {
        crate::dto::Cost::Low => 0,
        crate::dto::Cost::Med => 1,
        crate::dto::Cost::High => 2,
    }
}

fn kind_label(kind: crate::dto::WallpaperKind) -> &'static str {
    match kind {
        crate::dto::WallpaperKind::Shader => "Shader wallpaper",
        crate::dto::WallpaperKind::Web => "Web wallpaper",
        crate::dto::WallpaperKind::Video => "Video wallpaper",
        crate::dto::WallpaperKind::Image => "Image wallpaper",
    }
}

fn audio_request() -> PermissionRequest {
    PermissionRequest {
        key: PermissionKey::Audio,
        icon: "audio".into(),
        title: "See how loud your music is".into(),
        target: None,
        detail: "Audio levels only, as 64 bars. Never your microphone, never a recording.".into(),
        if_denied: None,
        tech: "audio".into(),
    }
}

fn mouse_request() -> PermissionRequest {
    PermissionRequest {
        key: PermissionKey::InputMouse,
        icon: "mouse".into(),
        title: "Follow your mouse over the desktop".into(),
        target: None,
        detail: "Pointer position only, while the desktop is visible. No clicks, no typing.".into(),
        if_denied: None,
        tech: "input.mouse".into(),
    }
}

impl Daemon for StubDaemon {
    fn status(&self) -> Result<DaemonStatus> {
        self.with(|s| Ok(s.fx.status.clone()))
    }

    fn featured(&self) -> Result<Featured> {
        self.with(|s| Ok(s.fx.featured.clone()))
    }

    fn gallery(&self, q: &GalleryQuery) -> Result<Vec<WallpaperSummary>> {
        self.with(|s| {
            let text = q.text.as_deref().unwrap_or("").trim().to_lowercase();
            let category = q.category.as_deref().unwrap_or("all");
            let mut items: Vec<WallpaperSummary> = s
                .fx
                .gallery
                .iter()
                .filter(|w| {
                    let hay = format!("{} {} {} {:?}", w.title, w.author, w.category, w.kind)
                        .to_lowercase();
                    (text.is_empty() || hay.contains(&text))
                        && (!q.low_impact_only.unwrap_or(false) || w.cost == crate::dto::Cost::Low)
                        && match category {
                            "all" => true,
                            "audio" => w.uses.contains(&Capability::Audio),
                            "interactive" => w.uses.contains(&Capability::Mouse),
                            other => w.category == other,
                        }
                })
                .cloned()
                .collect();
            match q.sort.unwrap_or_default() {
                GallerySort::Trending => {}
                GallerySort::New => items.reverse(),
                GallerySort::Likes => items.sort_by_key(|w| std::cmp::Reverse(w.likes)),
                GallerySort::Lightest => items.sort_by(|a, b| {
                    cost_rank(a.cost)
                        .cmp(&cost_rank(b.cost))
                        .then(a.gpu_pct.total_cmp(&b.gpu_pct))
                }),
            }
            Ok(items)
        })
    }

    fn wallpaper(&self, id: &str) -> Result<WallpaperDetail> {
        self.with(|s| {
            if let Some(d) = s.fx.details.iter().find(|d| d.id == id) {
                return Ok(d.clone());
            }
            let w = s.summary(id)?;
            let base =
                s.fx.details
                    .first()
                    .cloned()
                    .ok_or_else(|| DaemonError::UnknownWallpaper(id.to_owned()))?;
            let uses_label = if w.uses.contains(&Capability::Audio) {
                "Audio levels only"
            } else if w.uses.contains(&Capability::Mouse) {
                "Mouse position only"
            } else {
                "Nothing outside its folder"
            };
            let initials: String = w
                .author
                .chars()
                .filter(char::is_ascii_alphanumeric)
                .take(2)
                .collect();
            Ok(WallpaperDetail {
                description: format!("{} by {}.", w.title, w.author),
                version: "1.0.0".into(),
                uses_label: uses_label.into(),
                watts: (w.gpu_pct * 2.0).round() / 10.0,
                creator: crate::dto::Creator {
                    handle: w.author.clone(),
                    initials: initials.to_uppercase(),
                    wallpapers: 6,
                    installs: w.likes * 2,
                    since: 2026,
                    verified: false,
                },
                more_from_creator: s
                    .fx
                    .gallery
                    .iter()
                    .filter(|g| g.id != id)
                    .take(5)
                    .cloned()
                    .collect(),
                id: w.id,
                title: w.title,
                author: w.author,
                kind: w.kind,
                category: w.category,
                cost: w.cost,
                gpu_pct: w.gpu_pct,
                uses: w.uses,
                likes: w.likes,
                installed: w.installed,
                art: w.art,
                ..base
            })
        })
    }

    fn displays(&self) -> Result<Vec<Display>> {
        self.with(|s| Ok(s.fx.displays.clone()))
    }

    fn apply(&self, req: &ApplyRequest) -> Result<ApplyResult> {
        self.with(|s| {
            let w = s.summary(&req.wallpaper_id)?;
            if let Some(bad) = req
                .displays
                .iter()
                .find(|id| !s.fx.displays.iter().any(|d| d.id == **id))
            {
                return Err(DaemonError::UnknownDisplay(*bad));
            }
            for d in
                s.fx.displays
                    .iter_mut()
                    .filter(|d| req.displays.contains(&d.id))
            {
                d.wallpaper_id = w.id.clone();
                d.art = w.art.clone();
            }
            let count = u32::try_from(req.displays.len()).unwrap_or(u32::MAX);
            s.fx.status.now_playing = Some(NowPlaying {
                id: w.id,
                title: w.title,
                art: w.art,
                displays: count,
            });
            Ok(ApplyResult {
                applied: req.displays.clone(),
                estimated_gpu_pct: w.gpu_pct,
            })
        })
    }

    fn save_preset(&self, req: &PresetRequest) -> Result<u32> {
        self.with(|s| {
            if req.name.trim().is_empty() {
                return Err(DaemonError::Empty("preset name"));
            }
            s.summary(&req.wallpaper_id)?;
            let is_new = !s
                .saved_presets
                .iter()
                .any(|p| p.wallpaper_id == req.wallpaper_id && p.name == req.name);
            s.saved_presets
                .retain(|p| !(p.wallpaper_id == req.wallpaper_id && p.name == req.name));
            s.saved_presets.push(req.clone());
            let saved = s
                .saved_presets
                .iter()
                .filter(|p| p.wallpaper_id == req.wallpaper_id)
                .count();
            match s.fx.details.iter_mut().find(|d| d.id == req.wallpaper_id) {
                Some(d) => {
                    if is_new {
                        d.presets += 1;
                    }
                    Ok(d.presets)
                }
                None => Ok(u32::try_from(saved).unwrap_or(u32::MAX)),
            }
        })
    }

    fn install_request(&self, id: &str) -> Result<InstallRequest> {
        self.with(|s| {
            if let Some(r) = s.fx.install_requests.iter().find(|r| r.wallpaper_id == id) {
                return Ok(r.clone());
            }
            let w = s.summary(id)?;
            let always_blocked =
                s.fx.install_requests
                    .first()
                    .map(|r| r.always_blocked.clone())
                    .unwrap_or_default();
            let mut permissions = Vec::new();
            if w.uses.contains(&Capability::Audio) {
                permissions.push(audio_request());
            }
            if w.uses.contains(&Capability::Mouse) {
                permissions.push(mouse_request());
            }
            Ok(InstallRequest {
                wallpaper_id: w.id,
                title: w.title,
                author: w.author.clone(),
                version: "1.0.0".into(),
                kind_label: kind_label(w.kind).into(),
                size_bytes: 2_400_000,
                art: w.art,
                signature: Some(Signature {
                    signer: w.author,
                    fingerprint: "3C91\u{2026}A07D".into(),
                    unchanged: true,
                    listed: true,
                }),
                permissions,
                always_blocked,
            })
        })
    }

    fn install(&self, id: &str, granted: &[PermissionKey]) -> Result<InstallResult> {
        let req = self.install_request(id)?;
        self.with(|s| {
            if let Some(w) = s.fx.gallery.iter_mut().find(|w| w.id == id) {
                w.installed = true;
            }
            s.fx.status.library_count += 1;
            let (granted, denied): (Vec<_>, Vec<_>) = req
                .permissions
                .iter()
                .map(|p| p.key)
                .partition(|k| granted.contains(k));
            Ok(InstallResult {
                wallpaper_id: id.to_owned(),
                granted,
                denied,
            })
        })
    }

    fn performance(&self) -> Result<PerfSnapshot> {
        self.with(|s| Ok(s.fx.performance.clone()))
    }

    fn set_preset(&self, id: PresetId) -> Result<PerfSnapshot> {
        self.with(|s| {
            let p = &mut s.fx.performance;
            p.preset = id;
            p.settings.fps_cap = match id {
                PresetId::Quality | PresetId::Balanced => 60,
                PresetId::Saver | PresetId::Ultra => 30,
            };
            Ok(p.clone())
        })
    }

    fn set_pause_rule(&self, rule_id: &str, action: &str) -> Result<PerfSnapshot> {
        self.with(|s| {
            let rule =
                s.fx.performance
                    .rules
                    .iter_mut()
                    .find(|r| r.id == rule_id)
                    .ok_or_else(|| DaemonError::UnknownRule(rule_id.to_owned()))?;
            if !rule.options.iter().any(|o| o == action) {
                return Err(DaemonError::UnsupportedAction {
                    rule: rule_id.to_owned(),
                    action: action.to_owned(),
                });
            }
            rule.action = action.to_owned();
            Ok(s.fx.performance.clone())
        })
    }

    fn set_perf_settings(&self, settings: &PerfSettings) -> Result<PerfSnapshot> {
        self.with(|s| {
            let available = s.fx.performance.settings.hw_decode_available;
            s.fx.performance.settings = PerfSettings {
                hw_decode_available: available,
                ..settings.clone()
            };
            Ok(s.fx.performance.clone())
        })
    }

    fn set_app_rule(&self, exe: &str, action: &str) -> Result<PerfSnapshot> {
        let exe = exe.trim();
        if exe.is_empty() {
            return Err(DaemonError::Empty("app name"));
        }
        self.with(|s| {
            let rules = &mut s.fx.performance.app_rules;
            match rules.iter_mut().find(|r| r.exe.eq_ignore_ascii_case(exe)) {
                Some(r) => r.action = action.to_owned(),
                None => rules.push(crate::dto::AppRule {
                    exe: exe.to_owned(),
                    icon: "window".into(),
                    action: action.to_owned(),
                }),
            }
            Ok(s.fx.performance.clone())
        })
    }

    fn identify_displays(&self) -> Result<()> {
        Ok(())
    }

    fn pause_all(&self, paused: bool) -> Result<DaemonStatus> {
        self.with(|s| {
            s.fx.status.paused = paused;
            s.fx.status.displays_live = if paused { 0 } else { 2 };
            s.fx.status.cpu_pct = if paused { 0.0 } else { 1.9 };
            if paused {
                for d in &mut s.fx.performance.displays {
                    d.state = RunState::Paused;
                    d.fps = 0;
                    d.gpu_pct = 0.0;
                }
            } else {
                s.fx.performance.displays = s.initial_displays.clone();
            }
            Ok(s.fx.status.clone())
        })
    }

    fn creator_project(&self) -> Result<CreatorProject> {
        self.with(|s| Ok(s.fx.creator.clone()))
    }

    fn declare_origin(&self, origin: &str) -> Result<CreatorProject> {
        self.with(|s| {
            let c = &mut s.fx.creator;
            if !c.declared.iter().any(|d| d == "network") {
                c.declared.push("network".into());
            }
            c.issues.retain(|i| i.origin != origin);
            Ok(c.clone())
        })
    }

    fn pack_and_sign(&self) -> Result<PackResult> {
        self.with(|s| {
            let c = &s.fx.creator;
            Ok(PackResult {
                file: format!("{}-{}.wwp", c.name, c.version),
                size_bytes: c.total_bytes,
                signed_by: c.signer.name.clone(),
            })
        })
    }

    fn creator_action(&self, action: CreatorAction) -> Result<String> {
        self.with(|s| {
            let name = &s.fx.creator.name;
            Ok(match action {
                CreatorAction::OpenEditor => format!("Opened {name} in your editor"),
                CreatorAction::TryOnDesktop => "Showing it on your desktop for 30 seconds".into(),
                CreatorAction::SubmitToGallery => "Gallery pull request drafted".into(),
                CreatorAction::NewFile => "Created untitled.js".into(),
                CreatorAction::AddProperty => "Added a property to wallpaper.json".into(),
                CreatorAction::AddFiles => format!("Copied the files into {name}"),
                CreatorAction::TemplateVideo => {
                    "Started a new project from the video template".into()
                }
                CreatorAction::TemplateWeb => "Started a new project from the web template".into(),
                CreatorAction::TemplateShader => {
                    "Started a new project from the shader template".into()
                }
                CreatorAction::TemplateVisualizer => {
                    "Started a new project from the visualizer template".into()
                }
            })
        })
    }

    fn funding(&self) -> Result<Option<Funding>> {
        self.with(|s| Ok(s.fx.funding.clone()))
    }

    fn settings(&self) -> Result<Settings> {
        self.with(|s| Ok(s.fx.settings.clone()))
    }

    fn update_settings(&self, settings: &Settings) -> Result<Settings> {
        self.with(|s| {
            s.fx.settings = settings.clone();
            Ok(s.fx.settings.clone())
        })
    }
}
