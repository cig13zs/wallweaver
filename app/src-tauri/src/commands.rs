//! Tauri commands. Each one forwards to the [`Daemon`] held in app state; names
//! match `ui/src/lib/daemon/tauri.ts` and the allow-list in `capabilities/`.

use serde::Serialize;
use tauri::State;

use crate::daemon::{Daemon, DaemonError};
use crate::dto::{
    ApplyRequest, ApplyResult, CreatorAction, CreatorProject, DaemonStatus, Display, Featured,
    Funding, GalleryQuery, InstallRequest, InstallResult, LinkId, PackResult, PerfSettings,
    PerfSnapshot, PermissionKey, PresetId, PresetRequest, Settings, WallpaperDetail,
    WallpaperSummary,
};
use crate::links;

/// The daemon client this window talks to.
pub struct AppDaemon(pub Box<dyn Daemon>);

impl std::fmt::Debug for AppDaemon {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("AppDaemon")
    }
}

/// Errors cross the IPC boundary as plain messages the UI can show.
#[derive(Debug, thiserror::Error)]
pub enum CommandError {
    #[error(transparent)]
    Daemon(#[from] DaemonError),
    #[error("couldn't open the link: {0}")]
    Open(String),
}

impl Serialize for CommandError {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

type Res<T> = Result<T, CommandError>;
type D<'a> = State<'a, AppDaemon>;

#[tauri::command]
pub async fn status(d: D<'_>) -> Res<DaemonStatus> {
    Ok(d.0.status()?)
}

#[tauri::command]
pub async fn featured(d: D<'_>) -> Res<Featured> {
    Ok(d.0.featured()?)
}

#[tauri::command]
pub async fn gallery(d: D<'_>, query: GalleryQuery) -> Res<Vec<WallpaperSummary>> {
    Ok(d.0.gallery(&query)?)
}

#[tauri::command]
pub async fn wallpaper(d: D<'_>, id: String) -> Res<WallpaperDetail> {
    Ok(d.0.wallpaper(&id)?)
}

#[tauri::command]
pub async fn displays(d: D<'_>) -> Res<Vec<Display>> {
    Ok(d.0.displays()?)
}

#[tauri::command]
pub async fn apply(d: D<'_>, req: ApplyRequest) -> Res<ApplyResult> {
    Ok(d.0.apply(&req)?)
}

#[tauri::command]
pub async fn save_preset(d: D<'_>, req: PresetRequest) -> Res<u32> {
    Ok(d.0.save_preset(&req)?)
}

#[tauri::command]
pub async fn install_request(d: D<'_>, id: String) -> Res<InstallRequest> {
    Ok(d.0.install_request(&id)?)
}

#[tauri::command]
pub async fn install(d: D<'_>, id: String, granted: Vec<PermissionKey>) -> Res<InstallResult> {
    Ok(d.0.install(&id, &granted)?)
}

#[tauri::command]
pub async fn performance(d: D<'_>) -> Res<PerfSnapshot> {
    Ok(d.0.performance()?)
}

#[tauri::command]
pub async fn set_preset(d: D<'_>, id: PresetId) -> Res<PerfSnapshot> {
    Ok(d.0.set_preset(id)?)
}

#[tauri::command]
pub async fn set_pause_rule(d: D<'_>, rule_id: String, action: String) -> Res<PerfSnapshot> {
    Ok(d.0.set_pause_rule(&rule_id, &action)?)
}

#[tauri::command]
pub async fn set_perf_settings(d: D<'_>, settings: PerfSettings) -> Res<PerfSnapshot> {
    Ok(d.0.set_perf_settings(&settings)?)
}

#[tauri::command]
pub async fn set_app_rule(d: D<'_>, exe: String, action: String) -> Res<PerfSnapshot> {
    Ok(d.0.set_app_rule(&exe, &action)?)
}

#[tauri::command]
pub async fn identify_displays(d: D<'_>) -> Res<()> {
    Ok(d.0.identify_displays()?)
}

#[tauri::command]
pub async fn pause_all(d: D<'_>, paused: bool) -> Res<DaemonStatus> {
    Ok(d.0.pause_all(paused)?)
}

#[tauri::command]
pub async fn creator_project(d: D<'_>) -> Res<CreatorProject> {
    Ok(d.0.creator_project()?)
}

#[tauri::command]
pub async fn declare_origin(d: D<'_>, origin: String) -> Res<CreatorProject> {
    Ok(d.0.declare_origin(&origin)?)
}

#[tauri::command]
pub async fn pack_and_sign(d: D<'_>) -> Res<PackResult> {
    Ok(d.0.pack_and_sign()?)
}

#[tauri::command]
pub async fn creator_action(d: D<'_>, action: CreatorAction) -> Res<String> {
    Ok(d.0.creator_action(action)?)
}

#[tauri::command]
pub async fn funding(d: D<'_>) -> Res<Option<Funding>> {
    Ok(d.0.funding()?)
}

#[tauri::command]
pub async fn settings(d: D<'_>) -> Res<Settings> {
    Ok(d.0.settings()?)
}

#[tauri::command]
pub async fn update_settings(d: D<'_>, settings: Settings) -> Res<Settings> {
    Ok(d.0.update_settings(&settings)?)
}

/// Opens one of a fixed set of project pages in the default browser. The UI only
/// names the link; it can never hand the shell an arbitrary URL.
#[tauri::command]
pub async fn open_link(app: tauri::AppHandle, id: LinkId, subject: Option<String>) -> Res<()> {
    use tauri_plugin_opener::OpenerExt;
    let url = links::url_for(id, subject.as_deref());
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|e| CommandError::Open(e.to_string()))
}
