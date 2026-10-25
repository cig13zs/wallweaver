//! The render thread.
//!
//! One thread draws every monitor. It sleeps until the earliest frame is
//! due or a command arrives. When nothing is due (still images, everything
//! paused) it blocks on the command channel and uses no CPU at all. The GPU
//! device is opened when the first target is attached and closed when the
//! last one is detached, so a daemon with every wallpaper stopped holds no
//! GPU memory.

use std::collections::BTreeMap;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use image::RgbaImage;

use crate::content::Content;
use crate::player::{FrameStats, Player};
use crate::target::{Offscreen, Target, WindowHandles, WindowSurface};
use crate::{AdapterSummary, Gpu, GpuOptions, RenderError};

/// Slot id; the daemon uses monitor ids.
pub type SlotId = u32;

/// What to draw into.
#[derive(Debug, Clone, Copy)]
pub enum TargetSpec {
    /// An offscreen texture of this size.
    Offscreen {
        /// Width in pixels.
        width: u32,
        /// Height in pixels.
        height: u32,
    },
    /// A native window.
    Window {
        /// Its handles.
        handles: WindowHandles,
        /// Width in pixels.
        width: u32,
        /// Height in pixels.
        height: u32,
    },
}

/// Per-slot statistics.
#[derive(Debug, Clone, PartialEq)]
pub struct SlotStats {
    /// Slot id.
    pub slot: SlotId,
    /// Backend name, if content is loaded.
    pub kind: Option<&'static str>,
    /// Frame counters.
    pub frames: FrameStats,
    /// Current frame cap (0 = paused).
    pub fps_cap: u32,
    /// Estimated GPU bytes held.
    pub gpu_bytes: u64,
    /// How an animation stores frames, if it is one.
    pub animation_mode: Option<&'static str>,
}

/// Render loop statistics.
#[derive(Debug, Clone, PartialEq)]
pub struct LoopStats {
    /// The open adapter, or `None` if the device is closed.
    pub adapter: Option<AdapterSummary>,
    /// True if GPU timestamps are being recorded.
    pub timestamps: bool,
    /// Per-slot numbers.
    pub slots: Vec<SlotStats>,
}

type Reply<T> = Sender<Result<T, String>>;

enum Command {
    Attach(SlotId, TargetSpec, u32, Reply<()>),
    Load(SlotId, Box<Content>, Reply<()>),
    Clear(SlotId),
    Detach(SlotId),
    SetFps(SlotId, u32),
    SetMouse(SlotId, Option<[f32; 4]>),
    Resize(SlotId, u32, u32),
    Snapshot(SlotId, Reply<RgbaImage>),
    Stats(Sender<LoopStats>),
    Shutdown,
}

/// Handle to a running render thread. Dropping it stops the thread.
#[derive(Debug)]
pub struct RenderLoop {
    tx: Sender<Command>,
    thread: Option<JoinHandle<()>>,
}

impl std::fmt::Debug for Command {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Command")
    }
}

impl RenderLoop {
    /// Start the render thread. No GPU is opened until a target is attached.
    pub fn spawn(opts: GpuOptions) -> RenderLoop {
        let (tx, rx) = channel();
        let thread = std::thread::Builder::new()
            .name("ww-render".into())
            .spawn(move || run(opts, rx))
            .ok();
        RenderLoop { tx, thread }
    }

    fn call<T>(&self, make: impl FnOnce(Reply<T>) -> Command) -> Result<T, RenderError> {
        let (tx, rx) = channel();
        self.tx
            .send(make(tx))
            .map_err(|_| RenderError::Gpu("the render thread has stopped".into()))?;
        rx.recv()
            .map_err(|_| RenderError::Gpu("the render thread has stopped".into()))?
            .map_err(RenderError::Gpu)
    }

    /// Attach a target to a slot, opening the GPU if needed.
    pub fn attach(
        &self,
        slot: SlotId,
        target: TargetSpec,
        fps_cap: u32,
    ) -> Result<(), RenderError> {
        self.call(|r| Command::Attach(slot, target, fps_cap, r))
    }

    /// Show decoded content on a slot.
    pub fn load(&self, slot: SlotId, content: Content) -> Result<(), RenderError> {
        self.call(|r| Command::Load(slot, Box::new(content), r))
    }

    /// Drop a slot's content and its GPU memory, keeping the target.
    pub fn clear(&self, slot: SlotId) {
        let _ = self.tx.send(Command::Clear(slot));
    }

    /// Remove a slot entirely. The GPU closes with the last slot.
    pub fn detach(&self, slot: SlotId) {
        let _ = self.tx.send(Command::Detach(slot));
    }

    /// Set a slot's frame cap; 0 pauses it.
    pub fn set_fps(&self, slot: SlotId, fps: u32) {
        let _ = self.tx.send(Command::SetFps(slot, fps));
    }

    /// Pointer position for a slot's shader, or `None` for zeros.
    pub fn set_mouse(&self, slot: SlotId, mouse: Option<[f32; 4]>) {
        let _ = self.tx.send(Command::SetMouse(slot, mouse));
    }

    /// Resize a slot's target.
    pub fn resize(&self, slot: SlotId, width: u32, height: u32) {
        let _ = self.tx.send(Command::Resize(slot, width, height));
    }

    /// Read back an offscreen slot's last frame.
    pub fn snapshot(&self, slot: SlotId) -> Result<RgbaImage, RenderError> {
        self.call(|r| Command::Snapshot(slot, r))
    }

    /// Current statistics.
    pub fn stats(&self) -> Option<LoopStats> {
        let (tx, rx) = channel();
        self.tx.send(Command::Stats(tx)).ok()?;
        rx.recv().ok()
    }
}

impl Drop for RenderLoop {
    fn drop(&mut self) {
        let _ = self.tx.send(Command::Shutdown);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

struct Slot {
    player: Player,
}

fn run(opts: GpuOptions, rx: Receiver<Command>) {
    let start = Instant::now();
    let mut gpu: Option<Gpu> = None;
    let mut slots: BTreeMap<SlotId, Slot> = BTreeMap::new();
    loop {
        let now = start.elapsed();
        // Draw everything that is due.
        if let Some(g) = &gpu {
            for slot in slots.values_mut() {
                if slot.player.next_due(now).is_some_and(|t| t <= now) {
                    // A failed frame (lost surface) is retried at the next
                    // due time; the daemon re-attaches on monitor changes.
                    let _ = slot.player.render(g, now);
                }
            }
        }
        let now = start.elapsed();
        let next = slots
            .values_mut()
            .filter_map(|s| s.player.next_due(now))
            .min();
        let cmd = match next {
            // Nothing scheduled: block until told otherwise. Zero CPU.
            None => match rx.recv() {
                Ok(c) => c,
                Err(_) => break,
            },
            Some(t) => match rx.recv_timeout(t.saturating_sub(now)) {
                Ok(c) => c,
                Err(RecvTimeoutError::Timeout) => continue,
                Err(RecvTimeoutError::Disconnected) => break,
            },
        };
        let now = start.elapsed();
        match cmd {
            Command::Attach(id, spec, fps, reply) => {
                if gpu.is_none() {
                    match Gpu::new(&opts) {
                        Ok(g) => gpu = Some(g),
                        Err(e) => {
                            let _ = reply.send(Err(e.to_string()));
                            continue;
                        }
                    }
                }
                let Some(g) = &gpu else { continue };
                slots.remove(&id);
                let target = match spec {
                    TargetSpec::Offscreen { width, height } => {
                        Ok(Target::Offscreen(Offscreen::new(g, width, height)))
                    }
                    TargetSpec::Window {
                        handles,
                        width,
                        height,
                    } => {
                        // SAFETY: the desktop host keeps the window alive until it
                        // detaches this slot (see WindowHandles).
                        unsafe { WindowSurface::new(g, handles, width, height) }.map(Target::Window)
                    }
                };
                match target {
                    Ok(t) => {
                        slots.insert(
                            id,
                            Slot {
                                player: Player::new(t, fps),
                            },
                        );
                        let _ = reply.send(Ok(()));
                    }
                    Err(e) => {
                        let _ = reply.send(Err(e.to_string()));
                    }
                }
                if slots.is_empty() {
                    gpu = None;
                }
            }
            Command::Load(id, content, reply) => {
                let res = match (&gpu, slots.get_mut(&id)) {
                    (Some(g), Some(slot)) => slot
                        .player
                        .set_content(g, *content, now)
                        .map_err(|e| e.to_string()),
                    _ => Err(format!("slot {id} has no target")),
                };
                let _ = reply.send(res);
            }
            Command::Clear(id) => {
                if let Some(slot) = slots.get_mut(&id) {
                    slot.player.clear();
                }
            }
            Command::Detach(id) => {
                slots.remove(&id);
                if slots.is_empty() {
                    // Last target gone: close the device and free driver memory.
                    gpu = None;
                    release_heap();
                }
            }
            Command::SetFps(id, fps) => {
                if let Some(slot) = slots.get_mut(&id) {
                    slot.player.set_fps_cap(fps);
                }
            }
            Command::SetMouse(id, m) => {
                if let Some(slot) = slots.get_mut(&id) {
                    slot.player.set_mouse(m);
                }
            }
            Command::Resize(id, w, h) => {
                if let (Some(g), Some(slot)) = (&gpu, slots.get_mut(&id)) {
                    slot.player.resize(g, w, h);
                }
            }
            Command::Snapshot(id, reply) => {
                let res = match (&gpu, slots.get(&id).map(|s| s.player.target())) {
                    (Some(g), Some(Target::Offscreen(o))) => {
                        o.read_rgba(g).map_err(|e| e.to_string())
                    }
                    _ => Err(format!("slot {id} is not an offscreen target")),
                };
                let _ = reply.send(res);
            }
            Command::Stats(reply) => {
                let stats = LoopStats {
                    adapter: gpu.as_ref().map(|g| g.adapter().clone()),
                    timestamps: gpu.as_ref().is_some_and(Gpu::has_timestamps),
                    slots: slots
                        .iter()
                        .map(|(id, s)| SlotStats {
                            slot: *id,
                            kind: s.player.scene().map(|sc| sc.kind()),
                            frames: s.player.stats(),
                            fps_cap: s.player.fps_cap(),
                            gpu_bytes: s.player.gpu_bytes(),
                            animation_mode: s.player.scene().and_then(|sc| sc.animation_mode()),
                        })
                        .collect(),
                };
                let _ = reply.send(stats);
            }
            Command::Shutdown => break,
        }
    }
    // Surfaces must go before the device.
    slots.clear();
    drop(gpu);
}

/// Hand freed heap pages back to the OS. glibc keeps freed memory in its
/// arenas, so after closing a GPU device (which frees a lot at once) the
/// process would otherwise keep looking as big as it was while drawing.
fn release_heap() {
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    // SAFETY: malloc_trim only releases unused memory and has no
    // preconditions.
    unsafe {
        libc::malloc_trim(0);
    }
}

/// Sleep helper for callers that drive a [`Player`] themselves.
pub fn sleep_until(start: Instant, at: Duration) {
    let now = start.elapsed();
    if at > now {
        std::thread::sleep(at - now);
    }
}
