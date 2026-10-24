//! Wallweaver's native renderer.
//!
//! Every wallpaper type except web pages is drawn here, on `wgpu` (DirectX
//! 12 on Windows, Metal on macOS, Vulkan or OpenGL on Linux). No browser
//! engine is involved.
//!
//! Design rules:
//!
//! * **Draw only on change.** A still image draws once. A slideshow draws
//!   when the image changes. An animated image draws when its next frame is
//!   due. Shaders draw at the frame cap. See [`pacing`].
//! * **Paused means zero work.** A paused slot has no due time, and when no
//!   slot is due the render thread blocks on its command channel.
//! * **Decode off the render thread, upload once.** [`content`] decodes
//!   and shrinks images to the screen size on the caller's thread. Animated
//!   images that fit the budget live on the GPU as a texture array, so
//!   playing them costs no uploads.
//! * **Stop frees everything.** Clearing a slot drops its scene; detaching
//!   the last slot closes the GPU device.

mod content;
mod error;
pub mod fit;
mod gpu;
pub mod pacing;
mod player;
mod runloop;
mod scene;
pub mod shader;
mod target;
pub mod video;

pub use content::{
    AnimFormat, AnimatedContent, AnimationBudget, Content, DEFAULT_SHADER_FPS, Frames, LoadOptions,
    Look, ShaderContent, SlideshowContent, StreamSource, decode_still, load_file, load_package,
    load_still, parse_hex_color,
};
pub use error::RenderError;
pub use gpu::{AdapterSummary, Gpu, GpuOptions};
pub use player::{FrameStats, Player};
pub use runloop::{LoopStats, RenderLoop, SlotId, SlotStats, TargetSpec, sleep_until};
pub use scene::Scene;
pub use target::{OFFSCREEN_FORMAT, Offscreen, Target, WindowHandles, WindowSurface};

pub use raw_window_handle;
