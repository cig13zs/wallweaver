//! Wayland host. Not implemented yet.
//!
//! Plan: a `zwlr_layer_shell_v1` surface on the `background` layer per
//! output, anchored to all four edges with exclusive zone -1 and no
//! keyboard interactivity, created through `wayland-client` and handed to
//! wgpu as a Wayland surface. Fullscreen detection uses
//! `zwlr_foreign_toplevel_manager_v1` where the compositor offers it.
//! Works on wlroots compositors and KDE Plasma; GNOME has no layer-shell and
//! needs a Shell extension instead.

use crate::DesktopError;

/// The Wayland host. It can't be constructed until it is implemented.
#[derive(Debug)]
pub enum WaylandHost {}

impl WaylandHost {
    /// Always fails with an explanation for now.
    pub fn new() -> Result<WaylandHost, DesktopError> {
        Err(DesktopError::Unsupported(
            "Wayland sessions are not supported yet: the wlr-layer-shell background host is planned for v0.2. \
             On X11 sessions Wallweaver works today."
                .into(),
        ))
    }
}

impl crate::DesktopHost for WaylandHost {
    fn name(&self) -> &'static str {
        match *self {}
    }
    fn monitors(&mut self) -> Result<Vec<crate::Monitor>, DesktopError> {
        match *self {}
    }
    fn create_window(&mut self, _: &crate::Monitor) -> Result<crate::HostWindow, DesktopError> {
        match *self {}
    }
    fn destroy_window(&mut self, _: ww_governor::MonitorId) {
        match *self {}
    }
    fn show(&mut self, _: ww_governor::MonitorId, _: bool) {
        match *self {}
    }
    fn signals(&mut self, _: &[crate::Monitor]) -> ww_governor::Signals {
        match *self {}
    }
    fn cursor_position(&mut self) -> Option<(i32, i32)> {
        match *self {}
    }
    fn pump(&mut self, _: std::time::Duration) -> Vec<crate::HostEvent> {
        match *self {}
    }
}
