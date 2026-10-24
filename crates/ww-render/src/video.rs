//! Video backend interface. Not implemented in this build.
//!
//! The plan, so a backend can be added without touching the rest of the
//! renderer (details in `docs/VIDEO.md`):
//!
//! * Decode in hardware and keep frames on the GPU. The decoder writes into
//!   a texture the renderer samples directly; frames never come back to the
//!   CPU.
//!   * Windows: Media Foundation source reader with a D3D11 device manager,
//!     NV12 output into a shared D3D11 texture, opened in the D3D12 device
//!     through a shared handle.
//!   * macOS: VideoToolbox decompression session producing IOSurface-backed
//!     `CVPixelBuffer`s, wrapped as Metal textures with `CVMetalTextureCache`.
//!   * Linux: VA-API decode to a DMA-BUF, imported into Vulkan as external
//!     memory.
//!   * Fallback everywhere: libmpv's render API, for codecs the platform
//!     decoder lacks.
//! * Convert NV12/P010 to RGB in the quad shader, so no extra copy is made.
//! * Pace by presentation timestamps, capped like any other content. Pausing
//!   pauses the decoder; stopping closes it.

use std::time::Duration;

/// A decoded frame that lives on the GPU.
pub trait VideoFrame: Send {
    /// Presentation time from the start of the clip.
    fn pts(&self) -> Duration;
    /// Size in pixels.
    fn size(&self) -> (u32, u32);
}

/// A hardware video decoder.
pub trait VideoDecoder: Send {
    /// Frame type produced.
    type Frame: VideoFrame;

    /// The next frame, or `None` at the end of the clip.
    fn next_frame(&mut self) -> Option<Self::Frame>;
    /// Jump back to the start (for looping).
    fn rewind(&mut self);
    /// Stop decoding work until [`VideoDecoder::resume`].
    fn pause(&mut self);
    /// Continue after [`VideoDecoder::pause`].
    fn resume(&mut self);
    /// Clip length, if known.
    fn duration(&self) -> Option<Duration>;
}
