//! Windows host.
//!
//! The wallpaper layer of the Windows desktop is not a public API, so this
//! follows the long-standing technique and the layout change Windows 11
//! 24H2 introduced, written from Microsoft's description of the new layout
//! (no code taken from other wallpaper apps):
//!
//! * **Classic layout (Windows 10, Windows 11 before 24H2).** Sending the
//!   undocumented message `0x052C` to `Progman` makes Explorer split the
//!   desktop into the icon layer (`SHELLDLL_DefView`, inside a top-level
//!   `WorkerW`) and a separate empty top-level `WorkerW` behind it. Our
//!   window becomes a child of that empty `WorkerW`.
//! * **24H2 layout.** `Progman` itself carries `WS_EX_NOREDIRECTIONBITMAP`,
//!   and the wallpaper `WorkerW` is a *child* of `Progman`, below the
//!   layered `SHELLDLL_DefView`. Our window is made `WS_EX_LAYERED` (fully
//!   opaque) before it is parented to `Progman`, then placed below
//!   `SHELLDLL_DefView` and above that `WorkerW`.
//!
//! The layout is detected from `Progman`'s extended style, not from the
//! build number, so preview builds and future updates that keep the new
//! layout work without changes. When Explorer restarts, the
//! `TaskbarCreated` broadcast arrives and the daemon recreates the windows.

use std::collections::BTreeMap;
use std::num::NonZeroIsize;
use std::time::{Duration, Instant};

use raw_window_handle::{
    RawDisplayHandle, RawWindowHandle, Win32WindowHandle, WindowsDisplayHandle,
};
use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITOR_DEFAULTTONULL, MONITORINFO,
    MONITORINFOEXW, MapWindowPoints, MonitorFromWindow,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
use windows::Win32::System::StationsAndDesktops::{
    CloseDesktop, DESKTOP_CONTROL_FLAGS, DESKTOP_SWITCHDESKTOP, OpenInputDesktop,
};
use windows::Win32::System::Threading::{
    OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows::Win32::UI::Shell::{
    NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY, NOTIFYICONDATAW, QUNS_BUSY,
    QUNS_PRESENTATION_MODE, QUNS_RUNNING_D3D_FULL_SCREEN, SHQueryUserNotificationState,
    Shell_NotifyIconW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, DestroyMenu, IDI_APPLICATION, LoadIconW, MF_STRING, PostMessageW,
    SetForegroundWindow, TPM_RETURNCMD, TPM_RIGHTBUTTON, TrackPopupMenu, WM_APP, WM_CONTEXTMENU,
    WM_LBUTTONDBLCLK, WM_RBUTTONUP,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CS_HREDRAW, CS_VREDRAW, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW,
    EnumWindows, FindWindowExW, FindWindowW, GWL_EXSTYLE, GWL_STYLE, GetClassNameW, GetCursorPos,
    GetForegroundWindow, GetWindowLongPtrW, GetWindowRect, GetWindowThreadProcessId, HWND_BOTTOM,
    IsZoomed, LWA_ALPHA, MSG, MsgWaitForMultipleObjects, PM_REMOVE, PeekMessageW, QS_ALLINPUT,
    RegisterClassExW, RegisterWindowMessageW, SEND_MESSAGE_TIMEOUT_FLAGS, SMTO_NORMAL, SW_HIDE,
    SW_SHOWNA, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SendMessageTimeoutW,
    SetLayeredWindowAttributes, SetParent, SetWindowLongPtrW, SetWindowPos, ShowWindow,
    TranslateMessage, WINDOW_EX_STYLE, WINDOW_STYLE, WM_DISPLAYCHANGE, WM_ERASEBKGND,
    WM_POWERBROADCAST, WM_SETTINGCHANGE, WM_WTSSESSION_CHANGE, WNDCLASSEXW, WS_CHILD,
    WS_CLIPCHILDREN, WS_CLIPSIBLINGS, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_NOREDIRECTIONBITMAP,
    WS_EX_TOOLWINDOW, WS_EX_TRANSPARENT, WS_POPUP,
};
use windows::core::{BOOL, PCWSTR, PWSTR, w};
use ww_governor::{MonitorId, MonitorSignals, Signals};

use crate::geometry::{Rect, covered_fraction, monitor_for};
use crate::{DesktopError, DesktopHost, HostEvent, HostWindow, Monitor, Waker};

const WALLPAPER_CLASS: PCWSTR = w!("WallweaverWallpaper");
const MESSAGE_CLASS: PCWSTR = w!("WallweaverHostMessages");
/// Undocumented message that asks Explorer to create the wallpaper WorkerW.
const SPAWN_WORKERW: u32 = 0x052C;

/// Which desktop layout Explorer is using.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layout {
    /// Separate top-level WorkerW behind the icons.
    Classic,
    /// Windows 11 24H2 and later: WorkerW is a child of Progman.
    Raised,
}

/// Decide the layout from Progman's extended window style.
pub fn detect_layout(progman_ex_style: u32) -> Layout {
    if progman_ex_style & WS_EX_NOREDIRECTIONBITMAP.0 != 0 {
        Layout::Raised
    } else {
        Layout::Classic
    }
}

const WM_WAKE: u32 = WM_APP + 1;
const WM_TRAY: u32 = WM_APP + 2;
const TRAY_OPEN: usize = 1;
const TRAY_PAUSE: usize = 2;
const TRAY_QUIT: usize = 3;

thread_local! {
    static TRAY_PAUSED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static EVENTS: std::cell::RefCell<Vec<HostEvent>> = const { std::cell::RefCell::new(Vec::new()) };
    static TASKBAR_CREATED: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

extern "system" fn wallpaper_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if msg == WM_ERASEBKGND {
        // The renderer paints every pixel; skipping the erase avoids a flash.
        return LRESULT(1);
    }
    // SAFETY: forwarding the unmodified message to the default handler.
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

extern "system" fn message_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let event = match msg {
        WM_WAKE => Some(HostEvent::Woken),
        WM_TRAY => {
            let mouse = (lparam.0 as u32) & 0xffff;
            if mouse == WM_RBUTTONUP || mouse == WM_CONTEXTMENU {
                tray_menu(hwnd)
            } else if mouse == WM_LBUTTONDBLCLK {
                Some(HostEvent::TrayOpenManager)
            } else {
                None
            }
        }
        WM_DISPLAYCHANGE | WM_SETTINGCHANGE => Some(HostEvent::MonitorsChanged),
        WM_POWERBROADCAST | WM_WTSSESSION_CHANGE => Some(HostEvent::SignalsChanged),
        m if m != 0 && m == TASKBAR_CREATED.with(|c| c.get()) => Some(HostEvent::ShellRestarted),
        _ => None,
    };
    if let Some(e) = event {
        EVENTS.with(|q| q.borrow_mut().push(e));
    }
    // SAFETY: forwarding the unmodified message to the default handler.
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

/// Show the tray menu and return the chosen action.
fn tray_menu(hwnd: HWND) -> Option<HostEvent> {
    let paused = TRAY_PAUSED.with(|p| p.get());
    // SAFETY: the menu is created, shown and destroyed within this function;
    // all strings are NUL-terminated literals.
    unsafe {
        let menu = CreatePopupMenu().ok()?;
        let _ = AppendMenuW(menu, MF_STRING, TRAY_OPEN, w!("Open Wallweaver"));
        let _ = AppendMenuW(
            menu,
            MF_STRING,
            TRAY_PAUSE,
            if paused {
                w!("Resume wallpapers")
            } else {
                w!("Pause wallpapers")
            },
        );
        let _ = AppendMenuW(menu, MF_STRING, TRAY_QUIT, w!("Quit"));
        let mut pt = POINT::default();
        let _ = GetCursorPos(&mut pt);
        // Required so the menu closes when the user clicks elsewhere.
        let _ = SetForegroundWindow(hwnd);
        let cmd = TrackPopupMenu(
            menu,
            TPM_RETURNCMD | TPM_RIGHTBUTTON,
            pt.x,
            pt.y,
            None,
            hwnd,
            None,
        );
        let _ = DestroyMenu(menu);
        match cmd.0 as usize {
            TRAY_OPEN => Some(HostEvent::TrayOpenManager),
            TRAY_PAUSE => Some(HostEvent::TrayTogglePause),
            TRAY_QUIT => Some(HostEvent::TrayQuit),
            _ => None,
        }
    }
}

/// The Windows desktop host. Must be used from one thread, which also pumps
/// its messages.
#[derive(Debug)]
pub struct WindowsHost {
    instance: windows::Win32::Foundation::HINSTANCE,
    message_window: HWND,
    windows: BTreeMap<MonitorId, (HWND, Rect)>,
    monitor_handles: BTreeMap<MonitorId, isize>,
    tray: bool,
}

impl WindowsHost {
    /// Register window classes and create the hidden message window.
    pub fn new() -> Result<WindowsHost, DesktopError> {
        // SAFETY: GetModuleHandleW(None) returns this executable's handle and
        // has no other effect.
        let module =
            unsafe { GetModuleHandleW(None) }.map_err(|e| DesktopError::Platform(e.to_string()))?;
        let instance = module.into();
        for (class, proc_) in [
            (
                WALLPAPER_CLASS,
                wallpaper_proc as unsafe extern "system" fn(_, _, _, _) -> _,
            ),
            (
                MESSAGE_CLASS,
                message_proc as unsafe extern "system" fn(_, _, _, _) -> _,
            ),
        ] {
            let wc = WNDCLASSEXW {
                cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
                style: CS_HREDRAW | CS_VREDRAW,
                lpfnWndProc: Some(proc_),
                hInstance: instance,
                lpszClassName: class,
                ..Default::default()
            };
            // SAFETY: `wc` is fully initialized and lives for the call; a
            // class that is already registered just returns 0, which is fine.
            unsafe { RegisterClassExW(&wc) };
        }
        // SAFETY: the string is a valid NUL-terminated literal.
        let taskbar = unsafe { RegisterWindowMessageW(w!("TaskbarCreated")) };
        TASKBAR_CREATED.with(|c| c.set(taskbar));
        // A hidden top-level window (not message-only: those don't receive
        // broadcasts such as WM_DISPLAYCHANGE and TaskbarCreated).
        // SAFETY: the class was registered above; all pointers are valid or None.
        let message_window = unsafe {
            CreateWindowExW(
                WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                MESSAGE_CLASS,
                w!("Wallweaver"),
                WS_POPUP,
                0,
                0,
                0,
                0,
                None,
                None,
                Some(instance),
                None,
            )
        }
        .map_err(|e| DesktopError::Platform(format!("creating the message window: {e}")))?;
        Ok(WindowsHost {
            instance,
            message_window,
            windows: BTreeMap::new(),
            monitor_handles: BTreeMap::new(),
            tray: false,
        })
    }

    /// Find the parent our wallpaper windows go into, and the layout.
    fn wallpaper_parent() -> Result<(HWND, Layout, Option<HWND>, Option<HWND>), DesktopError> {
        // SAFETY: plain window lookups with valid literal strings.
        let progman = unsafe { FindWindowW(w!("Progman"), PCWSTR::null()) }.map_err(|_| {
            DesktopError::Platform(
                "the desktop window (Progman) was not found; is Explorer running?".into(),
            )
        })?;
        // Ask Explorer to create the wallpaper WorkerW. Both parameter forms
        // are in use across Windows versions; sending both is harmless.
        for (wp, lp) in [(0usize, 0isize), (0xD, 1)] {
            let mut result = 0usize;
            // SAFETY: progman is a valid window; the result pointer outlives the call.
            unsafe {
                SendMessageTimeoutW(
                    progman,
                    SPAWN_WORKERW,
                    WPARAM(wp),
                    LPARAM(lp),
                    SMTO_NORMAL | SEND_MESSAGE_TIMEOUT_FLAGS(0),
                    1000,
                    Some(&mut result),
                )
            };
        }
        // SAFETY: progman is a valid window handle.
        let ex_style = unsafe { GetWindowLongPtrW(progman, GWL_EXSTYLE) } as u32;
        match detect_layout(ex_style) {
            Layout::Raised => {
                // SAFETY: child lookups under a valid parent.
                let defview = unsafe {
                    FindWindowExW(Some(progman), None, w!("SHELLDLL_DefView"), PCWSTR::null())
                }
                .ok();
                // SAFETY: as above.
                let workerw =
                    unsafe { FindWindowExW(Some(progman), None, w!("WorkerW"), PCWSTR::null()) }
                        .ok();
                Ok((progman, Layout::Raised, defview, workerw))
            }
            Layout::Classic => {
                let mut found = HWND::default();
                // SAFETY: the callback only reads window handles and writes
                // through the pointer we pass, which outlives EnumWindows.
                let _ = unsafe {
                    EnumWindows(Some(find_workerw), LPARAM(&mut found as *mut HWND as isize))
                };
                if found.is_invalid() {
                    return Err(DesktopError::Platform(
                        "Explorer did not create the wallpaper layer (WorkerW)".into(),
                    ));
                }
                Ok((found, Layout::Classic, None, None))
            }
        }
    }
}

/// EnumWindows callback: the wallpaper WorkerW is the top-level WorkerW that
/// follows the window holding SHELLDLL_DefView.
extern "system" fn find_workerw(top: HWND, out: LPARAM) -> BOOL {
    // SAFETY: lookups on a window handle provided by EnumWindows.
    let defview = unsafe { FindWindowExW(Some(top), None, w!("SHELLDLL_DefView"), PCWSTR::null()) };
    if defview.is_ok() {
        // SAFETY: as above; null parent means top-level windows.
        if let Ok(worker) = unsafe { FindWindowExW(None, Some(top), w!("WorkerW"), PCWSTR::null()) }
        {
            // SAFETY: `out` is the &mut HWND passed by wallpaper_parent.
            unsafe { *(out.0 as *mut HWND) = worker };
            return BOOL(0);
        }
    }
    BOOL(1)
}

struct MonitorList(Vec<(HMONITOR, Monitor)>);

extern "system" fn collect_monitor(
    hmon: HMONITOR,
    _hdc: HDC,
    _rect: *mut RECT,
    data: LPARAM,
) -> BOOL {
    // SAFETY: `data` is the &mut MonitorList passed to EnumDisplayMonitors,
    // valid for the duration of the enumeration.
    let list = unsafe { &mut *(data.0 as *mut MonitorList) };
    let mut info = MONITORINFOEXW::default();
    info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
    // SAFETY: info is sized correctly and outlives the call.
    let ok = unsafe { GetMonitorInfoW(hmon, &mut info as *mut MONITORINFOEXW as *mut MONITORINFO) };
    if ok.as_bool() {
        let r = info.monitorInfo.rcMonitor;
        let name_len = info
            .szDevice
            .iter()
            .position(|&c| c == 0)
            .unwrap_or(info.szDevice.len());
        let id = list.0.len() as MonitorId;
        list.0.push((
            hmon,
            Monitor {
                id,
                name: String::from_utf16_lossy(&info.szDevice[..name_len]),
                rect: Rect::from_edges(r.left, r.top, r.right, r.bottom),
                primary: info.monitorInfo.dwFlags & 1 != 0,
            },
        ));
    }
    BOOL(1)
}

fn window_rect(hwnd: HWND) -> Option<Rect> {
    let mut r = RECT::default();
    // SAFETY: r outlives the call; an invalid hwnd just returns an error.
    unsafe { GetWindowRect(hwnd, &mut r) }.ok()?;
    Some(Rect::from_edges(r.left, r.top, r.right, r.bottom))
}

fn class_name(hwnd: HWND) -> String {
    let mut buf = [0u16; 64];
    // SAFETY: the buffer is valid for its full length.
    let n = unsafe { GetClassNameW(hwnd, &mut buf) };
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

fn process_name(hwnd: HWND) -> Option<String> {
    let mut pid = 0u32;
    // SAFETY: pid outlives the call.
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    if pid == 0 {
        return None;
    }
    // SAFETY: limited query access only; the handle is closed below.
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }.ok()?;
    let mut buf = [0u16; 512];
    let mut len = buf.len() as u32;
    // SAFETY: buf and len are valid; len is updated to the written length.
    let res = unsafe {
        QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_WIN32,
            PWSTR(buf.as_mut_ptr()),
            &mut len,
        )
    };
    // SAFETY: handle came from OpenProcess above.
    let _ = unsafe { CloseHandle(handle) };
    res.ok()?;
    let path = String::from_utf16_lossy(&buf[..len as usize]);
    path.rsplit(['\\', '/']).next().map(str::to_string)
}

impl DesktopHost for WindowsHost {
    fn name(&self) -> &'static str {
        "windows"
    }

    fn monitors(&mut self) -> Result<Vec<Monitor>, DesktopError> {
        let mut list = MonitorList(Vec::new());
        // SAFETY: the callback writes only into `list`, which outlives the call.
        let ok = unsafe {
            EnumDisplayMonitors(
                None,
                None,
                Some(collect_monitor),
                LPARAM(&mut list as *mut MonitorList as isize),
            )
        };
        if !ok.as_bool() && list.0.is_empty() {
            return Err(DesktopError::Platform("could not list monitors".into()));
        }
        self.monitor_handles = list.0.iter().map(|(h, m)| (m.id, h.0 as isize)).collect();
        Ok(list.0.into_iter().map(|(_, m)| m).collect())
    }

    fn create_window(&mut self, monitor: &Monitor) -> Result<HostWindow, DesktopError> {
        self.destroy_window(monitor.id);
        let (parent, layout, defview, workerw) = Self::wallpaper_parent()?;
        let ex_style = match layout {
            // Layered must be set before SetParent for the new layout.
            Layout::Raised => WS_EX_LAYERED | WS_EX_NOACTIVATE | WS_EX_TRANSPARENT,
            Layout::Classic => WS_EX_NOACTIVATE | WS_EX_TRANSPARENT,
        };
        let r = monitor.rect;
        // SAFETY: the class is registered; the window starts as a hidden popup
        // and is reparented below.
        let hwnd = unsafe {
            CreateWindowExW(
                ex_style | WINDOW_EX_STYLE(0),
                WALLPAPER_CLASS,
                w!("Wallweaver wallpaper"),
                WS_POPUP | WS_CLIPSIBLINGS | WS_CLIPCHILDREN,
                r.x,
                r.y,
                r.width as i32,
                r.height as i32,
                None,
                None,
                Some(self.instance),
                None,
            )
        }
        .map_err(|e| DesktopError::Platform(format!("creating the wallpaper window: {e}")))?;
        if layout == Layout::Raised {
            // SAFETY: hwnd was just created and is layered.
            unsafe { SetLayeredWindowAttributes(hwnd, Default::default(), 255, LWA_ALPHA) }
                .map_err(|e| DesktopError::Platform(e.to_string()))?;
        }
        // Become a child window of the wallpaper layer.
        // SAFETY: hwnd is our window; reading and writing its style is safe.
        unsafe {
            let style = GetWindowLongPtrW(hwnd, GWL_STYLE) as u32;
            let style = (style & !WS_POPUP.0) | WS_CHILD.0;
            SetWindowLongPtrW(hwnd, GWL_STYLE, WINDOW_STYLE(style).0 as isize);
        }
        // SAFETY: both handles are valid windows.
        unsafe { SetParent(hwnd, Some(parent)) }
            .map_err(|e| DesktopError::Platform(format!("attaching to the desktop: {e}")))?;
        // Child coordinates are relative to the parent's client area, whose
        // origin is the top-left of the virtual screen.
        let mut pts = [POINT { x: r.x, y: r.y }];
        // SAFETY: pts is a valid slice for the duration of the call.
        unsafe { MapWindowPoints(None, Some(parent), &mut pts) };
        let insert_after = match (layout, defview) {
            // Directly below the icon layer.
            (Layout::Raised, Some(dv)) => Some(dv),
            _ => Some(HWND_BOTTOM),
        };
        // SAFETY: valid handles; flags keep focus where it is.
        unsafe {
            SetWindowPos(
                hwnd,
                insert_after,
                pts[0].x,
                pts[0].y,
                r.width as i32,
                r.height as i32,
                SWP_NOACTIVATE,
            )
        }
        .map_err(|e| DesktopError::Platform(e.to_string()))?;
        if let (Layout::Raised, Some(w)) = (layout, workerw) {
            // Keep Explorer's own wallpaper WorkerW below ours.
            // SAFETY: valid handles; size and position unchanged.
            let _ = unsafe {
                SetWindowPos(
                    w,
                    Some(hwnd),
                    0,
                    0,
                    0,
                    0,
                    SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE,
                )
            };
        }
        self.windows.insert(monitor.id, (hwnd, r));
        let raw = NonZeroIsize::new(hwnd.0 as isize)
            .ok_or_else(|| DesktopError::Platform("null window".into()))?;
        let mut handle = Win32WindowHandle::new(raw);
        handle.hinstance = NonZeroIsize::new(self.instance.0 as isize);
        Ok(HostWindow {
            monitor: monitor.id,
            display: RawDisplayHandle::Windows(WindowsDisplayHandle::new()),
            window: RawWindowHandle::Win32(handle),
            width: r.width,
            height: r.height,
        })
    }

    fn destroy_window(&mut self, monitor: MonitorId) {
        if let Some((hwnd, _)) = self.windows.remove(&monitor) {
            // SAFETY: the window was created by this host on this thread.
            let _ = unsafe { DestroyWindow(hwnd) };
        }
    }

    fn show(&mut self, monitor: MonitorId, visible: bool) {
        if let Some((hwnd, _)) = self.windows.get(&monitor) {
            // SAFETY: valid window handle owned by this host.
            let _ = unsafe { ShowWindow(*hwnd, if visible { SW_SHOWNA } else { SW_HIDE }) };
        }
    }

    fn signals(&mut self, monitors: &[Monitor]) -> Signals {
        let mut s = Signals::default();
        for m in monitors {
            s.monitors.insert(m.id, MonitorSignals::default());
        }

        let mut power = SYSTEM_POWER_STATUS::default();
        // SAFETY: power outlives the call.
        if unsafe { GetSystemPowerStatus(&mut power) }.is_ok() {
            s.on_battery = power.ACLineStatus == 0;
            s.battery_percent =
                (power.BatteryLifePercent <= 100).then_some(power.BatteryLifePercent);
            s.battery_saver = power.SystemStatusFlag == 1;
        }

        // A locked workstation has no input desktop we can open.
        // SAFETY: the returned handle is closed right away.
        match unsafe { OpenInputDesktop(DESKTOP_CONTROL_FLAGS(0), false, DESKTOP_SWITCHDESKTOP) } {
            Ok(desk) => {
                // SAFETY: desk came from OpenInputDesktop.
                let _ = unsafe { CloseDesktop(desk) };
            }
            Err(_) => s.session_locked = true,
        }

        // SAFETY: no arguments; returns a possibly null handle.
        let fg = unsafe { GetForegroundWindow() };
        if fg.is_invalid() {
            return s;
        }
        let class = class_name(fg);
        if matches!(
            class.as_str(),
            "Progman" | "WorkerW" | "Shell_TrayWnd" | "Shell_SecondaryTrayWnd"
        ) {
            // The desktop itself is in front: nothing covers the wallpaper.
            return s;
        }
        let Some(rect) = window_rect(fg) else {
            return s;
        };
        let rects: Vec<(u32, &Rect)> = monitors.iter().map(|m| (m.id, &m.rect)).collect();
        // SAFETY: fg is a window handle; NULL is returned if off-screen.
        let hmon = unsafe { MonitorFromWindow(fg, MONITOR_DEFAULTTONULL) };
        let on = self
            .monitor_handles
            .iter()
            .find(|(_, h)| **h == hmon.0 as isize)
            .map(|(id, _)| *id)
            .or_else(|| monitor_for(&rect, rects.iter().copied()));
        let Some(mon_id) = on else { return s };
        let Some(mon) = monitors.iter().find(|m| m.id == mon_id) else {
            return s;
        };
        // SAFETY: no arguments.
        let quns = unsafe { SHQueryUserNotificationState() }.ok();
        let os_says_fullscreen = matches!(
            quns,
            Some(QUNS_BUSY) | Some(QUNS_RUNNING_D3D_FULL_SCREEN) | Some(QUNS_PRESENTATION_MODE)
        );
        // SAFETY: fg is a window handle.
        let maximized = unsafe { IsZoomed(fg) }.as_bool();
        let entry = s.monitors.entry(mon_id).or_default();
        entry.fullscreen_app = os_says_fullscreen || (rect.covers(&mon.rect, 0) && !maximized);
        entry.covered_fraction = if maximized {
            1.0
        } else {
            covered_fraction(&rect, &mon.rect)
        };
        entry.foreground_app = process_name(fg);
        s
    }

    fn cursor_position(&mut self) -> Option<(i32, i32)> {
        let mut p = POINT::default();
        // SAFETY: p outlives the call.
        unsafe { GetCursorPos(&mut p) }.ok()?;
        Some((p.x, p.y))
    }

    fn waker(&self) -> Waker {
        let hwnd = self.message_window.0 as isize;
        Waker::new(move || {
            // SAFETY: PostMessageW is thread-safe; if the window is gone the
            // call fails harmlessly.
            let _ =
                unsafe { PostMessageW(Some(HWND(hwnd as *mut _)), WM_WAKE, WPARAM(0), LPARAM(0)) };
        })
    }

    fn set_tray(&mut self, paused: bool) {
        TRAY_PAUSED.with(|p| p.set(paused));
        let mut data = NOTIFYICONDATAW {
            cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: self.message_window,
            uID: 1,
            uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP,
            uCallbackMessage: WM_TRAY,
            ..Default::default()
        };
        // SAFETY: loading a stock icon has no preconditions.
        if let Ok(icon) = unsafe { LoadIconW(None, IDI_APPLICATION) } {
            data.hIcon = icon;
        }
        let tip: Vec<u16> = if paused {
            "Wallweaver (paused)"
        } else {
            "Wallweaver"
        }
        .encode_utf16()
        .collect();
        data.szTip[..tip.len()].copy_from_slice(&tip);
        let op = if self.tray { NIM_MODIFY } else { NIM_ADD };
        // SAFETY: data is fully initialized and outlives the call.
        if unsafe { Shell_NotifyIconW(op, &data) }.as_bool() {
            self.tray = true;
        }
    }

    fn pump(&mut self, timeout: Duration) -> Vec<HostEvent> {
        let deadline = Instant::now() + timeout;
        loop {
            let mut msg = MSG::default();
            // SAFETY: msg outlives each call; messages are dispatched to our
            // own window procedures on this thread.
            while unsafe { PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE) }.as_bool() {
                // SAFETY: msg was filled by PeekMessageW.
                unsafe {
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            }
            let events = EVENTS.with(|q| std::mem::take(&mut *q.borrow_mut()));
            let now = Instant::now();
            if !events.is_empty() || now >= deadline {
                return events;
            }
            let wait = (deadline - now).as_millis().min(u128::from(u32::MAX - 1)) as u32;
            // Sleep until a message arrives or the timeout passes.
            // SAFETY: no handles are passed; this only waits on the queue.
            unsafe { MsgWaitForMultipleObjects(None, false, wait, QS_ALLINPUT) };
        }
    }
}

impl Drop for WindowsHost {
    fn drop(&mut self) {
        let ids: Vec<_> = self.windows.keys().copied().collect();
        for id in ids {
            self.destroy_window(id);
        }
        if self.tray {
            let data = NOTIFYICONDATAW {
                cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
                hWnd: self.message_window,
                uID: 1,
                ..Default::default()
            };
            // SAFETY: removes the icon this host added.
            let _ = unsafe { Shell_NotifyIconW(NIM_DELETE, &data) };
        }
        // SAFETY: created by this host.
        let _ = unsafe { DestroyWindow(self.message_window) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_is_detected_from_the_progman_style() {
        assert_eq!(detect_layout(0), Layout::Classic);
        assert_eq!(detect_layout(WS_EX_NOREDIRECTIONBITMAP.0), Layout::Raised);
        assert_eq!(
            detect_layout(WS_EX_NOREDIRECTIONBITMAP.0 | 0x8),
            Layout::Raised
        );
    }
}
