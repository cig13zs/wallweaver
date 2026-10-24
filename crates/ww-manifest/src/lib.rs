//! Types and validation for `wallpaper.json`, the manifest at the root of
//! every Wallweaver package.
//!
//! Parsing is strict on purpose: unknown fields are rejected, and every value
//! that ends up in a file path, a CSP header or a consent screen is checked
//! before the rest of the app ever sees it. Use [`Manifest::from_json`] to
//! parse and validate in one step.
//!
//! ```
//! let json = r#"{
//!   "manifestVersion": 1,
//!   "id": "mira.aurora-loom",
//!   "name": "Aurora Loom",
//!   "version": "1.2.0",
//!   "type": "web",
//!   "entry": "index.html",
//!   "author": { "name": "mira" },
//!   "license": "MIT",
//!   "permissions": { "audio": true }
//! }"#;
//! let manifest = ww_manifest::Manifest::from_json(json).unwrap();
//! assert!(manifest.permissions.audio);
//! ```

mod error;
mod origin;
pub mod path;
mod schema;
mod validate;

use indexmap::IndexMap;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub use error::{Issue, ManifestError};
pub use origin::{NetworkOrigin, OriginError};
pub use schema::{SCHEMA_ID, json_schema, json_schema_string};

/// The only manifest version this crate understands.
pub const MANIFEST_VERSION: u32 = 1;

/// File name of the manifest inside a package.
pub const MANIFEST_FILE: &str = "wallpaper.json";

/// A parsed `wallpaper.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[schemars(
    title = "Wallweaver wallpaper.json",
    description = "Manifest for a Wallweaver wallpaper or theme package (schema v1)."
)]
pub struct Manifest {
    /// Optional link to this schema, for editor completion. Ignored by Wallweaver.
    #[serde(rename = "$schema", default, skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,

    /// Manifest format version. Must be 1.
    #[schemars(range(min = 1, max = 1))]
    pub manifest_version: u32,

    /// Package id: lowercase letters, digits, dots and dashes, 3 to 64 characters.
    /// A `creator.name` style id is recommended, for example `mira.aurora-loom`.
    #[schemars(
        regex(pattern = r"^[a-z0-9]+([.-][a-z0-9]+)*$"),
        length(min = 3, max = 64)
    )]
    pub id: String,

    /// Display name shown in the gallery and library.
    #[schemars(length(min = 1, max = 80))]
    pub name: String,

    /// Semantic version of the package, for example `1.2.0`.
    #[schemars(with = "String")]
    pub version: semver::Version,

    /// What kind of package this is.
    #[serde(rename = "type")]
    pub kind: WallpaperType,

    /// Path of the file Wallweaver loads first, relative to the package root.
    #[schemars(length(min = 1, max = 255))]
    pub entry: String,

    /// Who made the package.
    pub author: Author,

    /// SPDX license expression, for example `MIT` or `CC-BY-4.0`.
    /// Use `LicenseRef-<name>` for a custom license.
    #[schemars(length(min = 1, max = 128))]
    pub license: String,

    /// One or two sentences describing the wallpaper.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(length(max = 500))]
    pub description: Option<String>,

    /// Up to 10 lowercase tags such as `space` or `audio-reactive`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[schemars(length(max = 10))]
    pub tags: Vec<String>,

    /// Thumbnail image (png, jpg or webp), relative to the package root.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preview: Option<String>,

    /// User-adjustable settings, rendered by the app and delivered to the
    /// wallpaper through `wallweaver.onProperties()`. Order is kept.
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub properties: IndexMap<String, Property>,

    /// What the package asks to use. Anything not listed is denied.
    #[serde(default)]
    pub permissions: Permissions,

    /// Images and timing for a `slideshow` package. Not allowed on other types.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slideshow: Option<Slideshow>,

    /// How the native renderer should draw the wallpaper. Every field is
    /// optional and the defaults suit most wallpapers.
    #[serde(default, skip_serializing_if = "Render::is_default")]
    pub render: Render,
}

/// Images and timing for a slideshow.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Slideshow {
    /// Image paths in display order. `entry` must be one of them; it is shown
    /// first and used as the still snapshot.
    #[schemars(length(min = 1, max = 1000))]
    pub images: Vec<String>,
    /// Seconds each image stays up, 5 to 86400. Defaults to 300.
    #[serde(default = "Slideshow::default_interval")]
    #[schemars(range(min = 5, max = 86400))]
    pub interval: u32,
    /// Show the images in random order.
    #[serde(default, skip_serializing_if = "is_false")]
    pub shuffle: bool,
}

impl Slideshow {
    /// Default seconds per image.
    pub const DEFAULT_INTERVAL: u32 = 300;

    fn default_interval() -> u32 {
        Self::DEFAULT_INTERVAL
    }
}

/// Rendering hints.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Render {
    /// How an image that doesn't match the screen's shape is fitted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fit: Option<Fit>,
    /// Highest frame rate the wallpaper needs, 1 to 240. Animated images use
    /// their own frame timing up to this cap; shaders default to 30.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 1, max = 240))]
    pub max_fps: Option<u32>,
    /// Color behind letterboxed images, `#RRGGBB`. Defaults to black.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background: Option<String>,
}

impl Render {
    fn is_default(&self) -> bool {
        self == &Render::default()
    }
}

/// How an image is fitted to the screen.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Fit {
    /// Fill the screen and crop the overflow. The default.
    #[default]
    Cover,
    /// Show the whole image, letterboxed.
    Contain,
    /// Stretch to the screen, ignoring the aspect ratio.
    Stretch,
    /// Draw at native size, centered.
    Center,
}

/// Package type. Everything except `web` is drawn by Wallweaver's native
/// renderer, so no browser engine is loaded for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum WallpaperType {
    /// A still image (png, jpg or webp).
    Image,
    /// Several still images shown in turn. List them in `slideshow.images`.
    Slideshow,
    /// An animated image: GIF, animated WebP or APNG.
    Animated,
    /// A fragment shader: WGSL, or GLSL written the Shadertoy way
    /// (`mainImage(out vec4, in vec2)` with `iTime`, `iResolution`, `iMouse`).
    Shader,
    /// A looping video file (mp4 or webm).
    Video,
    /// HTML, CSS and JavaScript (including WebGL and WebGPU). The only type
    /// that runs in a web view.
    Web,
    /// Color tokens, fonts and optional wallpaper bindings.
    Theme,
}

/// Extensions of still images the native renderer decodes.
pub const IMAGE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "webp"];

impl WallpaperType {
    /// Every type, in manifest order.
    pub const ALL: [WallpaperType; 7] = [
        WallpaperType::Image,
        WallpaperType::Slideshow,
        WallpaperType::Animated,
        WallpaperType::Shader,
        WallpaperType::Video,
        WallpaperType::Web,
        WallpaperType::Theme,
    ];

    /// File extensions accepted for the `entry` file of this type.
    pub fn entry_extensions(self) -> &'static [&'static str] {
        match self {
            WallpaperType::Image | WallpaperType::Slideshow => IMAGE_EXTENSIONS,
            WallpaperType::Animated => &["gif", "webp", "png", "apng"],
            WallpaperType::Shader => &["wgsl", "frag", "glsl"],
            WallpaperType::Video => &["mp4", "webm"],
            WallpaperType::Web => &["html", "htm"],
            WallpaperType::Theme => &["json"],
        }
    }

    /// Lowercase name as it appears in the manifest.
    pub fn as_str(self) -> &'static str {
        match self {
            WallpaperType::Image => "image",
            WallpaperType::Slideshow => "slideshow",
            WallpaperType::Animated => "animated",
            WallpaperType::Shader => "shader",
            WallpaperType::Video => "video",
            WallpaperType::Web => "web",
            WallpaperType::Theme => "theme",
        }
    }

    /// True if this type needs a web view. Only `web` does.
    pub fn needs_webview(self) -> bool {
        self == WallpaperType::Web
    }

    /// Guess the type for a single dropped file from its extension, for
    /// "drop any file and it's a wallpaper". `png` is treated as a still
    /// image; an APNG is detected later by the decoder.
    pub fn guess_from_file_name(name: &str) -> Option<WallpaperType> {
        let ext = path::extension(name)?;
        let ext = ext.as_str();
        Some(match ext {
            "png" | "jpg" | "jpeg" => WallpaperType::Image,
            "webp" => WallpaperType::Image,
            "gif" | "apng" => WallpaperType::Animated,
            "wgsl" | "frag" | "glsl" => WallpaperType::Shader,
            "mp4" | "webm" => WallpaperType::Video,
            "html" | "htm" => WallpaperType::Web,
            _ => return None,
        })
    }
}

impl std::fmt::Display for WallpaperType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Package author.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Author {
    /// Name or handle.
    #[schemars(length(min = 1, max = 80))]
    pub name: String,
    /// Optional https link to the author's page.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

/// A user-adjustable property.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "lowercase", deny_unknown_fields)]
pub enum Property {
    /// A number picked on a slider.
    Slider {
        /// Label shown next to the control.
        label: String,
        /// Starting value. Must be between `min` and `max`.
        default: f64,
        /// Smallest allowed value.
        min: f64,
        /// Largest allowed value.
        max: f64,
        /// Step between values. Defaults to a smooth slider.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        step: Option<f64>,
    },
    /// A color, as `#RRGGBB` or `#RRGGBBAA`.
    Color {
        /// Label shown next to the control.
        label: String,
        /// Starting color.
        default: String,
    },
    /// An on/off toggle.
    Bool {
        /// Label shown next to the control.
        label: String,
        /// Starting value.
        default: bool,
    },
    /// One choice from a fixed list.
    Combo {
        /// Label shown next to the control.
        label: String,
        /// The `value` of the option selected at first.
        default: String,
        /// The choices, in display order.
        options: Vec<ComboOption>,
    },
}

impl Property {
    /// The property's label.
    pub fn label(&self) -> &str {
        match self {
            Property::Slider { label, .. }
            | Property::Color { label, .. }
            | Property::Bool { label, .. }
            | Property::Combo { label, .. } => label,
        }
    }

    /// The property's default value as JSON, the form wallpapers receive it in.
    pub fn default_value(&self) -> serde_json::Value {
        match self {
            Property::Slider { default, .. } => serde_json::json!(default),
            Property::Color { default, .. } | Property::Combo { default, .. } => {
                serde_json::json!(default)
            }
            Property::Bool { default, .. } => serde_json::json!(default),
        }
    }
}

/// One entry in a combo property.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ComboOption {
    /// Value delivered to the wallpaper.
    pub value: String,
    /// Text shown to the user.
    pub label: String,
}

/// Permissions a package asks for. Everything defaults to denied, and the
/// user still has to approve each one on the consent screen.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Permissions {
    /// Audio levels of what is playing (64 frequency bars). Never the
    /// microphone and never raw audio.
    #[serde(default, skip_serializing_if = "is_false")]
    pub audio: bool,

    /// System usage figures the package wants, refreshed once a second.
    #[serde(
        rename = "system.stats",
        default,
        skip_serializing_if = "Vec::is_empty"
    )]
    pub system_stats: Vec<StatKind>,

    /// City-level weather supplied by Wallweaver's weather plugin.
    #[serde(default, skip_serializing_if = "is_false")]
    pub weather: bool,

    /// Upcoming calendar events from a calendar the user picks.
    #[serde(default, skip_serializing_if = "is_false")]
    pub calendar: bool,

    /// Origins the package may contact directly, such as
    /// `https://api.open-meteo.com`. https and wss only, no paths, no
    /// wildcards, no local network addresses. At most 8.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[schemars(length(max = 8))]
    pub network: Vec<String>,

    /// Pointer position while the desktop is visible. No clicks or keys.
    #[serde(rename = "input.mouse", default, skip_serializing_if = "is_false")]
    pub input_mouse: bool,

    /// Allow `eval()` and `new Function()`. Avoid it if you can; the consent
    /// screen marks it as a risk.
    #[serde(rename = "script.eval", default, skip_serializing_if = "is_false")]
    pub script_eval: bool,
}

impl Permissions {
    /// True when the package asks for nothing at all.
    pub fn is_empty(&self) -> bool {
        self == &Permissions::default()
    }

    /// The declared network origins that parse cleanly, normalized.
    /// Validation guarantees all of them parse for a validated manifest.
    pub fn network_origins(&self) -> Vec<NetworkOrigin> {
        self.network
            .iter()
            .filter_map(|s| NetworkOrigin::parse(s).ok())
            .collect()
    }
}

/// A system statistic a package can ask for.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "lowercase")]
pub enum StatKind {
    /// Total CPU usage in percent.
    Cpu,
    /// GPU usage in percent.
    Gpu,
    /// Memory usage in percent.
    Ram,
    /// Network throughput in bytes per second (no addresses, no content).
    Net,
}

impl StatKind {
    /// Lowercase name as it appears in the manifest.
    pub fn as_str(self) -> &'static str {
        match self {
            StatKind::Cpu => "cpu",
            StatKind::Gpu => "gpu",
            StatKind::Ram => "ram",
            StatKind::Net => "net",
        }
    }
}

fn is_false(b: &bool) -> bool {
    !*b
}

impl Manifest {
    /// Parse and validate a manifest from JSON text.
    pub fn from_json(text: &str) -> Result<Manifest, ManifestError> {
        let manifest = Self::parse_unvalidated(text)?;
        manifest.validate()?;
        Ok(manifest)
    }

    /// Parse and validate a manifest from raw bytes (must be UTF-8).
    pub fn from_slice(bytes: &[u8]) -> Result<Manifest, ManifestError> {
        let text = std::str::from_utf8(bytes).map_err(|_| ManifestError::NotUtf8)?;
        Self::from_json(text)
    }

    /// Parse without running the semantic checks. Only useful for tooling
    /// that wants to show every problem at once; anything that loads a
    /// package must use [`Manifest::from_json`].
    pub fn parse_unvalidated(text: &str) -> Result<Manifest, ManifestError> {
        let text = text.strip_prefix('\u{feff}').unwrap_or(text);
        let de = &mut serde_json::Deserializer::from_str(text);
        serde_path_to_error::deserialize(de).map_err(ManifestError::from_serde)
    }

    /// Run every semantic check and return all problems found.
    pub fn validate(&self) -> Result<(), ManifestError> {
        let issues = validate::check(self);
        if issues.is_empty() {
            Ok(())
        } else {
            Err(ManifestError::Invalid(issues))
        }
    }

    /// Default values for all properties, keyed by property name.
    pub fn default_properties(&self) -> serde_json::Map<String, serde_json::Value> {
        self.properties
            .iter()
            .map(|(k, p)| (k.clone(), p.default_value()))
            .collect()
    }

    /// Build a manifest for a single file the user dropped onto Wallweaver,
    /// so any image, GIF, shader, video or web page works without writing
    /// `wallpaper.json` by hand. Returns `None` for file types Wallweaver
    /// can't show. The result is validated.
    pub fn for_local_file(file_name: &str) -> Option<Manifest> {
        let kind = WallpaperType::guess_from_file_name(file_name)?;
        let stem = file_name
            .rsplit('/')
            .next()
            .and_then(|n| n.rsplit_once('.').map(|(s, _)| s))
            .unwrap_or("wallpaper");
        let slug: String = stem
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() {
                    c.to_ascii_lowercase()
                } else {
                    '-'
                }
            })
            .collect::<String>()
            .split('-')
            .filter(|p| !p.is_empty())
            .collect::<Vec<_>>()
            .join("-");
        let slug = if slug.is_empty() {
            "wallpaper".into()
        } else {
            slug
        };
        let mut id = format!("local.{slug}");
        id.truncate(64);
        let id = id.trim_end_matches(['-', '.']).to_string();
        let name: String = stem.chars().filter(|c| !c.is_control()).take(80).collect();
        let manifest = Manifest {
            schema: None,
            manifest_version: MANIFEST_VERSION,
            id,
            name: if name.trim().is_empty() {
                "Wallpaper".into()
            } else {
                name
            },
            version: semver::Version::new(0, 0, 0),
            kind,
            entry: file_name.to_string(),
            author: Author {
                name: "me".into(),
                url: None,
            },
            license: "LicenseRef-Personal".into(),
            description: None,
            tags: Vec::new(),
            preview: None,
            properties: IndexMap::new(),
            permissions: Permissions::default(),
            slideshow: None,
            render: Render::default(),
        };
        manifest.validate().ok()?;
        Some(manifest)
    }

    /// Every package file the manifest points at: `entry`, `preview` and
    /// slideshow images, without duplicates, as `(field, path)` pairs.
    pub fn referenced_files(&self) -> Vec<(&'static str, &str)> {
        let mut out = vec![("entry", self.entry.as_str())];
        if let Some(p) = &self.preview {
            out.push(("preview", p.as_str()));
        }
        if let Some(show) = &self.slideshow {
            for img in &show.images {
                if !out.iter().any(|(_, p)| *p == img.as_str()) {
                    out.push(("slideshow.images", img.as_str()));
                }
            }
        }
        out
    }

    /// Serialize back to pretty JSON.
    pub fn to_json_pretty(&self) -> String {
        // Serializing these plain types cannot fail.
        serde_json::to_string_pretty(self).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests;
