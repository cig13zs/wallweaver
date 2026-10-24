//! macOS host. Not implemented yet.
//!
//! Plan: one borderless `NSWindow` per `NSScreen` at
//! `kCGDesktopWindowLevel`, with `canJoinAllSpaces`, `stationary` and
//! `ignoresCycle` collection behavior, ignoring mouse events, and a
//! `CAMetalLayer` content view handed to wgpu. Fullscreen and occlusion come
//! from `NSWorkspace` notifications and `NSWindow.occlusionState`; battery
//! from IOKit power sources. Window creation must happen on the main
//! thread, so the daemon's event loop runs there on macOS.

use crate::DesktopError;

/// The macOS host. It can't be constructed until it is implemented.
#[derive(Debug)]
pub enum MacHost {}

impl MacHost {
    /// Always fails with an explanation for now.
    pub fn new() -> Result<MacHost, DesktopError> {
        Err(DesktopError::Unsupported(
            "macOS is not supported yet: the desktop-level window host is planned (see docs/ROADMAP.md). \
             The renderer and tools already build on macOS."
                .into(),
        ))
    }
}

impl crate::DesktopHost for MacHost {
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
