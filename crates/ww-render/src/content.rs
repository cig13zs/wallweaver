//! Loading wallpapers into CPU memory, ready to upload. Everything here is
//! meant to run on a loader thread so the render thread never decodes.

use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use image::{AnimationDecoder, ImageDecoder, ImageReader, RgbaImage};
use ww_manifest::{Fit, Manifest, WallpaperType};

use crate::RenderError;
use crate::fit::decode_size;
use crate::shader::{ShaderLang, ShaderModules};

/// Largest image side accepted from a wallpaper file.
pub const MAX_IMAGE_SIDE: u32 = 16_384;
/// Largest allocation the image decoder may make for one file.
pub const MAX_DECODE_ALLOC: u64 = 512 * 1024 * 1024;

/// Memory limits for animated images.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AnimationBudget {
    /// Most bytes of frames kept on the GPU as a texture array. Within this,
    /// playback costs no uploads at all.
    pub gpu_bytes: u64,
    /// Most bytes of decoded frames kept in RAM. Within this (but over the
    /// GPU budget), each new frame is uploaded from RAM. Over it, frames are
    /// decoded on a worker thread as they are needed.
    pub cpu_bytes: u64,
}

impl Default for AnimationBudget {
    fn default() -> Self {
        AnimationBudget {
            gpu_bytes: 96 * 1024 * 1024,
            cpu_bytes: 64 * 1024 * 1024,
        }
    }
}

/// How to load content.
#[derive(Debug, Clone)]
pub struct LoadOptions {
    /// Size of the screen it will be shown on.
    pub target_size: (u32, u32),
    /// Limits for animated images.
    pub budget: AnimationBudget,
    /// Texture array layer limit of the device that will show it.
    pub max_layers: u32,
}

impl LoadOptions {
    /// Defaults for a screen of the given size.
    pub fn for_size(width: u32, height: u32) -> LoadOptions {
        LoadOptions {
            target_size: (width.max(1), height.max(1)),
            budget: AnimationBudget::default(),
            max_layers: 256,
        }
    }
}

/// Appearance settings shared by every image-based backend.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Look {
    /// How the image fits the screen.
    pub fit: Fit,
    /// Letterbox color, linear 0..1 RGBA.
    pub background: [f64; 4],
}

impl Default for Look {
    fn default() -> Self {
        Look {
            fit: Fit::Cover,
            background: [0.0, 0.0, 0.0, 1.0],
        }
    }
}

/// A decoded wallpaper, ready to hand to the renderer.
#[derive(Debug)]
pub enum Content {
    /// One still image.
    Still {
        /// Pixels at the size they will be shown (or smaller).
        image: RgbaImage,
        /// Fit and background.
        look: Look,
    },
    /// Images shown in turn. Only the first is decoded up front.
    Slideshow(SlideshowContent),
    /// An animated image.
    Animated(AnimatedContent),
    /// A fragment shader.
    Shader(Box<ShaderContent>),
}

impl Content {
    /// Short name of the backend that will draw this.
    pub fn kind(&self) -> &'static str {
        match self {
            Content::Still { .. } => "image",
            Content::Slideshow(_) => "slideshow",
            Content::Animated(_) => "animated",
            Content::Shader(_) => "shader",
        }
    }
}

/// Slideshow content.
#[derive(Debug)]
pub struct SlideshowContent {
    /// Absolute paths of every image, in display order.
    pub images: Vec<PathBuf>,
    /// Time each image stays up.
    pub interval: Duration,
    /// Randomize the order.
    pub shuffle: bool,
    /// The first image, already decoded.
    pub first: RgbaImage,
    /// Fit and background.
    pub look: Look,
    /// Screen size, for decoding the rest.
    pub target_size: (u32, u32),
}

/// Animated image content.
#[derive(Debug)]
pub struct AnimatedContent {
    /// The frames, or how to get them.
    pub frames: Frames,
    /// Frame size.
    pub size: (u32, u32),
    /// Fit and background.
    pub look: Look,
    /// Frame rate cap from the manifest, if any.
    pub max_fps: Option<u32>,
}

/// Where an animation's frames live.
#[derive(Debug)]
pub enum Frames {
    /// Every frame decoded. `gpu_resident` says whether they fit the GPU
    /// budget and layer limit (uploaded once as a texture array) or are
    /// uploaded one by one from RAM.
    Decoded {
        /// The frames.
        images: Vec<RgbaImage>,
        /// How long each is shown.
        delays: Vec<Duration>,
        /// Upload all at once as a texture array.
        gpu_resident: bool,
    },
    /// Too big to keep: decoded on a worker thread while playing.
    Streamed(StreamSource),
}

/// What a streaming worker needs to decode an animation again and again.
#[derive(Debug, Clone)]
pub struct StreamSource {
    /// The encoded file.
    pub bytes: Arc<[u8]>,
    /// Its format.
    pub format: AnimFormat,
    /// Size to scale frames to.
    pub size: (u32, u32),
}

/// Animated image formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnimFormat {
    /// GIF.
    Gif,
    /// Animated WebP.
    WebP,
    /// Animated PNG.
    Apng,
}

/// A compiled shader.
#[derive(Debug)]
pub struct ShaderContent {
    /// Validated modules.
    pub modules: ShaderModules,
    /// Frame rate cap from the manifest, defaulting to 30.
    pub max_fps: u32,
}

/// Default frame cap for shaders when the manifest doesn't set one.
pub const DEFAULT_SHADER_FPS: u32 = 30;

/// Load a validated package from its unpacked folder.
pub fn load_package(
    manifest: &Manifest,
    root: &Path,
    opts: &LoadOptions,
) -> Result<Content, RenderError> {
    let look = Look {
        fit: manifest.render.fit.unwrap_or_default(),
        background: manifest
            .render
            .background
            .as_deref()
            .and_then(parse_hex_color)
            .unwrap_or([0.0, 0.0, 0.0, 1.0]),
    };
    let entry = package_path(root, &manifest.entry);
    match manifest.kind {
        WallpaperType::Slideshow => {
            let show = manifest.slideshow.as_ref().ok_or_else(|| {
                RenderError::Unsupported("slideshow package without a slideshow section".into())
            })?;
            let images: Vec<PathBuf> = show.images.iter().map(|i| package_path(root, i)).collect();
            let first = load_still(&entry, look.fit, opts.target_size)?;
            Ok(Content::Slideshow(SlideshowContent {
                images,
                interval: Duration::from_secs(u64::from(show.interval)),
                shuffle: show.shuffle,
                first,
                look,
                target_size: opts.target_size,
            }))
        }
        kind => load_kind(kind, &entry, look, manifest.render.max_fps, opts),
    }
}

/// Load any single file the user dropped in, guessing the type from its
/// extension. File names are not restricted the way package paths are.
pub fn load_file(path: &Path, opts: &LoadOptions) -> Result<Content, RenderError> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .unwrap_or_default();
    let kind = WallpaperType::guess_from_file_name(&format!("file.{ext}")).ok_or_else(|| {
        RenderError::Unsupported(format!(
            "Wallweaver can't show .{ext} files yet; try an image, GIF, WebP, APNG or shader"
        ))
    })?;
    load_kind(kind, path, Look::default(), None, opts)
}

fn load_kind(
    kind: WallpaperType,
    path: &Path,
    look: Look,
    max_fps: Option<u32>,
    opts: &LoadOptions,
) -> Result<Content, RenderError> {
    match kind {
        WallpaperType::Image | WallpaperType::Animated => {
            let bytes = read(path)?;
            if let Some(format) = animation_format(&bytes)? {
                return load_animated(bytes, format, look, max_fps, opts, &display(path));
            }
            let image = decode_still(&bytes, look.fit, opts.target_size, &display(path))?;
            Ok(Content::Still { image, look })
        }
        WallpaperType::Shader => {
            let code = std::fs::read_to_string(path).map_err(|source| RenderError::Io {
                path: path.to_path_buf(),
                source,
            })?;
            let lang = match path.extension().and_then(|e| e.to_str()) {
                Some(e) if e.eq_ignore_ascii_case("wgsl") => ShaderLang::Wgsl,
                _ => ShaderLang::Glsl,
            };
            let modules = ShaderModules::compile(lang, &code)?;
            Ok(Content::Shader(Box::new(ShaderContent {
                modules,
                max_fps: max_fps.unwrap_or(DEFAULT_SHADER_FPS),
            })))
        }
        WallpaperType::Slideshow => Err(RenderError::Unsupported(
            "a slideshow needs its package manifest".into(),
        )),
        WallpaperType::Video => Err(RenderError::Unsupported(
            "video wallpapers are not in this build yet; the hardware-decode backend is planned (see docs/VIDEO.md)".into(),
        )),
        WallpaperType::Web => Err(RenderError::Unsupported(
            "web wallpapers run in the web view host, not the native renderer".into(),
        )),
        WallpaperType::Theme => Err(RenderError::Unsupported("themes are not drawn".into())),
    }
}

fn package_path(root: &Path, rel: &str) -> PathBuf {
    let mut p = root.to_path_buf();
    p.extend(rel.split('/'));
    p
}

fn display(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

fn read(path: &Path) -> Result<Vec<u8>, RenderError> {
    std::fs::read(path).map_err(|source| RenderError::Io {
        path: path.to_path_buf(),
        source,
    })
}

fn limits() -> image::Limits {
    let mut l = image::Limits::default();
    l.max_image_width = Some(MAX_IMAGE_SIDE);
    l.max_image_height = Some(MAX_IMAGE_SIDE);
    l.max_alloc = Some(MAX_DECODE_ALLOC);
    l
}

/// Decode a still image file and shrink it to what the screen shows.
pub fn load_still(path: &Path, fit: Fit, target: (u32, u32)) -> Result<RgbaImage, RenderError> {
    decode_still(&read(path)?, fit, target, &display(path))
}

/// Decode still image bytes and shrink them to what the screen shows.
pub fn decode_still(
    bytes: &[u8],
    fit: Fit,
    target: (u32, u32),
    what: &str,
) -> Result<RgbaImage, RenderError> {
    let err = |e: image::ImageError| RenderError::Decode {
        what: what.to_string(),
        message: e.to_string(),
    };
    let mut reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| RenderError::Decode {
            what: what.to_string(),
            message: e.to_string(),
        })?;
    reader.limits(limits());
    let img = reader.decode().map_err(err)?.into_rgba8();
    Ok(shrink(img, fit, target))
}

fn shrink(img: RgbaImage, fit: Fit, target: (u32, u32)) -> RgbaImage {
    let (w, h) = decode_size(fit, img.dimensions(), target);
    if (w, h) == img.dimensions() {
        img
    } else {
        image::imageops::resize(&img, w, h, image::imageops::FilterType::Triangle)
    }
}

/// Detect animated GIF, WebP or PNG. Still WebP and PNG return `None`; a GIF
/// counts as animated even with one frame (the animated backend handles that
/// as a still).
fn animation_format(bytes: &[u8]) -> Result<Option<AnimFormat>, RenderError> {
    if bytes.starts_with(b"GIF8") {
        return Ok(Some(AnimFormat::Gif));
    }
    if bytes.len() > 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        let dec = image::codecs::webp::WebPDecoder::new(Cursor::new(bytes)).map_err(|e| {
            RenderError::Decode {
                what: "WebP".into(),
                message: e.to_string(),
            }
        })?;
        return Ok(dec.has_animation().then_some(AnimFormat::WebP));
    }
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        let dec = image::codecs::png::PngDecoder::new(Cursor::new(bytes)).map_err(|e| {
            RenderError::Decode {
                what: "PNG".into(),
                message: e.to_string(),
            }
        })?;
        let apng = dec.is_apng().map_err(|e| RenderError::Decode {
            what: "PNG".into(),
            message: e.to_string(),
        })?;
        return Ok(apng.then_some(AnimFormat::Apng));
    }
    Ok(None)
}

/// Open a frame iterator for an animation.
pub(crate) fn frame_iter(
    bytes: &[u8],
    format: AnimFormat,
) -> Result<image::Frames<'_>, image::ImageError> {
    let cursor = Cursor::new(bytes);
    Ok(match format {
        AnimFormat::Gif => {
            let mut d = image::codecs::gif::GifDecoder::new(cursor)?;
            d.set_limits(limits())?;
            d.into_frames()
        }
        AnimFormat::WebP => {
            let mut d = image::codecs::webp::WebPDecoder::new(cursor)?;
            d.set_limits(limits())?;
            d.into_frames()
        }
        AnimFormat::Apng => {
            let mut d = image::codecs::png::PngDecoder::new(cursor)?;
            d.set_limits(limits())?;
            d.apng()?.into_frames()
        }
    })
}

/// Turn a frame's delay into a duration. GIFs that ask for 0 or 10 ms
/// frames are played at 100 ms, as browsers do; otherwise such files would
/// spin the GPU for no visible benefit.
pub(crate) fn frame_delay(frame: &image::Frame, format: AnimFormat) -> Duration {
    let (num, den) = frame.delay().numer_denom_ms();
    let ms = if den == 0 {
        100
    } else {
        u64::from(num) / u64::from(den)
    };
    let ms = if format == AnimFormat::Gif && ms <= 10 {
        100
    } else {
        ms.max(10)
    };
    Duration::from_millis(ms)
}

fn load_animated(
    bytes: Vec<u8>,
    format: AnimFormat,
    look: Look,
    max_fps: Option<u32>,
    opts: &LoadOptions,
    what: &str,
) -> Result<Content, RenderError> {
    let err = |e: image::ImageError| RenderError::Decode {
        what: what.to_string(),
        message: e.to_string(),
    };
    let mut images = Vec::new();
    let mut delays = Vec::new();
    let mut size = None;
    let mut total: u64 = 0;
    let mut too_big = false;
    for frame in frame_iter(&bytes, format).map_err(err)? {
        let frame = frame.map_err(err)?;
        delays.push(frame_delay(&frame, format));
        let img = shrink(frame.into_buffer(), look.fit, opts.target_size);
        let dims = *size.get_or_insert(img.dimensions());
        let img = if img.dimensions() == dims {
            img
        } else {
            image::imageops::resize(&img, dims.0, dims.1, image::imageops::FilterType::Triangle)
        };
        total += u64::from(dims.0) * u64::from(dims.1) * 4;
        if total > opts.budget.cpu_bytes.max(opts.budget.gpu_bytes) {
            too_big = true;
            images.clear();
            break;
        }
        images.push(img);
    }
    let size = size.ok_or_else(|| RenderError::Decode {
        what: what.to_string(),
        message: "the animation has no frames".into(),
    })?;
    if too_big {
        return Ok(Content::Animated(AnimatedContent {
            frames: Frames::Streamed(StreamSource {
                bytes: bytes.into(),
                format,
                size,
            }),
            size,
            look,
            max_fps,
        }));
    }
    let gpu_resident = total <= opts.budget.gpu_bytes && images.len() as u32 <= opts.max_layers;
    if !gpu_resident && total > opts.budget.cpu_bytes {
        return Ok(Content::Animated(AnimatedContent {
            frames: Frames::Streamed(StreamSource {
                bytes: bytes.into(),
                format,
                size,
            }),
            size,
            look,
            max_fps,
        }));
    }
    Ok(Content::Animated(AnimatedContent {
        frames: Frames::Decoded {
            images,
            delays,
            gpu_resident,
        },
        size,
        look,
        max_fps,
    }))
}

/// Parse `#RRGGBB` into linear-ish 0..1 RGBA (no gamma conversion, matching
/// how the image is drawn).
pub fn parse_hex_color(s: &str) -> Option<[f64; 4]> {
    let hex = s.strip_prefix('#')?;
    if hex.len() != 6 {
        return None;
    }
    let v = u32::from_str_radix(hex, 16).ok()?;
    let c = |shift: u32| f64::from((v >> shift) & 0xff) / 255.0;
    Some([c(16), c(8), c(0), 1.0])
}
