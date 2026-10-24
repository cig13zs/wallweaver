//! Where frames go: an offscreen texture (tests, benchmarks) or a window.

use image::RgbaImage;
use raw_window_handle::{RawDisplayHandle, RawWindowHandle};

use crate::{Gpu, RenderError};

/// Format of offscreen targets. Not sRGB, so shader output lands unchanged
/// (the way Shadertoy and browsers treat it) and image bytes pass straight
/// through.
pub const OFFSCREEN_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

/// An offscreen render target.
#[derive(Debug)]
pub struct Offscreen {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    size: (u32, u32),
}

impl Offscreen {
    /// Create a target of the given size.
    pub fn new(gpu: &Gpu, width: u32, height: u32) -> Offscreen {
        let size = (width.max(1), height.max(1));
        let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("offscreen target"),
            size: wgpu::Extent3d {
                width: size.0,
                height: size.1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: OFFSCREEN_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&Default::default());
        Offscreen {
            texture,
            view,
            size,
        }
    }

    /// Copy the last frame back to the CPU.
    pub fn read_rgba(&self, gpu: &Gpu) -> Result<RgbaImage, RenderError> {
        let (w, h) = self.size;
        let row = (w * 4).div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT)
            * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("readback"),
            size: u64::from(row) * u64::from(h),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut enc = gpu.device.create_command_encoder(&Default::default());
        enc.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(row),
                    rows_per_image: Some(h),
                },
            },
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );
        gpu.queue.submit([enc.finish()]);
        let slice = buffer.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        gpu.wait_idle();
        rx.recv()
            .map_err(|_| RenderError::Gpu("readback was cancelled".into()))?
            .map_err(|e| RenderError::Gpu(e.to_string()))?;
        let data = slice
            .get_mapped_range()
            .map_err(|e| RenderError::Gpu(e.to_string()))?;
        let mut out = Vec::with_capacity((w * h * 4) as usize);
        for y in 0..h as usize {
            let start = y * row as usize;
            out.extend_from_slice(&data[start..start + (w * 4) as usize]);
        }
        drop(data);
        buffer.unmap();
        RgbaImage::from_raw(w, h, out)
            .ok_or_else(|| RenderError::Gpu("readback size mismatch".into()))
    }
}

/// Raw handles of a native window, made sendable to the render thread.
#[derive(Debug, Clone, Copy)]
pub struct WindowHandles {
    /// Display connection (X11 display, or the Windows/macOS marker).
    pub display: RawDisplayHandle,
    /// The window itself.
    pub window: RawWindowHandle,
}

// SAFETY: the handles are plain identifiers (an HWND, an X11 window id plus
// display pointer). Win32 and Xlib/XCB accept them from any thread, and the
// desktop host keeps the window alive until the render loop has detached
// the slot that uses it.
unsafe impl Send for WindowHandles {}

/// A window target.
#[derive(Debug)]
pub struct WindowSurface {
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
}

impl WindowSurface {
    /// Create a surface for a native window.
    ///
    /// # Safety
    ///
    /// The window behind `handles` must stay alive, and must not be
    /// destroyed, until this surface is dropped.
    pub unsafe fn new(
        gpu: &Gpu,
        handles: WindowHandles,
        width: u32,
        height: u32,
    ) -> Result<WindowSurface, RenderError> {
        // SAFETY: forwarded from the caller's contract above.
        let surface = unsafe {
            gpu.instance
                .create_surface_unsafe(wgpu::SurfaceTargetUnsafe::RawHandle {
                    raw_display_handle: Some(handles.display),
                    raw_window_handle: handles.window,
                })
        }
        .map_err(|e| RenderError::Surface(e.to_string()))?;
        if !gpu.adapter.is_surface_supported(&surface) {
            return Err(RenderError::Surface(
                "the graphics adapter can't draw to this window".into(),
            ));
        }
        let caps = surface.get_capabilities(&gpu.adapter);
        let format = caps
            .formats
            .iter()
            .copied()
            .find(|f| !f.is_srgb())
            .or_else(|| caps.formats.first().copied())
            .ok_or_else(|| RenderError::Surface("the window offers no pixel formats".into()))?;
        // Fifo is vsync: frames line up with the display and never tear. With
        // two frames of latency allowed, the GPU never queues work ahead.
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            color_space: wgpu::SurfaceColorSpace::Auto,
            width: width.max(1),
            height: height.max(1),
            present_mode: wgpu::PresentMode::Fifo,
            desired_maximum_frame_latency: 1,
            alpha_mode: caps
                .alpha_modes
                .first()
                .copied()
                .unwrap_or(wgpu::CompositeAlphaMode::Auto),
            view_formats: vec![],
        };
        surface.configure(&gpu.device, &config);
        Ok(WindowSurface { surface, config })
    }
}

/// A render target.
#[derive(Debug)]
pub enum Target {
    /// Offscreen texture.
    Offscreen(Offscreen),
    /// A window on the desktop.
    Window(WindowSurface),
}

/// A frame in progress.
pub(crate) enum Acquired {
    Offscreen(wgpu::TextureView),
    Window(wgpu::SurfaceTexture, wgpu::TextureView),
    Skip,
}

impl Target {
    /// Pixel size.
    pub fn size(&self) -> (u32, u32) {
        match self {
            Target::Offscreen(o) => o.size,
            Target::Window(w) => (w.config.width, w.config.height),
        }
    }

    /// Pixel format.
    pub fn format(&self) -> wgpu::TextureFormat {
        match self {
            Target::Offscreen(_) => OFFSCREEN_FORMAT,
            Target::Window(w) => w.config.format,
        }
    }

    /// Change the size (monitor resolution change).
    pub fn resize(&mut self, gpu: &Gpu, width: u32, height: u32) {
        match self {
            Target::Offscreen(o) => *o = Offscreen::new(gpu, width, height),
            Target::Window(w) => {
                w.config.width = width.max(1);
                w.config.height = height.max(1);
                w.surface.configure(&gpu.device, &w.config);
            }
        }
    }

    pub(crate) fn acquire(&mut self, gpu: &Gpu) -> Result<Acquired, RenderError> {
        match self {
            Target::Offscreen(o) => Ok(Acquired::Offscreen(o.view.clone())),
            Target::Window(w) => match w.surface.get_current_texture() {
                wgpu::CurrentSurfaceTexture::Success(t)
                | wgpu::CurrentSurfaceTexture::Suboptimal(t) => {
                    let view = t.texture.create_view(&Default::default());
                    Ok(Acquired::Window(t, view))
                }
                wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                    Ok(Acquired::Skip)
                }
                wgpu::CurrentSurfaceTexture::Outdated => {
                    w.surface.configure(&gpu.device, &w.config);
                    Ok(Acquired::Skip)
                }
                wgpu::CurrentSurfaceTexture::Lost => {
                    Err(RenderError::Surface("the window surface was lost".into()))
                }
                wgpu::CurrentSurfaceTexture::Validation => {
                    Err(RenderError::Surface("surface validation error".into()))
                }
            },
        }
    }
}
