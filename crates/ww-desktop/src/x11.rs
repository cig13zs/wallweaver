//! X11 host.
//!
//! Each monitor (from RandR) gets a window of type
//! `_NET_WM_WINDOW_TYPE_DESKTOP`, which EWMH window managers keep below
//! every other window, on all workspaces, out of the taskbar and the pager.
//! The window's input shape is empty, so clicks fall through to whatever is
//! underneath (desktop icons, if a file manager draws them).
//!
//! Fullscreen detection reads `_NET_ACTIVE_WINDOW` and that window's
//! `_NET_WM_STATE`. Battery comes from `/sys/class/power_supply`. Lock
//! detection is not available on plain X11 and is reported as unlocked.
//!
//! The connection goes through libxcb (loaded at run time) so the renderer
//! can create a Vulkan or GL surface from the same connection.

use std::collections::BTreeMap;
use std::num::NonZeroU32;
use std::os::fd::AsRawFd;
use std::ptr::NonNull;
use std::time::{Duration, Instant};

use raw_window_handle::{RawDisplayHandle, RawWindowHandle, XcbDisplayHandle, XcbWindowHandle};
use ww_governor::{MonitorId, MonitorSignals, Signals};
use x11rb::connection::Connection;
use x11rb::protocol::Event;
use x11rb::protocol::randr::{self, ConnectionExt as _};
use x11rb::protocol::shape::{self, ConnectionExt as _};
use x11rb::protocol::xproto::{
    AtomEnum, ClipOrdering, ConnectionExt as _, CreateWindowAux, EventMask, PropMode, Window,
    WindowClass,
};
use x11rb::wrapper::ConnectionExt as _;
use x11rb::xcb_ffi::XCBConnection;

use crate::geometry::{Rect, covered_fraction, monitor_for};
use crate::power::read_power_supplies;
use crate::{DesktopError, DesktopHost, HostEvent, HostWindow, Monitor};

struct Atoms {
    net_wm_window_type: u32,
    net_wm_window_type_desktop: u32,
    net_wm_state: u32,
    net_wm_state_below: u32,
    net_wm_state_sticky: u32,
    net_wm_state_skip_taskbar: u32,
    net_wm_state_skip_pager: u32,
    net_wm_state_fullscreen: u32,
    net_wm_state_maximized_vert: u32,
    net_wm_state_maximized_horz: u32,
    net_wm_state_hidden: u32,
    net_wm_desktop: u32,
    net_active_window: u32,
    net_wm_pid: u32,
    net_wm_name: u32,
    utf8_string: u32,
}

/// The X11 desktop host.
pub struct X11Host {
    conn: XCBConnection,
    screen: usize,
    root: Window,
    atoms: Atoms,
    windows: BTreeMap<MonitorId, Window>,
    randr: bool,
}

impl std::fmt::Debug for X11Host {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("X11Host")
            .field("screen", &self.screen)
            .field("windows", &self.windows)
            .finish_non_exhaustive()
    }
}

fn err(e: impl std::fmt::Display) -> DesktopError {
    DesktopError::Platform(e.to_string())
}

impl X11Host {
    /// Connect to the X server named by `$DISPLAY`.
    pub fn new() -> Result<X11Host, DesktopError> {
        let (conn, screen) =
            XCBConnection::connect(None).map_err(|e| DesktopError::NoDisplay(e.to_string()))?;
        let root = conn.setup().roots[screen].root;
        let intern = |name: &[u8]| -> Result<u32, DesktopError> {
            Ok(conn
                .intern_atom(false, name)
                .map_err(err)?
                .reply()
                .map_err(err)?
                .atom)
        };
        let atoms = Atoms {
            net_wm_window_type: intern(b"_NET_WM_WINDOW_TYPE")?,
            net_wm_window_type_desktop: intern(b"_NET_WM_WINDOW_TYPE_DESKTOP")?,
            net_wm_state: intern(b"_NET_WM_STATE")?,
            net_wm_state_below: intern(b"_NET_WM_STATE_BELOW")?,
            net_wm_state_sticky: intern(b"_NET_WM_STATE_STICKY")?,
            net_wm_state_skip_taskbar: intern(b"_NET_WM_STATE_SKIP_TASKBAR")?,
            net_wm_state_skip_pager: intern(b"_NET_WM_STATE_SKIP_PAGER")?,
            net_wm_state_fullscreen: intern(b"_NET_WM_STATE_FULLSCREEN")?,
            net_wm_state_maximized_vert: intern(b"_NET_WM_STATE_MAXIMIZED_VERT")?,
            net_wm_state_maximized_horz: intern(b"_NET_WM_STATE_MAXIMIZED_HORZ")?,
            net_wm_state_hidden: intern(b"_NET_WM_STATE_HIDDEN")?,
            net_wm_desktop: intern(b"_NET_WM_DESKTOP")?,
            net_active_window: intern(b"_NET_ACTIVE_WINDOW")?,
            net_wm_pid: intern(b"_NET_WM_PID")?,
            net_wm_name: intern(b"_NET_WM_NAME")?,
            utf8_string: intern(b"UTF8_STRING")?,
        };
        let randr = conn
            .randr_query_version(1, 5)
            .ok()
            .and_then(|c| c.reply().ok())
            .is_some_and(|v| v.major_version > 1 || (v.major_version == 1 && v.minor_version >= 5));
        if randr {
            let _ = conn.randr_select_input(root, randr::NotifyMask::SCREEN_CHANGE);
        }
        // Follow the active window so fullscreen changes are noticed.
        let _ = conn.change_window_attributes(
            root,
            &x11rb::protocol::xproto::ChangeWindowAttributesAux::new()
                .event_mask(EventMask::PROPERTY_CHANGE),
        );
        let _ = conn.flush();
        Ok(X11Host {
            conn,
            screen,
            root,
            atoms,
            windows: BTreeMap::new(),
            randr,
        })
    }

    fn property32(&self, window: Window, prop: u32, ty: impl Into<u32>, max: u32) -> Vec<u32> {
        self.conn
            .get_property(false, window, prop, ty.into(), 0, max)
            .ok()
            .and_then(|c| c.reply().ok())
            .and_then(|r| r.value32().map(|v| v.collect()))
            .unwrap_or_default()
    }

    fn active_window(&self) -> Option<Window> {
        self.property32(self.root, self.atoms.net_active_window, AtomEnum::WINDOW, 1)
            .first()
            .copied()
            .filter(|w| *w != 0)
    }

    fn window_rect(&self, w: Window) -> Option<Rect> {
        let geo = self.conn.get_geometry(w).ok()?.reply().ok()?;
        let tr = self
            .conn
            .translate_coordinates(w, self.root, 0, 0)
            .ok()?
            .reply()
            .ok()?;
        Some(Rect::new(
            i32::from(tr.dst_x),
            i32::from(tr.dst_y),
            u32::from(geo.width),
            u32::from(geo.height),
        ))
    }

    fn process_name(&self, w: Window) -> Option<String> {
        let pid = *self
            .property32(w, self.atoms.net_wm_pid, AtomEnum::CARDINAL, 1)
            .first()?;
        let comm = std::fs::read_to_string(format!("/proc/{pid}/comm")).ok()?;
        Some(comm.trim().to_string())
    }
}

impl DesktopHost for X11Host {
    fn name(&self) -> &'static str {
        "x11"
    }

    fn monitors(&mut self) -> Result<Vec<Monitor>, DesktopError> {
        if self.randr
            && let Ok(reply) = self
                .conn
                .randr_get_monitors(self.root, true)
                .map_err(err)?
                .reply()
            && !reply.monitors.is_empty()
        {
            return Ok(reply
                .monitors
                .iter()
                .enumerate()
                .map(|(i, m)| {
                    let name = self
                        .conn
                        .get_atom_name(m.name)
                        .ok()
                        .and_then(|c| c.reply().ok())
                        .map(|r| String::from_utf8_lossy(&r.name).into_owned())
                        .unwrap_or_else(|| format!("monitor-{i}"));
                    Monitor {
                        id: i as MonitorId,
                        name,
                        rect: Rect::new(
                            i32::from(m.x),
                            i32::from(m.y),
                            u32::from(m.width),
                            u32::from(m.height),
                        ),
                        primary: m.primary,
                    }
                })
                .collect());
        }
        let s = &self.conn.setup().roots[self.screen];
        Ok(vec![Monitor {
            id: 0,
            name: "screen".into(),
            rect: Rect::new(
                0,
                0,
                u32::from(s.width_in_pixels),
                u32::from(s.height_in_pixels),
            ),
            primary: true,
        }])
    }

    fn create_window(&mut self, monitor: &Monitor) -> Result<HostWindow, DesktopError> {
        self.destroy_window(monitor.id);
        let r = monitor.rect;
        let win = self.conn.generate_id().map_err(err)?;
        self.conn
            .create_window(
                x11rb::COPY_DEPTH_FROM_PARENT,
                win,
                self.root,
                r.x as i16,
                r.y as i16,
                r.width.min(u32::from(u16::MAX)) as u16,
                r.height.min(u32::from(u16::MAX)) as u16,
                0,
                WindowClass::INPUT_OUTPUT,
                x11rb::COPY_FROM_PARENT,
                &CreateWindowAux::new()
                    .background_pixel(0)
                    .event_mask(EventMask::STRUCTURE_NOTIFY),
            )
            .map_err(err)?;
        let a = &self.atoms;
        let c = &self.conn;
        c.change_property32(
            PropMode::REPLACE,
            win,
            a.net_wm_window_type,
            AtomEnum::ATOM,
            &[a.net_wm_window_type_desktop],
        )
        .map_err(err)?;
        c.change_property32(
            PropMode::REPLACE,
            win,
            a.net_wm_state,
            AtomEnum::ATOM,
            &[
                a.net_wm_state_below,
                a.net_wm_state_sticky,
                a.net_wm_state_skip_taskbar,
                a.net_wm_state_skip_pager,
            ],
        )
        .map_err(err)?;
        c.change_property32(
            PropMode::REPLACE,
            win,
            a.net_wm_desktop,
            AtomEnum::CARDINAL,
            &[0xFFFF_FFFF],
        )
        .map_err(err)?;
        c.change_property8(
            PropMode::REPLACE,
            win,
            AtomEnum::WM_CLASS,
            AtomEnum::STRING,
            b"wallweaver\0Wallweaver\0",
        )
        .map_err(err)?;
        c.change_property8(
            PropMode::REPLACE,
            win,
            a.net_wm_name,
            a.utf8_string,
            b"Wallweaver wallpaper",
        )
        .map_err(err)?;
        // WM_NORMAL_HINTS with USPosition|USSize so the window manager keeps
        // it exactly on the monitor.
        let hints: [u32; 18] = [
            1 | 2,
            r.x as u32,
            r.y as u32,
            r.width,
            r.height,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
        ];
        c.change_property32(
            PropMode::REPLACE,
            win,
            AtomEnum::WM_NORMAL_HINTS,
            AtomEnum::WM_SIZE_HINTS,
            &hints,
        )
        .map_err(err)?;
        // Empty input shape: clicks pass through.
        let _ = c.shape_rectangles(
            shape::SO::SET,
            shape::SK::INPUT,
            ClipOrdering::UNSORTED,
            win,
            0,
            0,
            &[],
        );
        c.flush().map_err(err)?;
        self.windows.insert(monitor.id, win);

        let raw_conn = NonNull::new(self.conn.get_raw_xcb_connection())
            .ok_or_else(|| DesktopError::Platform("no xcb connection".into()))?;
        let display = XcbDisplayHandle::new(Some(raw_conn), self.screen as i32);
        let window = XcbWindowHandle::new(
            NonZeroU32::new(win).ok_or_else(|| DesktopError::Platform("window id 0".into()))?,
        );
        Ok(HostWindow {
            monitor: monitor.id,
            display: RawDisplayHandle::Xcb(display),
            window: RawWindowHandle::Xcb(window),
            width: r.width,
            height: r.height,
        })
    }

    fn destroy_window(&mut self, monitor: MonitorId) {
        if let Some(w) = self.windows.remove(&monitor) {
            let _ = self.conn.destroy_window(w);
            let _ = self.conn.flush();
        }
    }

    fn show(&mut self, monitor: MonitorId, visible: bool) {
        if let Some(w) = self.windows.get(&monitor) {
            let _ = if visible {
                self.conn.map_window(*w)
            } else {
                self.conn.unmap_window(*w)
            };
            let _ = self.conn.flush();
        }
    }

    fn signals(&mut self, monitors: &[Monitor]) -> Signals {
        let mut s = Signals::default();
        for m in monitors {
            s.monitors.insert(m.id, MonitorSignals::default());
        }
        let power = read_power_supplies(std::path::Path::new("/sys/class/power_supply"));
        s.on_battery = power.on_battery;
        s.battery_percent = power.percent;

        let Some(active) = self.active_window() else {
            return s;
        };
        if self.windows.values().any(|w| *w == active) {
            return s;
        }
        let state = self.property32(active, self.atoms.net_wm_state, AtomEnum::ATOM, 32);
        if state.contains(&self.atoms.net_wm_state_hidden) {
            return s;
        }
        let Some(rect) = self.window_rect(active) else {
            return s;
        };
        let Some(mon_id) = monitor_for(&rect, monitors.iter().map(|m| (m.id, &m.rect))) else {
            return s;
        };
        let Some(mon) = monitors.iter().find(|m| m.id == mon_id) else {
            return s;
        };
        let maximized = state.contains(&self.atoms.net_wm_state_maximized_vert)
            && state.contains(&self.atoms.net_wm_state_maximized_horz);
        let entry = s.monitors.entry(mon_id).or_default();
        entry.fullscreen_app = state.contains(&self.atoms.net_wm_state_fullscreen);
        entry.covered_fraction = if maximized || entry.fullscreen_app {
            1.0
        } else {
            covered_fraction(&rect, &mon.rect)
        };
        entry.foreground_app = self.process_name(active);
        s
    }

    fn cursor_position(&mut self) -> Option<(i32, i32)> {
        let p = self.conn.query_pointer(self.root).ok()?.reply().ok()?;
        Some((i32::from(p.root_x), i32::from(p.root_y)))
    }

    fn pump(&mut self, timeout: Duration) -> Vec<HostEvent> {
        let deadline = Instant::now() + timeout;
        let mut events = Vec::new();
        loop {
            while let Ok(Some(ev)) = self.conn.poll_for_event() {
                match ev {
                    Event::RandrScreenChangeNotify(_) => events.push(HostEvent::MonitorsChanged),
                    Event::PropertyNotify(p) if p.atom == self.atoms.net_active_window => {
                        events.push(HostEvent::SignalsChanged)
                    }
                    _ => {}
                }
            }
            let now = Instant::now();
            if !events.is_empty() || now >= deadline {
                events.dedup();
                return events;
            }
            let ms = (deadline - now).as_millis().min(i32::MAX as u128) as i32;
            let mut pfd = libc::pollfd {
                fd: self.conn.as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            };
            // SAFETY: pfd is a single valid pollfd for the duration of the call.
            unsafe { libc::poll(&mut pfd, 1, ms) };
        }
    }
}

impl Drop for X11Host {
    fn drop(&mut self) {
        let ids: Vec<_> = self.windows.keys().copied().collect();
        for id in ids {
            self.destroy_window(id);
        }
    }
}
