//! Wallweaver manager window.
//!
//! This process exists only while its window is open. Wallpapers, the tray and the
//! power governor live in the daemon, which keeps running after the window closes.

pub mod commands;
pub mod daemon;
pub mod dto;
pub mod links;

#[cfg(test)]
mod tests;

use tauri::{Manager, WindowEvent};

use crate::commands::AppDaemon;
use crate::daemon::StubDaemon;

/// Builds and runs the manager window. Returns when the window closes.
pub fn run() {
    let daemon = match StubDaemon::bundled() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("wallweaver-manager: {e}");
            std::process::exit(1);
        }
    };

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(AppDaemon(Box::new(daemon)))
        .invoke_handler(tauri::generate_handler![
            commands::status,
            commands::featured,
            commands::gallery,
            commands::wallpaper,
            commands::displays,
            commands::apply,
            commands::save_preset,
            commands::install_request,
            commands::install,
            commands::performance,
            commands::set_preset,
            commands::set_pause_rule,
            commands::set_perf_settings,
            commands::set_app_rule,
            commands::identify_displays,
            commands::pause_all,
            commands::creator_project,
            commands::declare_origin,
            commands::pack_and_sign,
            commands::creator_action,
            commands::funding,
            commands::settings,
            commands::update_settings,
            commands::open_link,
        ])
        .on_window_event(|window, event| {
            // Closing the window ends this process; the daemon is a separate process and keeps going.
            if let WindowEvent::CloseRequested { .. } = event {
                window.app_handle().exit(0);
            }
        })
        .build(tauri::generate_context!());

    match app {
        Ok(app) => app.run(|_, _| {}),
        Err(e) => {
            eprintln!("wallweaver-manager: failed to start: {e}");
            std::process::exit(1);
        }
    }
}
