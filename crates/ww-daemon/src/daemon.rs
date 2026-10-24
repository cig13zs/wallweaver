//! The daemon's main thread: desktop host, governor and wallpapers.
//!
//! Everything here runs on one thread. It sleeps inside the desktop host's
//! message wait and wakes for window-system events, IPC requests (through
//! the host's waker), finished loads, the governor's next deadline and, only
//! while a wallpaper is set, a once-a-second check of fullscreen, battery and
//! lock state. With no wallpaper set it doesn't wake at all.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, Instant};

use ww_desktop::{DesktopHost, HostEvent, HostWindow, Monitor, Waker, process};
use ww_governor::{Governor, MonitorId, Signals, State};
use ww_ipc::{
    ConsentLine, DaemonStatus, ErrorCode, Event, MonitorInfo, MonitorStatus, MonitorTarget,
    PROTOCOL_VERSION, RemoteError, Request, Response, WallpaperInfo, WallpaperSource,
};
use ww_manifest::WallpaperType;
use ww_permissions::{Capability, Effective, consent_items, declared};
use ww_render::{Content, GpuOptions, LoadOptions, RenderLoop, TargetSpec, WindowHandles};

use crate::server::{ConnId, EventSink, Msg};
use crate::store::{Resolved, Store, resolve};

/// How often host signals are read while a wallpaper is set.
const SIGNAL_INTERVAL: Duration = Duration::from_secs(1);
/// Pointer polling interval for shaders with mouse access.
const MOUSE_INTERVAL: Duration = Duration::from_millis(33);

/// Startup options.
#[derive(Debug, Clone)]
pub struct Options {
    /// Render offscreen at this size instead of on the desktop.
    pub headless: Option<(u32, u32)>,
    /// Settings and package folder.
    pub data_dir: PathBuf,
    /// IPC endpoint override.
    pub endpoint: Option<ww_ipc::Endpoint>,
    /// Wallpaper to show at startup on every monitor.
    pub set: Option<PathBuf>,
    /// Exit after this long (benchmarks and tests).
    pub exit_after: Option<Duration>,
    /// Graphics adapter options.
    pub gpu: GpuOptions,
}

/// A finished load.
pub struct LoadResult {
    monitor: MonitorId,
    generation: u64,
    result: Result<Content, String>,
}

type Reply = Sender<Result<Response, RemoteError>>;

/// One reply shared by the loads of a multi-monitor request.
struct Pending {
    remaining: usize,
    reply: Option<Reply>,
    error: Option<RemoteError>,
}

impl Pending {
    fn finish(&mut self, err: Option<RemoteError>) {
        if err.is_some() && self.error.is_none() {
            self.error = err;
        }
        self.remaining = self.remaining.saturating_sub(1);
        if self.remaining == 0
            && let Some(reply) = self.reply.take()
        {
            let _ = reply.send(match self.error.take() {
                Some(e) => Err(e),
                None => Ok(Response::Ok),
            });
        }
    }
}

struct Slot {
    resolved: Resolved,
    generation: u64,
    attached: bool,
    window: Option<HostWindow>,
    pending: Option<Rc<RefCell<Pending>>>,
    mouse: bool,
}

/// The daemon.
pub struct Daemon {
    opts: Options,
    host: Box<dyn DesktopHost>,
    waker: Waker,
    renderer: RenderLoop,
    governor: Governor,
    store: Store,
    monitors: Vec<Monitor>,
    slots: BTreeMap<MonitorId, Slot>,
    subscribers: Vec<(ConnId, EventSink)>,
    rx: Receiver<Msg>,
    tx: Sender<Msg>,
    start: Instant,
    user_paused: bool,
    user_stopped: bool,
    last_signals: Signals,
    next_signal_poll: Instant,
    next_mouse_poll: Instant,
    generation: u64,
    quit: bool,
}

fn remote(code: ErrorCode, msg: impl Into<String>) -> RemoteError {
    RemoteError::new(code, msg)
}

impl Daemon {
    /// Set up the daemon. Nothing is drawn and no GPU is opened yet.
    pub fn new(opts: Options, host: Box<dyn DesktopHost>) -> Result<Daemon, String> {
        let store = Store::load(&opts.data_dir);
        let governor = Governor::new(store.policy.clone());
        let waker = host.waker();
        let (tx, rx) = channel();
        let renderer = RenderLoop::spawn(opts.gpu.clone());
        let mut d = Daemon {
            opts,
            host,
            waker,
            renderer,
            governor,
            store,
            monitors: Vec::new(),
            slots: BTreeMap::new(),
            subscribers: Vec::new(),
            rx,
            tx,
            start: Instant::now(),
            user_paused: false,
            user_stopped: false,
            last_signals: Signals::default(),
            next_signal_poll: Instant::now(),
            next_mouse_poll: Instant::now(),
            generation: 0,
            quit: false,
        };
        d.monitors = d.host.monitors().map_err(|e| e.to_string())?;
        Ok(d)
    }

    /// Channel and waker for the IPC threads.
    pub fn ipc_handles(&self) -> (Sender<Msg>, Waker) {
        (self.tx.clone(), self.waker.clone())
    }

    /// Run until asked to quit.
    pub fn run(&mut self) {
        self.host.set_tray(false);
        if let Some(path) = self.opts.set.clone() {
            let source = if path.is_dir() {
                WallpaperSource::Folder {
                    path: path.to_string_lossy().into_owned(),
                }
            } else {
                WallpaperSource::File {
                    path: path.to_string_lossy().into_owned(),
                }
            };
            if let Err(e) = self.set_wallpaper(MonitorTarget::All, source, None) {
                crate::log(&format!("could not set {}: {}", path.display(), e.message));
            }
        }
        let deadline = self.opts.exit_after.map(|d| self.start + d);
        while !self.quit {
            let timeout = self.next_timeout(deadline);
            let events = self.host.pump(timeout);
            for e in events {
                self.on_host_event(e);
            }
            while let Ok(msg) = self.rx.try_recv() {
                self.on_msg(msg);
            }
            self.tick(false);
            if deadline.is_some_and(|d| Instant::now() >= d) {
                break;
            }
        }
        // Release surfaces before their windows.
        let ids: Vec<_> = self.slots.keys().copied().collect();
        for id in ids {
            self.renderer.detach(id);
        }
        let _ = self.renderer.stats();
        for id in self.slots.keys().copied().collect::<Vec<_>>() {
            self.host.destroy_window(id);
        }
    }

    fn next_timeout(&self, exit: Option<Instant>) -> Duration {
        let now = Instant::now();
        let mut next = now + Duration::from_secs(3600);
        if !self.slots.is_empty() {
            next = next.min(self.next_signal_poll);
        }
        if let Some(d) = self.governor.next_deadline() {
            next = next.min(self.start + d);
        }
        if self.mouse_active() {
            next = next.min(self.next_mouse_poll);
        }
        if let Some(e) = exit {
            next = next.min(e);
        }
        next.saturating_duration_since(now)
    }

    fn mouse_active(&self) -> bool {
        self.slots.iter().any(|(m, s)| {
            s.mouse && s.attached && self.governor.state(*m).is_some_and(State::draws)
        })
    }

    fn on_host_event(&mut self, e: HostEvent) {
        match e {
            HostEvent::Woken => {}
            HostEvent::SignalsChanged => self.tick(true),
            HostEvent::MonitorsChanged => self.monitors_changed(),
            HostEvent::ShellRestarted => {
                crate::log("the desktop shell restarted; reattaching wallpapers");
                let ids: Vec<_> = self.slots.keys().copied().collect();
                for id in ids {
                    self.detach(id);
                    self.start_load(id);
                }
            }
            HostEvent::TrayOpenManager => open_manager(),
            HostEvent::TrayTogglePause => {
                self.user_paused = !self.user_paused;
                self.user_stopped = false;
                self.host.set_tray(self.user_paused);
                self.tick(true);
            }
            HostEvent::TrayQuit => self.quit = true,
        }
    }

    fn monitors_changed(&mut self) {
        let Ok(new) = self.host.monitors() else {
            return;
        };
        let old = std::mem::replace(&mut self.monitors, new);
        let ids: Vec<_> = self.slots.keys().copied().collect();
        for id in ids {
            let before = old.iter().find(|m| m.id == id).map(|m| m.rect);
            let after = self.monitors.iter().find(|m| m.id == id).map(|m| m.rect);
            match after {
                None => {
                    self.detach(id);
                    self.slots.remove(&id);
                    self.broadcast(Event::WallpaperChanged {
                        monitor: id,
                        wallpaper: None,
                    });
                }
                Some(r) if Some(r) != before => {
                    // New size or position: rebuild at the new resolution.
                    self.detach(id);
                    self.start_load(id);
                }
                Some(_) => {}
            }
        }
        let monitors = self.monitor_infos();
        self.broadcast(Event::MonitorsChanged { monitors });
        self.tick(true);
    }

    fn on_msg(&mut self, msg: Msg) {
        match msg {
            Msg::Request { request, reply, .. } => {
                if let Some(result) = self.on_request(request, &reply) {
                    let _ = reply.send(result);
                }
            }
            Msg::Subscribe { conn, sink } => self.subscribers.push((conn, sink)),
            Msg::Disconnected(conn) => self.subscribers.retain(|(c, _)| *c != conn),
            Msg::Loaded(r) => self.on_loaded(r),
        }
    }

    /// Handle a request. `None` means the answer is sent later.
    fn on_request(
        &mut self,
        request: Request,
        reply: &Reply,
    ) -> Option<Result<Response, RemoteError>> {
        Some(match request {
            Request::Status => Ok(Response::Status(self.status())),
            Request::ListMonitors => Ok(Response::Monitors {
                monitors: self.monitor_infos(),
            }),
            Request::SetWallpaper { monitor, source } => {
                return match self.set_wallpaper(monitor, source, Some(reply.clone())) {
                    Ok(()) => None,
                    Err(e) => Some(Err(e)),
                };
            }
            Request::ClearWallpaper { monitor } => {
                for id in self.targets(monitor) {
                    self.detach(id);
                    self.host.destroy_window(id);
                    if self.slots.remove(&id).is_some() {
                        self.broadcast(Event::WallpaperChanged {
                            monitor: id,
                            wallpaper: None,
                        });
                    }
                }
                self.tick(true);
                Ok(Response::Ok)
            }
            Request::PauseAll => {
                self.user_paused = true;
                self.host.set_tray(true);
                self.tick(true);
                Ok(Response::Ok)
            }
            Request::ResumeAll => {
                self.user_paused = false;
                self.user_stopped = false;
                self.host.set_tray(false);
                self.tick(true);
                Ok(Response::Ok)
            }
            Request::StopAll => {
                self.user_stopped = true;
                self.host.set_tray(true);
                self.tick(true);
                Ok(Response::Ok)
            }
            Request::GetPolicy => Ok(Response::Policy {
                policy: self.governor.policy().clone(),
            }),
            Request::SetPolicy { policy } => {
                self.governor.set_policy(policy);
                self.store.policy = self.governor.policy().clone();
                if let Err(e) = self.store.save() {
                    crate::log(&format!("could not save settings: {e}"));
                }
                self.tick(true);
                Ok(Response::Policy {
                    policy: self.governor.policy().clone(),
                })
            }
            Request::SetProperties { .. } => Err(remote(
                ErrorCode::Unsupported,
                "native wallpapers have no adjustable properties yet",
            )),
            Request::GetConsent { source } => self.consent(&source),
            Request::GrantPermissions {
                package_id,
                capabilities,
            } => self.change_grants(&package_id, &capabilities, true),
            Request::RevokePermissions {
                package_id,
                capabilities,
            } => self.change_grants(&package_id, &capabilities, false),
            Request::Subscribe => Ok(Response::Ok),
            Request::Shutdown => {
                self.quit = true;
                Ok(Response::Ok)
            }
        })
    }

    fn consent(&self, source: &WallpaperSource) -> Result<Response, RemoteError> {
        let r = resolve(&self.opts.data_dir, source)
            .map_err(|e| remote(ErrorCode::InvalidWallpaper, e))?;
        let caps = r.manifest.as_ref().map(declared).unwrap_or_default();
        Ok(Response::Consent {
            package_id: r.id,
            items: consent_items(&caps)
                .into_iter()
                .map(ConsentLine::from)
                .collect(),
        })
    }

    fn targets(&self, t: MonitorTarget) -> Vec<MonitorId> {
        match t {
            MonitorTarget::All => self.monitors.iter().map(|m| m.id).collect(),
            MonitorTarget::Id(id) => self
                .monitors
                .iter()
                .filter(|m| m.id == id)
                .map(|m| m.id)
                .collect(),
        }
    }

    fn set_wallpaper(
        &mut self,
        target: MonitorTarget,
        source: WallpaperSource,
        reply: Option<Reply>,
    ) -> Result<(), RemoteError> {
        let resolved = resolve(&self.opts.data_dir, &source)
            .map_err(|e| remote(ErrorCode::InvalidWallpaper, e))?;
        match resolved.kind {
            WallpaperType::Image
            | WallpaperType::Animated
            | WallpaperType::Slideshow
            | WallpaperType::Shader => {}
            WallpaperType::Video => {
                return Err(remote(
                    ErrorCode::Unsupported,
                    "video wallpapers are not in this build yet",
                ));
            }
            WallpaperType::Web => {
                return Err(remote(
                    ErrorCode::Unsupported,
                    "web wallpapers need the web view host, which is not in this build yet",
                ));
            }
            WallpaperType::Theme => {
                return Err(remote(ErrorCode::BadRequest, "a theme is not a wallpaper"));
            }
        }
        let ids = self.targets(target);
        if ids.is_empty() {
            return Err(remote(ErrorCode::BadRequest, "no such monitor"));
        }
        let pending = Rc::new(RefCell::new(Pending {
            remaining: ids.len(),
            reply,
            error: None,
        }));
        let mouse = self.mouse_allowed(&resolved);
        for id in ids {
            self.detach(id);
            self.slots.insert(
                id,
                Slot {
                    resolved: resolved.clone(),
                    generation: 0,
                    attached: false,
                    window: None,
                    pending: Some(pending.clone()),
                    mouse,
                },
            );
            self.start_load(id);
        }
        Ok(())
    }

    fn mouse_allowed(&self, r: &Resolved) -> bool {
        let Some(m) = &r.manifest else { return false };
        Effective::resolve(m, r.trust, r.signer.as_deref(), self.store.record(r))
            .allows(&Capability::InputMouse)
    }

    /// Decode on a loader thread; the result comes back as `Msg::Loaded`.
    fn start_load(&mut self, monitor: MonitorId) {
        let Some(mon) = self.monitors.iter().find(|m| m.id == monitor).cloned() else {
            return;
        };
        let size = match self.opts.headless {
            Some(s) => s,
            None => (mon.rect.width, mon.rect.height),
        };
        self.generation += 1;
        let generation = self.generation;
        let Some(slot) = self.slots.get_mut(&monitor) else {
            return;
        };
        slot.generation = generation;
        let resolved = slot.resolved.clone();
        let tx = self.tx.clone();
        let waker = self.waker.clone();
        let _ = std::thread::Builder::new()
            .name("ww-load".into())
            .spawn(move || {
                let opts = LoadOptions::for_size(size.0, size.1);
                let result = match (&resolved.file, &resolved.manifest, &resolved.root) {
                    (Some(file), _, _) => ww_render::load_file(file, &opts),
                    (None, Some(m), Some(root)) => ww_render::load_package(m, root, &opts),
                    _ => Err(ww_render::RenderError::Unsupported(
                        "nothing to load".into(),
                    )),
                }
                .map_err(|e| e.to_string());
                let _ = tx.send(Msg::Loaded(LoadResult {
                    monitor,
                    generation,
                    result,
                }));
                waker.wake();
            });
    }

    fn on_loaded(&mut self, r: LoadResult) {
        let Some(slot) = self.slots.get(&r.monitor) else {
            return;
        };
        if slot.generation != r.generation {
            return; // superseded by a newer request
        }
        let content = match r.result {
            Ok(c) => c,
            Err(e) => {
                crate::log(&format!("monitor {}: {e}", r.monitor));
                if let Some(slot) = self.slots.remove(&r.monitor)
                    && let Some(p) = slot.pending
                {
                    p.borrow_mut()
                        .finish(Some(remote(ErrorCode::InvalidWallpaper, e)));
                }
                return;
            }
        };
        let attach = self.attach(r.monitor).and_then(|()| {
            self.renderer
                .load(r.monitor, content)
                .map_err(|e| remote(ErrorCode::Platform, e.to_string()))
        });
        let Some(slot) = self.slots.get_mut(&r.monitor) else {
            return;
        };
        let pending = slot.pending.take();
        match attach {
            Ok(()) => {
                self.host.show(r.monitor, true);
                let info = wallpaper_info(&slot.resolved);
                if let Some(p) = pending {
                    p.borrow_mut().finish(None);
                }
                self.broadcast(Event::WallpaperChanged {
                    monitor: r.monitor,
                    wallpaper: Some(info),
                });
                self.tick(true);
            }
            Err(e) => {
                crate::log(&format!("monitor {}: {}", r.monitor, e.message));
                self.detach(r.monitor);
                self.slots.remove(&r.monitor);
                if let Some(p) = pending {
                    p.borrow_mut().finish(Some(e));
                }
            }
        }
    }

    fn attach(&mut self, monitor: MonitorId) -> Result<(), RemoteError> {
        let fps = self
            .governor
            .state(monitor)
            .map(State::fps)
            .filter(|f| *f > 0)
            .unwrap_or(self.governor.policy().max_fps);
        let spec = match self.opts.headless {
            Some((width, height)) => TargetSpec::Offscreen { width, height },
            None => {
                let mon = self
                    .monitors
                    .iter()
                    .find(|m| m.id == monitor)
                    .cloned()
                    .ok_or_else(|| remote(ErrorCode::BadRequest, "no such monitor"))?;
                let window = match self.slots.get(&monitor).and_then(|s| s.window) {
                    Some(w) => w,
                    None => self
                        .host
                        .create_window(&mon)
                        .map_err(|e| remote(ErrorCode::Platform, e.to_string()))?,
                };
                if let Some(s) = self.slots.get_mut(&monitor) {
                    s.window = Some(window);
                }
                TargetSpec::Window {
                    handles: WindowHandles {
                        display: window.display,
                        window: window.window,
                    },
                    width: window.width,
                    height: window.height,
                }
            }
        };
        self.renderer
            .attach(monitor, spec, fps)
            .map_err(|e| remote(ErrorCode::Platform, e.to_string()))?;
        if let Some(s) = self.slots.get_mut(&monitor) {
            s.attached = true;
        }
        Ok(())
    }

    /// Release the renderer's resources for a monitor and hide its window.
    fn detach(&mut self, monitor: MonitorId) {
        if let Some(s) = self.slots.get_mut(&monitor)
            && s.attached
        {
            self.renderer.detach(monitor);
            s.attached = false;
            // Make sure the surface is gone before the window can be touched.
            let _ = self.renderer.stats();
            self.host.show(monitor, false);
        }
    }

    /// Read signals (when due or forced), update the governor and apply
    /// what it decides.
    fn tick(&mut self, force: bool) {
        let now_i = Instant::now();
        let now = now_i.duration_since(self.start);
        let mut changed = false;
        if force || (!self.slots.is_empty() && now_i >= self.next_signal_poll) {
            let mons: Vec<Monitor> = self
                .monitors
                .iter()
                .filter(|m| self.slots.contains_key(&m.id))
                .cloned()
                .collect();
            let mut s = self.host.signals(&mons);
            s.monitors.retain(|id, _| self.slots.contains_key(id));
            s.user_paused = self.user_paused;
            s.user_stopped = self.user_stopped;
            self.last_signals = s;
            self.next_signal_poll = now_i + SIGNAL_INTERVAL;
            changed = true;
        }
        let deadline_due = self.governor.next_deadline().is_some_and(|d| d <= now);
        if changed || deadline_due {
            let signals = self.last_signals.clone();
            let transitions = self.governor.update(now, &signals);
            for t in transitions {
                self.apply(t.monitor, t.to);
                self.broadcast(Event::StateChanged {
                    monitor: t.monitor,
                    state: t.to,
                    reason: t.reason,
                });
            }
        }
        if self.mouse_active() && now_i >= self.next_mouse_poll {
            self.next_mouse_poll = now_i + MOUSE_INTERVAL;
            if let Some((x, y)) = self.host.cursor_position() {
                for (id, slot) in &self.slots {
                    if !slot.mouse {
                        continue;
                    }
                    if let Some(m) = self.monitors.iter().find(|m| m.id == *id) {
                        let r = m.rect;
                        let inside = x >= r.x
                            && y >= r.y
                            && x < r.x + r.width as i32
                            && y < r.y + r.height as i32;
                        if inside {
                            // Shadertoy's iMouse has its origin at the bottom left.
                            let px = (x - r.x) as f32;
                            let py = (r.height as i32 - (y - r.y)) as f32;
                            self.renderer.set_mouse(*id, Some([px, py, 0.0, 0.0]));
                        }
                    }
                }
            }
        }
    }

    fn apply(&mut self, monitor: MonitorId, state: State) {
        let Some(slot) = self.slots.get(&monitor) else {
            return;
        };
        match state {
            State::Running { fps } | State::Throttled { fps } => {
                if slot.attached {
                    self.renderer.set_fps(monitor, fps);
                } else if slot.pending.is_none() {
                    // Coming back from Stopped: rebuild from disk.
                    self.start_load(monitor);
                }
            }
            State::Paused => {
                if slot.attached {
                    self.renderer.set_fps(monitor, 0);
                }
            }
            State::Stopped => self.detach(monitor),
        }
    }

    fn change_grants(
        &mut self,
        package_id: &str,
        caps: &[Capability],
        grant: bool,
    ) -> Result<Response, RemoteError> {
        let resolved = self
            .slots
            .values()
            .map(|s| s.resolved.clone())
            .find(|r| r.id == package_id)
            .or_else(|| {
                resolve(
                    &self.opts.data_dir,
                    &WallpaperSource::Installed {
                        id: package_id.to_string(),
                    },
                )
                .ok()
            })
            .ok_or_else(|| {
                remote(
                    ErrorCode::BadRequest,
                    format!("no wallpaper with id '{package_id}' is set or installed"),
                )
            })?;
        let manifest = resolved
            .manifest
            .clone()
            .ok_or_else(|| remote(ErrorCode::BadRequest, "this wallpaper has no manifest"))?;
        let record = self.store.record_mut(&resolved);
        for cap in caps {
            if grant {
                record
                    .grant(&manifest, cap.clone())
                    .map_err(|e| remote(ErrorCode::PermissionDenied, e.to_string()))?;
            } else {
                record.revoke(cap);
            }
        }
        if let Err(e) = self.store.save() {
            crate::log(&format!("could not save permissions: {e}"));
        }
        let ids: Vec<_> = self.slots.keys().copied().collect();
        for id in ids {
            let allowed = self
                .slots
                .get(&id)
                .filter(|s| s.resolved.id == package_id)
                .map(|s| self.mouse_allowed(&s.resolved));
            if let (Some(allowed), Some(s)) = (allowed, self.slots.get_mut(&id)) {
                s.mouse = allowed;
                if !allowed {
                    self.renderer.set_mouse(id, None);
                }
            }
        }
        Ok(Response::Ok)
    }

    fn monitor_infos(&self) -> Vec<MonitorInfo> {
        self.monitors.iter().map(monitor_info).collect()
    }

    fn status(&self) -> DaemonStatus {
        let stats = self.renderer.stats();
        DaemonStatus {
            version: crate::VERSION.into(),
            protocol: PROTOCOL_VERSION,
            uptime_ms: self.start.elapsed().as_millis() as u64,
            rss_bytes: process::rss_bytes(),
            adapter: stats.and_then(|s| s.adapter).map(|a| a.to_string()),
            monitors: self
                .monitors
                .iter()
                .map(|m| {
                    let slot = self.slots.get(&m.id);
                    let state = self.governor.states().find(|(id, _, _)| *id == m.id);
                    MonitorStatus {
                        monitor: monitor_info(m),
                        wallpaper: slot.map(|s| wallpaper_info(&s.resolved)),
                        state: state.map(|(_, s, _)| s),
                        reason: state.map(|(_, _, r)| r.clone()),
                    }
                })
                .collect(),
        }
    }

    fn broadcast(&mut self, event: Event) {
        self.subscribers.retain(|(_, sink)| sink.send(&event));
    }
}

fn monitor_info(m: &Monitor) -> MonitorInfo {
    MonitorInfo {
        id: m.id,
        name: m.name.clone(),
        x: m.rect.x,
        y: m.rect.y,
        width: m.rect.width,
        height: m.rect.height,
        primary: m.primary,
    }
}

fn wallpaper_info(r: &Resolved) -> WallpaperInfo {
    WallpaperInfo {
        id: r.id.clone(),
        name: r.name.clone(),
        kind: r.kind.as_str().into(),
        source: r.source.clone(),
    }
}

/// Start the manager window: `WW_MANAGER` if set, otherwise a
/// `wallweaver-manager` executable next to the daemon.
fn open_manager() {
    let exe = std::env::var_os("WW_MANAGER")
        .map(PathBuf::from)
        .or_else(|| {
            let me = std::env::current_exe().ok()?;
            let name = if cfg!(windows) {
                "wallweaver-manager.exe"
            } else {
                "wallweaver-manager"
            };
            Some(me.with_file_name(name))
        });
    match exe {
        Some(p) if p.exists() => {
            if let Err(e) = std::process::Command::new(&p).spawn() {
                crate::log(&format!("could not start {}: {e}", p.display()));
            }
        }
        _ => crate::log("the manager app is not installed next to the daemon"),
    }
}
