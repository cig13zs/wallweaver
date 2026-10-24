//! GPU-side wallpapers. A [`Scene`] owns every GPU resource of one
//! wallpaper on one target; dropping it frees them.

use std::borrow::Cow;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, SyncSender, TryRecvError, channel, sync_channel};
use std::time::Duration;

use image::RgbaImage;
use wgpu::util::DeviceExt;

use crate::content::{
    AnimatedContent, Content, Frames, Look, ShaderContent, SlideshowContent, StreamSource,
    frame_delay, frame_iter, load_still,
};
use crate::fit::place;
use crate::pacing::NextChange;
use crate::shader::{GLOBALS_SIZE, Globals};
use crate::{Gpu, RenderError};

const QUAD_WGSL: &str = "
struct Quad {
    rect: vec4<f32>,
    layer: u32,
    p0: u32,
    p1: u32,
    p2: u32,
}
@group(0) @binding(0) var<uniform> quad: Quad;
@group(0) @binding(1) var tex: texture_2d_array<f32>;
@group(0) @binding(2) var samp: sampler;

struct VOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@vertex
fn vs(@builtin(vertex_index) i: u32) -> VOut {
    let c = vec2<f32>(f32(i & 1u), f32((i >> 1u) & 1u));
    var out: VOut;
    out.pos = vec4<f32>(mix(quad.rect.x, quad.rect.z, c.x), mix(quad.rect.y, quad.rect.w, c.y), 0.0, 1.0);
    out.uv = c;
    return out;
}

@fragment
fn fs(v: VOut) -> @location(0) vec4<f32> {
    return textureSample(tex, samp, v.uv, i32(quad.layer));
}
";

#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct QuadUniform {
    rect: [f32; 4],
    layer: u32,
    pad: [u32; 3],
}

/// Per-frame inputs from the player.
#[derive(Debug, Clone, Copy)]
pub(crate) struct FrameInput {
    pub now: Duration,
    pub target_size: (u32, u32),
    pub mouse: [f32; 4],
    pub fps_cap: u32,
}

/// A wallpaper made GPU-resident for one target format.
#[derive(Debug)]
pub struct Scene {
    kind: Kind,
    format: wgpu::TextureFormat,
}

#[derive(Debug)]
enum Kind {
    Image(ImageScene),
    Slideshow(SlideshowScene),
    Animated(AnimatedScene),
    Shader(ShaderScene),
}

impl Scene {
    /// Upload content for a target with the given format. CPU copies of
    /// GPU-resident pixels are dropped once uploaded.
    pub fn new(
        gpu: &Gpu,
        content: Content,
        format: wgpu::TextureFormat,
        now: Duration,
    ) -> Result<Scene, RenderError> {
        let kind = match content {
            Content::Still { image, look } => {
                let quad = QuadPipeline::new(gpu, format);
                let tex = LayerTexture::from_images(gpu, &[image]);
                let bg = quad.bind(gpu, &tex);
                Kind::Image(ImageScene {
                    quad,
                    tex,
                    bg,
                    look,
                })
            }
            Content::Slideshow(show) => {
                Kind::Slideshow(SlideshowScene::new(gpu, format, show, now))
            }
            Content::Animated(anim) => Kind::Animated(AnimatedScene::new(gpu, format, anim, now)),
            Content::Shader(shader) => Kind::Shader(ShaderScene::new(gpu, format, *shader)?),
        };
        Ok(Scene { kind, format })
    }

    /// Target format the scene was built for.
    pub fn format(&self) -> wgpu::TextureFormat {
        self.format
    }

    /// Short backend name.
    pub fn kind(&self) -> &'static str {
        match self.kind {
            Kind::Image(_) => "image",
            Kind::Slideshow(_) => "slideshow",
            Kind::Animated(_) => "animated",
            Kind::Shader(_) => "shader",
        }
    }

    /// When the visible content next changes.
    pub(crate) fn next_change(&self, now: Duration) -> NextChange {
        match &self.kind {
            Kind::Image(_) => NextChange::Never,
            Kind::Slideshow(s) => s.next_change(now),
            Kind::Animated(a) => a.next_change(now),
            Kind::Shader(_) => NextChange::EveryFrame,
        }
    }

    /// Collect results from worker threads (decoded slideshow images and
    /// streamed animation frames).
    pub(crate) fn poll(&mut self) {
        match &mut self.kind {
            Kind::Slideshow(s) => s.poll(),
            Kind::Animated(a) => {
                if let AnimFrames::Stream { rx, next, .. } = &mut a.frames
                    && next.is_none()
                {
                    *next = rx.try_recv().ok();
                }
            }
            _ => {}
        }
    }

    /// The scene's own frame cap (shaders and animations may set one).
    pub(crate) fn max_fps(&self) -> Option<u32> {
        match &self.kind {
            Kind::Shader(s) => Some(s.max_fps),
            Kind::Animated(a) => a.max_fps,
            _ => None,
        }
    }

    /// Bring textures and uniforms up to date for time `now`.
    pub(crate) fn update(&mut self, gpu: &Gpu, input: &FrameInput) {
        match &mut self.kind {
            Kind::Image(s) => s
                .quad
                .set_rect(gpu, s.look, s.tex.size, input.target_size, 0),
            Kind::Slideshow(s) => s.update(gpu, input),
            Kind::Animated(a) => a.update(gpu, input),
            Kind::Shader(s) => s.update(gpu, input),
        }
    }

    /// Record the draw into `view`.
    pub(crate) fn encode(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        timestamps: Option<wgpu::RenderPassTimestampWrites<'_>>,
    ) {
        let clear = match &self.kind {
            Kind::Image(s) => s.look.background,
            Kind::Slideshow(s) => s.look.background,
            Kind::Animated(a) => a.look.background,
            Kind::Shader(_) => [0.0, 0.0, 0.0, 1.0],
        };
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("wallpaper"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: clear[0],
                        g: clear[1],
                        b: clear[2],
                        a: clear[3],
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: timestamps,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        match &self.kind {
            Kind::Image(s) => s.quad.draw(&mut pass, &s.bg),
            Kind::Slideshow(s) => s.quad.draw(&mut pass, &s.bg),
            Kind::Animated(a) => a.quad.draw(&mut pass, &a.bg),
            Kind::Shader(s) => {
                pass.set_pipeline(&s.pipeline);
                pass.set_bind_group(0, &s.bind_group, &[]);
                pass.draw(0..3, 0..1);
            }
        }
    }

    /// Bytes of texture memory this scene holds on the GPU (an estimate:
    /// drivers may pad).
    pub fn gpu_bytes(&self) -> u64 {
        match &self.kind {
            Kind::Image(s) => s.tex.bytes(),
            Kind::Slideshow(s) => s.tex.bytes(),
            Kind::Animated(a) => a.tex.bytes(),
            Kind::Shader(_) => GLOBALS_SIZE,
        }
    }

    /// How an animation keeps its frames, for diagnostics and tests.
    pub fn animation_mode(&self) -> Option<&'static str> {
        match &self.kind {
            Kind::Animated(a) => Some(match a.frames {
                AnimFrames::Array { .. } => "gpu-array",
                AnimFrames::Upload { .. } => "cpu-upload",
                AnimFrames::Stream { .. } => "streamed",
            }),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------- textures

#[derive(Debug)]
struct LayerTexture {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    size: (u32, u32),
    layers: u32,
}

impl LayerTexture {
    fn create(gpu: &Gpu, size: (u32, u32), layers: u32) -> LayerTexture {
        let side = gpu.max_texture_side();
        let size = (size.0.clamp(1, side), size.1.clamp(1, side));
        let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("wallpaper image"),
            size: wgpu::Extent3d {
                width: size.0,
                height: size.1,
                depth_or_array_layers: layers.max(1),
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        });
        LayerTexture {
            texture,
            view,
            size,
            layers: layers.max(1),
        }
    }

    fn from_images(gpu: &Gpu, images: &[RgbaImage]) -> LayerTexture {
        let size = images.first().map(|i| i.dimensions()).unwrap_or((1, 1));
        let tex = LayerTexture::create(gpu, size, images.len() as u32);
        for (i, img) in images.iter().enumerate() {
            tex.write(gpu, i as u32, img);
        }
        tex
    }

    fn write(&self, gpu: &Gpu, layer: u32, img: &RgbaImage) {
        let (w, h) = img.dimensions();
        let (w, h) = (w.min(self.size.0), h.min(self.size.1));
        gpu.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: 0,
                    y: 0,
                    z: layer,
                },
                aspect: wgpu::TextureAspect::All,
            },
            img.as_raw(),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(img.width() * 4),
                rows_per_image: Some(img.height()),
            },
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );
    }

    fn bytes(&self) -> u64 {
        u64::from(self.size.0) * u64::from(self.size.1) * 4 * u64::from(self.layers)
    }
}

// ------------------------------------------------------------ quad pipeline

#[derive(Debug)]
struct QuadPipeline {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    uniform: wgpu::Buffer,
}

impl QuadPipeline {
    fn new(gpu: &Gpu, format: wgpu::TextureFormat) -> QuadPipeline {
        let device = &gpu.device;
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("quad"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(QUAD_WGSL)),
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("quad"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("quad"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("quad"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleStrip,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("quad"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("quad"),
            size: std::mem::size_of::<QuadUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        QuadPipeline {
            pipeline,
            layout,
            sampler,
            uniform,
        }
    }

    fn set_rect(&self, gpu: &Gpu, look: Look, content: (u32, u32), target: (u32, u32), layer: u32) {
        let rect = place(look.fit, content, target).to_ndc(target);
        let u = QuadUniform {
            rect,
            layer,
            pad: [0; 3],
        };
        gpu.queue
            .write_buffer(&self.uniform, 0, bytemuck::bytes_of(&u));
    }

    /// A bind group for one texture. Rebuilt whenever the texture is replaced.
    fn bind(&self, gpu: &Gpu, tex: &LayerTexture) -> wgpu::BindGroup {
        tex.texture_bind_group(&gpu.device, &self.layout, &self.uniform, &self.sampler)
    }

    fn draw(&self, pass: &mut wgpu::RenderPass<'_>, bind_group: &wgpu::BindGroup) {
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, bind_group, &[]);
        pass.draw(0..4, 0..1);
    }
}

impl LayerTexture {
    fn texture_bind_group(
        &self,
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        uniform: &wgpu::Buffer,
        sampler: &wgpu::Sampler,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("quad"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&self.view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
            ],
        })
    }
}

// ------------------------------------------------------------------ image

#[derive(Debug)]
struct ImageScene {
    quad: QuadPipeline,
    tex: LayerTexture,
    bg: wgpu::BindGroup,
    look: Look,
}

// -------------------------------------------------------------- slideshow

#[derive(Debug)]
struct SlideshowScene {
    quad: QuadPipeline,
    tex: LayerTexture,
    bg: wgpu::BindGroup,
    look: Look,
    interval: Duration,
    /// When the current image went up.
    shown_at: Duration,
    /// Position in `order` of the current image.
    pos: usize,
    order: Vec<usize>,
    requests: Sender<usize>,
    ready: Receiver<(usize, Result<RgbaImage, String>)>,
    pending: Option<usize>,
    prefetched: Option<(usize, RgbaImage)>,
}

impl SlideshowScene {
    fn new(
        gpu: &Gpu,
        format: wgpu::TextureFormat,
        show: SlideshowContent,
        now: Duration,
    ) -> SlideshowScene {
        let n = show.images.len().max(1);
        let mut order: Vec<usize> = (0..n).collect();
        if show.shuffle {
            shuffle(&mut order[1..], seed());
        }
        let quad = QuadPipeline::new(gpu, format);
        let tex = LayerTexture::from_images(gpu, &[show.first]);
        let bg = quad.bind(gpu, &tex);
        let (req_tx, req_rx) = channel::<usize>();
        let (img_tx, img_rx) = channel();
        let images = show.images;
        let fit = show.look.fit;
        let target = show.target_size;
        // One small decoder thread per slideshow. It exits when the scene
        // (and with it the request sender) is dropped.
        let _ = std::thread::Builder::new()
            .name("ww-slideshow".into())
            .spawn(move || {
                while let Ok(i) = req_rx.recv() {
                    let res = images
                        .get(i)
                        .ok_or_else(|| "no such image".to_string())
                        .and_then(|p| load_still(p, fit, target).map_err(|e| e.to_string()));
                    if img_tx.send((i, res)).is_err() {
                        break;
                    }
                }
            });
        let mut s = SlideshowScene {
            quad,
            tex,
            bg,
            look: show.look,
            interval: show.interval,
            shown_at: now,
            pos: 0,
            order,
            requests: req_tx,
            ready: img_rx,
            pending: None,
            prefetched: None,
        };
        s.prefetch();
        s
    }

    fn next_index(&self) -> usize {
        self.order[(self.pos + 1) % self.order.len()]
    }

    fn prefetch(&mut self) {
        if self.order.len() < 2 || self.pending.is_some() || self.prefetched.is_some() {
            return;
        }
        let i = self.next_index();
        if self.requests.send(i).is_ok() {
            self.pending = Some(i);
        }
    }

    fn poll(&mut self) {
        loop {
            match self.ready.try_recv() {
                Ok((i, Ok(img))) => {
                    self.pending = None;
                    self.prefetched = Some((i, img));
                }
                Ok((_, Err(_))) => {
                    // Skip unreadable images instead of stalling the show.
                    self.pending = None;
                    self.pos = (self.pos + 1) % self.order.len();
                    self.prefetch();
                }
                Err(TryRecvError::Empty) | Err(TryRecvError::Disconnected) => break,
            }
        }
    }

    fn next_change(&self, now: Duration) -> NextChange {
        if self.order.len() < 2 {
            return NextChange::Never;
        }
        let due = self.shown_at + self.interval;
        if due <= now && self.prefetched.is_none() {
            // Due but the next image isn't decoded yet; look again shortly.
            NextChange::At(now + Duration::from_millis(50))
        } else {
            NextChange::At(due)
        }
    }

    fn update(&mut self, gpu: &Gpu, input: &FrameInput) {
        self.poll();
        if self.order.len() >= 2
            && input.now >= self.shown_at + self.interval
            && let Some((i, img)) = self.prefetched.take()
        {
            // Same size: overwrite in place. Otherwise swap textures; the old
            // one is freed as soon as the GPU is done with it.
            if img.dimensions() == self.tex.size {
                self.tex.write(gpu, 0, &img);
            } else {
                let t = LayerTexture::from_images(gpu, &[img]);
                self.bg = self.quad.bind(gpu, &t);
                self.tex = t;
            }
            self.pos = self.order.iter().position(|&o| o == i).unwrap_or(0);
            // Stay on the interval grid even if we woke a little late.
            let late = input.now.saturating_sub(self.shown_at + self.interval);
            self.shown_at = input.now - late.min(self.interval);
            self.prefetch();
        }
        self.quad
            .set_rect(gpu, self.look, self.tex.size, input.target_size, 0);
    }
}

fn seed() -> u64 {
    let t = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0x9e37_79b9_7f4a_7c15);
    t | 1
}

/// Fisher-Yates with xorshift64. Not for anything security related.
fn shuffle(items: &mut [usize], mut state: u64) {
    for i in (1..items.len()).rev() {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let j = (state % (i as u64 + 1)) as usize;
        items.swap(i, j);
    }
}

// --------------------------------------------------------------- animated

#[derive(Debug)]
enum AnimFrames {
    /// All frames on the GPU; switching frames is a uniform change.
    Array { delays: Vec<Duration> },
    /// Frames in RAM, uploaded when shown.
    Upload {
        images: Vec<RgbaImage>,
        delays: Vec<Duration>,
    },
    /// Frames decoded by a worker thread.
    Stream {
        rx: Receiver<(RgbaImage, Duration)>,
        stop: Arc<AtomicBool>,
        delay: Duration,
        /// No frame shown yet.
        waiting: bool,
        /// The next frame, already received from the worker.
        next: Option<(RgbaImage, Duration)>,
    },
}

#[derive(Debug)]
struct AnimatedScene {
    quad: QuadPipeline,
    tex: LayerTexture,
    bg: wgpu::BindGroup,
    look: Look,
    max_fps: Option<u32>,
    frames: AnimFrames,
    index: usize,
    shown_at: Duration,
    uploaded: Option<usize>,
}

impl AnimatedScene {
    fn new(
        gpu: &Gpu,
        format: wgpu::TextureFormat,
        anim: AnimatedContent,
        now: Duration,
    ) -> AnimatedScene {
        let quad = QuadPipeline::new(gpu, format);
        let (tex, frames) = match anim.frames {
            Frames::Decoded {
                images,
                delays,
                gpu_resident,
            } if gpu_resident && images.len() as u32 <= gpu.max_layers() => {
                // Upload once; the CPU copies are dropped at the end of this arm.
                (
                    LayerTexture::from_images(gpu, &images),
                    AnimFrames::Array { delays },
                )
            }
            Frames::Decoded { images, delays, .. } => {
                let tex = LayerTexture::from_images(gpu, &images[..1.min(images.len())]);
                (tex, AnimFrames::Upload { images, delays })
            }
            Frames::Streamed(src) => {
                let tex = LayerTexture::create(gpu, anim.size, 1);
                let (rx, stop) = spawn_stream(src);
                (
                    tex,
                    AnimFrames::Stream {
                        rx,
                        stop,
                        delay: Duration::from_millis(100),
                        waiting: true,
                        next: None,
                    },
                )
            }
        };
        let bg = quad.bind(gpu, &tex);
        AnimatedScene {
            quad,
            tex,
            bg,
            look: anim.look,
            max_fps: anim.max_fps,
            frames,
            index: 0,
            shown_at: now,
            uploaded: Some(0),
        }
    }

    fn delays(&self) -> Option<&[Duration]> {
        match &self.frames {
            AnimFrames::Array { delays } | AnimFrames::Upload { delays, .. } => Some(delays),
            AnimFrames::Stream { .. } => None,
        }
    }

    fn next_change(&self, now: Duration) -> NextChange {
        match &self.frames {
            AnimFrames::Stream {
                delay,
                waiting,
                next,
                ..
            } => {
                let due = if *waiting {
                    now
                } else {
                    self.shown_at + *delay
                };
                if due <= now && next.is_none() {
                    // Due, but the worker hasn't delivered yet. The render
                    // loop polls again when it wakes.
                    NextChange::At(now + Duration::from_millis(5))
                } else {
                    NextChange::At(due)
                }
            }
            _ => {
                let Some(delays) = self.delays() else {
                    return NextChange::Never;
                };
                if delays.len() < 2 {
                    return NextChange::Never;
                }
                NextChange::At(self.shown_at + delays[self.index])
            }
        }
    }

    fn update(&mut self, gpu: &Gpu, input: &FrameInput) {
        let now = input.now;
        match &mut self.frames {
            AnimFrames::Stream {
                rx,
                delay,
                waiting,
                next,
                ..
            } => {
                if next.is_none() {
                    *next = rx.try_recv().ok();
                }
                if (*waiting || now >= self.shown_at + *delay)
                    && let Some((img, d)) = next.take()
                {
                    self.tex.write(gpu, 0, &img);
                    self.shown_at = if *waiting {
                        now
                    } else {
                        self.shown_at + *delay
                    };
                    if now.saturating_sub(self.shown_at) > d {
                        // Fell behind (slow decode or a pause): restart timing.
                        self.shown_at = now;
                    }
                    *delay = d;
                    *waiting = false;
                    *next = rx.try_recv().ok();
                }
            }
            AnimFrames::Array { delays } | AnimFrames::Upload { delays, .. } => {
                if delays.len() >= 2 {
                    let total: Duration = delays.iter().sum();
                    let mut elapsed = now.saturating_sub(self.shown_at);
                    if !total.is_zero() && elapsed > total {
                        // Woke after a long pause: skip whole loops at once.
                        let loops = elapsed.as_nanos() / total.as_nanos();
                        let skip = total * loops as u32;
                        self.shown_at += skip;
                        elapsed -= skip;
                    }
                    while elapsed >= delays[self.index] {
                        elapsed -= delays[self.index];
                        self.shown_at += delays[self.index];
                        self.index = (self.index + 1) % delays.len();
                    }
                }
            }
        }
        let layer = match &self.frames {
            AnimFrames::Array { .. } => self.index as u32,
            AnimFrames::Upload { images, .. } => {
                if self.uploaded != Some(self.index)
                    && let Some(img) = images.get(self.index)
                {
                    self.tex.write(gpu, 0, img);
                    self.uploaded = Some(self.index);
                }
                0
            }
            AnimFrames::Stream { .. } => 0,
        };
        self.quad
            .set_rect(gpu, self.look, self.tex.size, input.target_size, layer);
    }
}

impl Drop for AnimatedScene {
    fn drop(&mut self) {
        if let AnimFrames::Stream { stop, .. } = &self.frames {
            stop.store(true, Ordering::Relaxed);
        }
    }
}

/// Decode frames on a worker thread, looping forever, two frames ahead.
fn spawn_stream(src: StreamSource) -> (Receiver<(RgbaImage, Duration)>, Arc<AtomicBool>) {
    let (tx, rx): (SyncSender<(RgbaImage, Duration)>, _) = sync_channel(2);
    let stop = Arc::new(AtomicBool::new(false));
    let stop_worker = stop.clone();
    let _ = std::thread::Builder::new()
        .name("ww-anim-stream".into())
        .spawn(move || {
            'outer: while !stop_worker.load(Ordering::Relaxed) {
                let Ok(frames) = frame_iter(&src.bytes, src.format) else {
                    break;
                };
                let mut any = false;
                for frame in frames {
                    let Ok(frame) = frame else { break };
                    any = true;
                    let d = frame_delay(&frame, src.format);
                    let mut img = frame.into_buffer();
                    if img.dimensions() != src.size {
                        img = image::imageops::resize(
                            &img,
                            src.size.0,
                            src.size.1,
                            image::imageops::FilterType::Triangle,
                        );
                    }
                    if tx.send((img, d)).is_err() || stop_worker.load(Ordering::Relaxed) {
                        break 'outer;
                    }
                }
                if !any {
                    break;
                }
            }
        });
    (rx, stop)
}

// ----------------------------------------------------------------- shader

#[derive(Debug)]
struct ShaderScene {
    pipeline: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
    uniform: wgpu::Buffer,
    max_fps: u32,
    /// Shader clock. Advances only while frames are drawn, so a paused
    /// wallpaper resumes where it stopped.
    time: Duration,
    last: Option<Duration>,
    frame: i32,
}

impl ShaderScene {
    fn new(
        gpu: &Gpu,
        format: wgpu::TextureFormat,
        content: ShaderContent,
    ) -> Result<ShaderScene, RenderError> {
        let device = &gpu.device;
        let modules = content.modules;
        let entry = modules.fragment_entry;
        // Errors that slipped past naga validation are caught here instead of
        // panicking the render thread.
        let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let fs = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("wallpaper fragment"),
            source: wgpu::ShaderSource::Naga(Cow::Owned(modules.fragment)),
        });
        let vs = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("fullscreen vertex"),
            source: wgpu::ShaderSource::Naga(Cow::Owned(modules.vertex)),
        });
        let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("shader globals"),
            contents: bytemuck::bytes_of(&Globals::default()),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("shader globals"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: wgpu::BufferSize::new(GLOBALS_SIZE),
                },
                count: None,
            }],
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("shader globals"),
            layout: &layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform.as_entire_binding(),
            }],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("shader"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("shader"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &vs,
                entry_point: Some("ww_vs"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &fs,
                entry_point: Some(entry),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        if let Some(err) = pollster::block_on(scope.pop()) {
            return Err(RenderError::Shader(err.to_string()));
        }
        Ok(ShaderScene {
            pipeline,
            bind_group,
            uniform,
            max_fps: content.max_fps.clamp(1, 240),
            time: Duration::ZERO,
            last: None,
            frame: 0,
        })
    }

    fn update(&mut self, gpu: &Gpu, input: &FrameInput) {
        let delta = match self.last {
            // Cap the step so a long pause doesn't make the shader jump.
            Some(last) => input
                .now
                .saturating_sub(last)
                .min(Duration::from_millis(250)),
            None => Duration::ZERO,
        };
        self.last = Some(input.now);
        self.time += delta;
        let g = Globals {
            resolution: [input.target_size.0 as f32, input.target_size.1 as f32, 1.0],
            time: self.time.as_secs_f32(),
            mouse: input.mouse,
            date: date_now(),
            time_delta: delta.as_secs_f32(),
            frame: self.frame,
            frame_rate: input.fps_cap.min(self.max_fps) as f32,
            pad: 0.0,
        };
        gpu.queue
            .write_buffer(&self.uniform, 0, bytemuck::bytes_of(&g));
        self.frame = self.frame.wrapping_add(1);
    }

    /// Set the shader clock (tests and golden images).
    #[cfg(test)]
    #[allow(dead_code)]
    fn set_time(&mut self, t: Duration) {
        self.time = t;
    }
}

/// `iDate` in UTC: year, month (0-11), day, seconds since midnight.
fn date_now() -> [f32; 4] {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0);
    let days = (secs / 86_400.0).floor() as i64;
    let (y, m, d) = civil_from_days(days);
    [
        y as f32,
        (m - 1) as f32,
        d as f32,
        (secs - days as f64 * 86_400.0) as f32,
    ]
}

/// Days since 1970-01-01 to (year, month 1-12, day 1-31). Howard Hinnant's
/// public-domain algorithm.
fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn civil_dates() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(20_750), (2026, 10, 24));
        assert_eq!(civil_from_days(11_016), (2000, 2, 29));
    }

    #[test]
    fn shuffle_is_a_permutation() {
        let mut v: Vec<usize> = (0..50).collect();
        shuffle(&mut v, 12345);
        let mut sorted = v.clone();
        sorted.sort();
        assert_eq!(sorted, (0..50).collect::<Vec<_>>());
        assert_ne!(v, (0..50).collect::<Vec<_>>());
    }
}
