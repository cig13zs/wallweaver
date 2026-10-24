use std::path::PathBuf;

/// Everything that can go wrong loading or drawing a wallpaper.
#[derive(Debug, thiserror::Error)]
pub enum RenderError {
    /// No graphics adapter could be opened.
    #[error("no usable graphics adapter: {0}")]
    NoAdapter(String),
    /// The adapter refused to create a device.
    #[error("could not open the graphics device: {0}")]
    Device(String),
    /// A window surface could not be created or configured.
    #[error("could not draw to the window: {0}")]
    Surface(String),
    /// A file could not be read.
    #[error("could not read {}: {source}", path.display())]
    Io {
        /// The file.
        path: PathBuf,
        /// The OS error.
        source: std::io::Error,
    },
    /// An image or animation could not be decoded.
    #[error("could not decode {what}: {message}")]
    Decode {
        /// Which file.
        what: String,
        /// Decoder message.
        message: String,
    },
    /// A shader failed to compile. The message points at the line in the
    /// wallpaper's own file.
    #[error("shader error: {0}")]
    Shader(String),
    /// The wallpaper type has no native backend yet.
    #[error("{0}")]
    Unsupported(String),
    /// A GPU operation failed after setup.
    #[error("graphics error: {0}")]
    Gpu(String),
}
