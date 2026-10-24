// Declares the app's own commands so each one gets an `allow-*` permission
// and only commands listed in capabilities/ are callable from the UI.
const COMMANDS: &[&str] = &[
    "status",
    "featured",
    "gallery",
    "wallpaper",
    "displays",
    "apply",
    "save_preset",
    "install_request",
    "install",
    "performance",
    "set_preset",
    "set_pause_rule",
    "set_perf_settings",
    "set_app_rule",
    "identify_displays",
    "pause_all",
    "creator_project",
    "declare_origin",
    "pack_and_sign",
    "creator_action",
    "funding",
    "settings",
    "update_settings",
    "open_link",
];

fn main() {
    tauri_build::try_build(
        tauri_build::Attributes::new()
            .app_manifest(tauri_build::AppManifest::new().commands(COMMANDS)),
    )
    .expect("failed to run tauri-build");
}
