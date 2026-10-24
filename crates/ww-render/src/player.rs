//! One wallpaper on one target: content, pacing and frame submission.

use std::time::Duration;

use crate::content::Content;
use crate::pacing::{NextChange, next_frame};
use crate::scene::{FrameInput, Scene};
use crate::target::{Acquired, Target};
use crate::{Gpu, RenderError};

/// Frame statistics.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct FrameStats {
    /// Frames drawn since the content was set.
    pub frames: u64,
    /// Frames with a GPU timestamp measurement.
    pub timed_frames: u64,
    /// Sum of measured GPU time, in nanoseconds.
    pub gpu_ns: u64,
}

impl FrameStats {
    /// Average GPU time per timed frame in milliseconds, if measured.
    pub fn gpu_ms_per_frame(&self) -> Option<f64> {
        (self.timed_frames > 0).then(|| self.gpu_ns as f64 / self.timed_frames as f64 / 1e6)
    }
}

/// Plays one wallpaper on one target.
#[derive(Debug)]
pub struct Player {
    target: Target,
    scene: Option<Scene>,
    fps_cap: u32,
    mouse: [f32; 4],
    last_frame: Option<Duration>,
    dirty: bool,
    stats: FrameStats,
    timer: Option<GpuTimer>,
}

impl Player {
    /// A player with nothing to show yet, running at `fps_cap`.
    pub fn new(target: Target, fps_cap: u32) -> Player {
        Player {
            target,
            scene: None,
            fps_cap,
            mouse: [0.0; 4],
            last_frame: None,
            dirty: true,
            stats: FrameStats::default(),
            timer: None,
        }
    }

    /// Show new content. The previous scene and its GPU memory are released
    /// first.
    pub fn set_content(
        &mut self,
        gpu: &Gpu,
        content: Content,
        now: Duration,
    ) -> Result<(), RenderError> {
        self.scene = None;
        self.scene = Some(Scene::new(gpu, content, self.target.format(), now)?);
        self.dirty = true;
        self.last_frame = None;
        self.stats = FrameStats::default();
        if gpu.has_timestamps() && self.timer.is_none() {
            self.timer = Some(GpuTimer::new(gpu));
        }
        Ok(())
    }

    /// Drop the content and every GPU resource it held.
    pub fn clear(&mut self) {
        self.scene = None;
        self.timer = None;
    }

    /// Set the frame cap from the governor. 0 pauses: no frames, no timers.
    pub fn set_fps_cap(&mut self, fps: u32) {
        let resumed = self.fps_cap == 0 && fps > 0;
        self.fps_cap = fps;
        if resumed {
            // Redraw once on resume in case the window was hidden meanwhile.
            self.dirty = true;
        }
    }

    /// Current frame cap (0 = paused).
    pub fn fps_cap(&self) -> u32 {
        self.fps_cap
    }

    /// Pointer position for shaders, already gated by the permission check.
    /// `None` reports zeros.
    pub fn set_mouse(&mut self, mouse: Option<[f32; 4]>) {
        self.mouse = mouse.unwrap_or([0.0; 4]);
    }

    /// Resize the target and redraw.
    pub fn resize(&mut self, gpu: &Gpu, width: u32, height: u32) {
        self.target.resize(gpu, width, height);
        self.dirty = true;
    }

    /// When the next frame is due, or `None` if nothing needs drawing until
    /// something external changes.
    pub fn next_due(&mut self, now: Duration) -> Option<Duration> {
        let scene = self.scene.as_mut()?;
        scene.poll();
        let cap = match scene.max_fps() {
            Some(m) => self.fps_cap.min(m),
            None => self.fps_cap,
        };
        let change = scene.next_change(now);
        next_frame(change, self.dirty, self.last_frame, cap, now)
    }

    /// True if the content never changes on its own.
    pub fn is_static(&self, now: Duration) -> bool {
        self.scene
            .as_ref()
            .is_none_or(|s| s.next_change(now) == NextChange::Never)
    }

    /// Draw one frame now. Returns false if the window wasn't ready.
    pub fn render(&mut self, gpu: &Gpu, now: Duration) -> Result<bool, RenderError> {
        let Some(scene) = self.scene.as_mut() else {
            return Ok(false);
        };
        let input = FrameInput {
            now,
            target_size: self.target.size(),
            mouse: self.mouse,
            fps_cap: self.fps_cap,
        };
        scene.update(gpu, &input);
        let acquired = self.target.acquire(gpu)?;
        let view = match &acquired {
            Acquired::Offscreen(v) | Acquired::Window(_, v) => v,
            Acquired::Skip => {
                // Try again at the next due time rather than spinning.
                self.last_frame = Some(now);
                return Ok(false);
            }
        };
        let mut encoder = gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("frame"),
            });
        let timestamps = self
            .timer
            .as_ref()
            .map(|t| wgpu::RenderPassTimestampWrites {
                query_set: &t.queries,
                beginning_of_pass_write_index: Some(0),
                end_of_pass_write_index: Some(1),
            });
        scene.encode(&mut encoder, view, timestamps);
        if let Some(t) = &self.timer {
            t.resolve(&mut encoder);
        }
        gpu.queue.submit([encoder.finish()]);
        if let Acquired::Window(frame, _) = acquired {
            gpu.queue.present(frame);
        }
        if let Some(t) = &self.timer
            && let Some(ns) = t.read(gpu)
        {
            self.stats.gpu_ns += ns;
            self.stats.timed_frames += 1;
        }
        self.stats.frames += 1;
        self.last_frame = Some(now);
        self.dirty = false;
        Ok(true)
    }

    /// Frame statistics since the content was set.
    pub fn stats(&self) -> FrameStats {
        self.stats
    }

    /// GPU memory held by the current scene (estimate).
    pub fn gpu_bytes(&self) -> u64 {
        self.scene.as_ref().map_or(0, Scene::gpu_bytes)
    }

    /// The scene, if content is set.
    pub fn scene(&self) -> Option<&Scene> {
        self.scene.as_ref()
    }

    /// The target.
    pub fn target(&self) -> &Target {
        &self.target
    }
}

/// GPU timestamps around the render pass. Only created when the adapter
/// supports timestamps and they were requested (benchmarks).
#[derive(Debug)]
struct GpuTimer {
    queries: wgpu::QuerySet,
    resolve: wgpu::Buffer,
    readback: wgpu::Buffer,
}

impl GpuTimer {
    fn new(gpu: &Gpu) -> GpuTimer {
        let queries = gpu.device.create_query_set(&wgpu::QuerySetDescriptor {
            label: Some("frame timer"),
            ty: wgpu::QueryType::Timestamp,
            count: 2,
        });
        let resolve = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("frame timer resolve"),
            size: 16,
            usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let readback = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("frame timer readback"),
            size: 16,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        GpuTimer {
            queries,
            resolve,
            readback,
        }
    }

    fn resolve(&self, encoder: &mut wgpu::CommandEncoder) {
        encoder.resolve_query_set(&self.queries, 0..2, &self.resolve, 0);
        encoder.copy_buffer_to_buffer(&self.resolve, 0, &self.readback, 0, 16);
    }

    fn read(&self, gpu: &Gpu) -> Option<u64> {
        let slice = self.readback.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        gpu.wait_idle();
        rx.recv().ok()?.ok()?;
        let data = slice.get_mapped_range().ok()?;
        let a = u64::from_le_bytes(data[0..8].try_into().ok()?);
        let b = u64::from_le_bytes(data[8..16].try_into().ok()?);
        drop(data);
        self.readback.unmap();
        let period = f64::from(gpu.queue.get_timestamp_period());
        Some((b.saturating_sub(a) as f64 * period) as u64)
    }
}
