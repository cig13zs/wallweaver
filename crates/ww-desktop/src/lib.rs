//! Desktop hosts: the part of Wallweaver that talks to the window system.
//!
//! A [`DesktopHost`] lists monitors, creates one window per monitor that
//! sits behind the desktop icons, reports what the governor needs to know
//! (fullscreen apps, covered desktops, battery, lock state) and tells the
//! daemon when monitors or the shell change.
//!
//! | Platform | Status |
//! |---|---|
//! | Windows 10/11 | [`windows`] module: `WorkerW` behind the icons, and the 24H2+ layout (a layered child of `Progman` below `SHELLDLL_DefView`) |
//! | Linux X11 | [`x11`] module: a `_NET_WM_WINDOW_TYPE_DESKTOP` window per RandR monitor |
//! | Linux Wayland | not yet: needs a `wlr-layer-shell` background surface ([`wayland`]) |
//! | macOS | not yet: needs a borderless window at the desktop window level ([`macos`]) |
//! | anything | [`NullHost`]: one virtual monitor, no windows (headless runs, benchmarks, tests) |

pub mod geometry;
pub mod macos;
pub mod power;
pub mod process;
pub mod wayland;

#[cfg(windows)]
pub mod windows;

#[cfg(all(unix, not(target_vendor = "apple")))]
pub mod x11;

use std::time::Duration;

use raw_window_handle::{RawDisplayHandle, RawWindowHandle};
use ww_governor::{MonitorId, Signals};

pub use geometry::Rect;

/// Why a host operation failed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DesktopError {
    /// This platform or session type has no host yet.
    #[error("{0}")]
    Unsupported(String),
    /// No display server to talk to.
    #[error("no display available: {0}")]
    NoDisplay(String),
    /// The window system refused something.
    #[error("{0}")]
    Platform(String),
    /// The monitor id is unknown.
    #[error("no monitor with id {0}")]
    NoSuchMonitor(MonitorId),
}

/// A connected monitor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Monitor {
    /// Stable for as long as the monitor stays connected.
    pub id: MonitorId,
    /// Name from the OS, such as `DP-1` or `\\.\DISPLAY1`.
    pub name: String,
    /// Position and size in desktop pixels.
    pub rect: Rect,
    /// True for the primary monitor.
    pub primary: bool,
}

/// A wallpaper window created by a host.
#[derive(Debug, Clone, Copy)]
pub struct HostWindow {
    /// The monitor it covers.
    pub monitor: MonitorId,
    /// Display connection handle for the renderer.
    pub display: RawDisplayHandle,
    /// Window handle for the renderer.
    pub window: RawWindowHandle,
    /// Size in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
}

/// Wakes a host's [`DesktopHost::pump`] from another thread, so the daemon
/// can sleep in the window-system wait and still react to IPC at once.
#[derive(Clone)]
pub struct Waker(std::sync::Arc<dyn Fn() + Send + Sync>);

impl Waker {
    /// Wrap a wake function.
    pub fn new(f: impl Fn() + Send + Sync + 'static) -> Waker {
        Waker(std::sync::Arc::new(f))
    }

    /// Wake the pump.
    pub fn wake(&self) {
        (self.0)()
    }
}

impl std::fmt::Debug for Waker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Waker")
    }
}

/// Something that happened in the window system.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostEvent {
    /// Woken through a [`Waker`].
    Woken,
    /// Tray menu: open the manager window.
    TrayOpenManager,
    /// Tray menu: pause or resume all wallpapers.
    TrayTogglePause,
    /// Tray menu: quit.
    TrayQuit,
    /// Monitors were added, removed, moved or resized.
    MonitorsChanged,
    /// The desktop shell restarted (for example Explorer crashed); windows
    /// must be created again.
    ShellRestarted,
    /// The session was locked or unlocked, or power state changed; read
    /// [`DesktopHost::signals`] again.
    SignalsChanged,
}

/// One OS's way of putting wallpapers behind the desktop.
pub trait DesktopHost {
    /// Short name, such as `windows` or `x11`.
    fn name(&self) -> &'static str;

    /// Connected monitors.
    fn monitors(&mut self) -> Result<Vec<Monitor>, DesktopError>;

    /// Create (or recreate) the wallpaper window for a monitor. It starts
    /// hidden; call [`DesktopHost::show`] after the first frame is drawn so
    /// there is no flash of an empty window.
    fn create_window(&mut self, monitor: &Monitor) -> Result<HostWindow, DesktopError>;

    /// Destroy a monitor's window. The renderer must have released its
    /// surface first.
    fn destroy_window(&mut self, monitor: MonitorId);

    /// Show or hide a monitor's window. Hidden windows let the OS wallpaper
    /// show through, which is what a stopped wallpaper looks like.
    fn show(&mut self, monitor: MonitorId, visible: bool);

    /// What the governor needs to know right now. User flags
    /// (`user_paused`, `user_stopped`) are left false for the daemon to set.
    fn signals(&mut self, monitors: &[Monitor]) -> Signals;

    /// Pointer position in desktop pixels, if known.
    fn cursor_position(&mut self) -> Option<(i32, i32)>;

    /// Handle pending window-system messages, waiting at most `timeout`.
    fn pump(&mut self, timeout: Duration) -> Vec<HostEvent>;

    /// A handle that interrupts [`DesktopHost::pump`] from any thread.
    fn waker(&self) -> Waker;

    /// Show the tray icon, or update its menu to reflect the pause state.
    /// Hosts without a tray ignore this; the manager window has the same
    /// controls.
    fn set_tray(&mut self, _paused: bool) {}
}

/// The host for the current platform and session.
///
/// On Linux, a Wayland session gets the Wayland host (not implemented yet,
/// so this returns an error) unless `WW_FORCE_X11=1` is set, which uses
/// XWayland; that works on some compositors only.
pub fn platform_host() -> Result<Box<dyn DesktopHost>, DesktopError> {
    #[cfg(windows)]
    {
        windows::WindowsHost::new().map(|h| Box::new(h) as Box<dyn DesktopHost>)
    }
    #[cfg(target_vendor = "apple")]
    {
        macos::MacHost::new().map(|h| Box::new(h) as Box<dyn DesktopHost>)
    }
    #[cfg(all(unix, not(target_vendor = "apple")))]
    {
        let wayland = std::env::var_os("WAYLAND_DISPLAY").is_some_and(|v| !v.is_empty());
        let force_x11 = std::env::var_os("WW_FORCE_X11").is_some_and(|v| v == "1");
        if wayland && !force_x11 {
            return wayland::WaylandHost::new().map(|h| Box::new(h) as Box<dyn DesktopHost>);
        }
        x11::X11Host::new().map(|h| Box::new(h) as Box<dyn DesktopHost>)
    }
    #[cfg(not(any(windows, unix)))]
    {
        Err(DesktopError::Unsupported(
            "this operating system has no desktop host".into(),
        ))
    }
}

/// A host with one virtual monitor and no windows. The daemon uses it in
/// `--headless` mode, where wallpapers render offscreen (benchmarks, CI).
#[derive(Debug, Clone)]
pub struct NullHost {
    monitor: Monitor,
    wake: std::sync::Arc<(std::sync::Mutex<bool>, std::sync::Condvar)>,
}

impl NullHost {
    /// One monitor of the given size.
    pub fn new(width: u32, height: u32) -> NullHost {
        NullHost {
            monitor: Monitor {
                id: 0,
                name: "virtual".into(),
                rect: Rect::new(0, 0, width, height),
                primary: true,
            },
            wake: Default::default(),
        }
    }
}

impl DesktopHost for NullHost {
    fn name(&self) -> &'static str {
        "headless"
    }

    fn monitors(&mut self) -> Result<Vec<Monitor>, DesktopError> {
        Ok(vec![self.monitor.clone()])
    }

    fn create_window(&mut self, monitor: &Monitor) -> Result<HostWindow, DesktopError> {
        Err(DesktopError::Unsupported(format!(
            "the headless host has no windows (monitor {})",
            monitor.id
        )))
    }

    fn destroy_window(&mut self, _monitor: MonitorId) {}

    fn show(&mut self, _monitor: MonitorId, _visible: bool) {}

    fn signals(&mut self, monitors: &[Monitor]) -> Signals {
        let mut s = Signals::default();
        for m in monitors {
            s.monitors.insert(m.id, Default::default());
        }
        s
    }

    fn cursor_position(&mut self) -> Option<(i32, i32)> {
        None
    }

    fn pump(&mut self, timeout: Duration) -> Vec<HostEvent> {
        let (lock, cv) = &*self.wake;
        let Ok(guard) = lock.lock() else {
            return Vec::new();
        };
        let Ok((mut woken, _)) = cv.wait_timeout_while(guard, timeout, |w| !*w) else {
            return Vec::new();
        };
        if *woken {
            *woken = false;
            vec![HostEvent::Woken]
        } else {
            Vec::new()
        }
    }

    fn waker(&self) -> Waker {
        let wake = self.wake.clone();
        Waker::new(move || {
            let (lock, cv) = &*wake;
            if let Ok(mut w) = lock.lock() {
                *w = true;
                cv.notify_one();
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn null_host_has_one_monitor_and_no_windows() {
        let mut h = NullHost::new(1920, 1080);
        let mons = h.monitors().unwrap();
        assert_eq!(mons.len(), 1);
        assert_eq!(mons[0].rect.width, 1920);
        assert!(h.create_window(&mons[0]).is_err());
        let s = h.signals(&mons);
        assert!(s.monitors.contains_key(&0));
        assert!(!s.on_battery);
    }

    #[test]
    fn null_host_wakes_from_another_thread() {
        let mut h = NullHost::new(8, 8);
        let w = h.waker();
        let t = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(50));
            w.wake();
        });
        let start = std::time::Instant::now();
        let ev = h.pump(Duration::from_secs(10));
        assert_eq!(ev, vec![HostEvent::Woken]);
        assert!(start.elapsed() < Duration::from_secs(5));
        t.join().unwrap();
        assert!(h.pump(Duration::from_millis(10)).is_empty());
    }

    #[test]
    fn stub_hosts_explain_what_is_missing() {
        let err = wayland::WaylandHost::new().unwrap_err().to_string();
        assert!(err.contains("layer-shell"), "{err}");
        let err = macos::MacHost::new().unwrap_err().to_string();
        assert!(err.contains("macOS"), "{err}");
    }
}
