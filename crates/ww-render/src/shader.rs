//! Shader wallpapers: WGSL, or GLSL written the Shadertoy way.
//!
//! Both languages use the same contract. The wallpaper defines
//! `mainImage`, gets the pixel coordinate with the origin at the bottom
//! left (as on Shadertoy), and returns a color:
//!
//! ```glsl
//! void mainImage(out vec4 fragColor, in vec2 fragCoord) {
//!     vec2 uv = fragCoord / iResolution.xy;
//!     fragColor = vec4(uv, 0.5 + 0.5 * sin(iTime), 1.0);
//! }
//! ```
//!
//! ```wgsl
//! fn mainImage(fragCoord: vec2<f32>) -> vec4<f32> {
//!     let uv = fragCoord / ww.iResolution.xy;
//!     return vec4<f32>(uv, 0.5 + 0.5 * sin(ww.iTime), 1.0);
//! }
//! ```
//!
//! Uniforms: `iResolution` (pixels), `iTime` and `iTimeDelta` (seconds),
//! `iFrame`, `iFrameRate`, `iMouse` (pixels; all zero unless the user
//! granted mouse access) and `iDate` (year, month 0-11, day, seconds into
//! the day). In WGSL they are fields of `ww`. Texture inputs (`iChannel0`
//! and friends) are not supported yet.

use naga::valid::{Capabilities, ValidationFlags, Validator};

use crate::RenderError;

/// Shader source language.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShaderLang {
    /// WGSL with a `fn mainImage(fragCoord: vec2<f32>) -> vec4<f32>`.
    Wgsl,
    /// GLSL with `void mainImage(out vec4, in vec2)`.
    Glsl,
}

/// Size of the uniform block, in bytes.
pub const GLOBALS_SIZE: u64 = 64;

/// The uniform block every shader sees. Layout matches std140 and WGSL.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Globals {
    /// Width, height and pixel aspect (always 1).
    pub resolution: [f32; 3],
    /// Seconds since the wallpaper started.
    pub time: f32,
    /// Pointer: x, y, and the click position (z, w) per Shadertoy; zero
    /// unless mouse input is granted.
    pub mouse: [f32; 4],
    /// Year, month (0-11), day of month, seconds since midnight.
    pub date: [f32; 4],
    /// Seconds since the previous frame.
    pub time_delta: f32,
    /// Frame counter.
    pub frame: i32,
    /// Current frame rate cap.
    pub frame_rate: f32,
    /// Padding to 64 bytes.
    pub pad: f32,
}

const WGSL_PRELUDE: &str = "struct WwGlobals {
    iResolution: vec3<f32>,
    iTime: f32,
    iMouse: vec4<f32>,
    iDate: vec4<f32>,
    iTimeDelta: f32,
    iFrame: i32,
    iFrameRate: f32,
    wwPad: f32,
}
@group(0) @binding(0) var<uniform> ww: WwGlobals;
";

const WGSL_EPILOGUE: &str = "
@fragment
fn ww_fs(@builtin(position) ww_pos: vec4<f32>) -> @location(0) vec4<f32> {
    let ww_c = mainImage(vec2<f32>(ww_pos.x, ww.iResolution.y - ww_pos.y));
    return vec4<f32>(ww_c.rgb, 1.0);
}
";

const GLSL_PRELUDE: &str = "#version 450
layout(set = 0, binding = 0, std140) uniform WwGlobals {
    vec3 iResolution;
    float iTime;
    vec4 iMouse;
    vec4 iDate;
    float iTimeDelta;
    int iFrame;
    float iFrameRate;
    float wwPad;
};
layout(location = 0) out vec4 wwFragColor;
";

const GLSL_EPILOGUE: &str = "
void main() {
    vec4 wwColor = vec4(0.0);
    mainImage(wwColor, vec2(gl_FragCoord.x, iResolution.y - gl_FragCoord.y));
    wwFragColor = vec4(wwColor.rgb, 1.0);
}
";

/// The fixed vertex stage: one triangle that covers the screen.
pub(crate) const FULLSCREEN_VS: &str = "
@vertex
fn ww_vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let x = f32((i << 1u) & 2u) * 2.0 - 1.0;
    let y = f32(i & 2u) * 2.0 - 1.0;
    return vec4<f32>(x, y, 0.0, 1.0);
}
";

/// Validated naga modules for both stages.
#[derive(Debug)]
pub struct ShaderModules {
    /// Fragment stage; entry point `ww_fs` (WGSL) or `main` (GLSL).
    pub fragment: naga::Module,
    /// Entry point name of the fragment stage.
    pub fragment_entry: &'static str,
    /// Vertex stage; entry point `ww_vs`.
    pub vertex: naga::Module,
}

impl ShaderModules {
    /// Wrap, parse and validate a wallpaper's shader. Errors point at the
    /// line in the wallpaper's own file.
    pub fn compile(lang: ShaderLang, user: &str) -> Result<ShaderModules, RenderError> {
        if user.contains("iChannel") {
            return Err(RenderError::Shader(
                "this shader reads iChannel textures, which Wallweaver doesn't support yet".into(),
            ));
        }
        let (fragment, fragment_entry) = match lang {
            ShaderLang::Wgsl => {
                let src = format!("{WGSL_PRELUDE}{user}\n{WGSL_EPILOGUE}");
                let offset = WGSL_PRELUDE.lines().count();
                let module = naga::front::wgsl::parse_str(&src).map_err(|e| {
                    let line = e.location(&src).map(|l| l.line_number as usize);
                    RenderError::Shader(describe(line, offset, user, e.message()))
                })?;
                (module, "ww_fs")
            }
            ShaderLang::Glsl => {
                let body = clean_glsl(user);
                let src = format!("{GLSL_PRELUDE}{body}\n{GLSL_EPILOGUE}");
                let offset = GLSL_PRELUDE.lines().count();
                let mut frontend = naga::front::glsl::Frontend::default();
                let opts = naga::front::glsl::Options::from(naga::ShaderStage::Fragment);
                let module = frontend.parse(&opts, &src).map_err(|e| {
                    let first = e.errors.first();
                    let line = first.and_then(|err| {
                        let loc = err.meta.location(&src);
                        (loc.line_number > 0).then_some(loc.line_number as usize)
                    });
                    let msg = first
                        .map(|err| err.kind.to_string())
                        .unwrap_or_else(|| "could not parse".into());
                    RenderError::Shader(describe(line, offset, user, &msg))
                })?;
                if !module
                    .functions
                    .iter()
                    .any(|(_, f)| f.name.as_deref() == Some("mainImage"))
                {
                    return Err(RenderError::Shader(
                        "a GLSL wallpaper needs a mainImage(out vec4 fragColor, in vec2 fragCoord) function".into(),
                    ));
                }
                (module, "main")
            }
        };
        validate(&fragment)?;
        let vertex = naga::front::wgsl::parse_str(FULLSCREEN_VS)
            .map_err(|e| RenderError::Shader(format!("internal vertex shader: {e}")))?;
        validate(&vertex)?;
        Ok(ShaderModules {
            fragment,
            fragment_entry,
            vertex,
        })
    }
}

fn validate(module: &naga::Module) -> Result<(), RenderError> {
    Validator::new(ValidationFlags::all(), Capabilities::empty())
        .validate(module)
        .map(|_| ())
        .map_err(|e| RenderError::Shader(format!("the shader is not valid: {}", e.as_inner())))
}

/// Remove lines the wrapper provides itself, keeping line numbers intact.
fn clean_glsl(user: &str) -> String {
    user.lines()
        .map(|l| {
            let t = l.trim_start();
            if t.starts_with("#version") || t.starts_with("precision ") {
                ""
            } else {
                l
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn describe(line: Option<usize>, offset: usize, user: &str, msg: &str) -> String {
    let user_lines = user.lines().count();
    match line {
        Some(l) if l > offset && l - offset <= user_lines => {
            format!("line {}: {msg}", l - offset)
        }
        _ => msg.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GLSL_OK: &str = "void mainImage(out vec4 fragColor, in vec2 fragCoord) {
    vec2 uv = fragCoord / iResolution.xy;
    fragColor = vec4(uv, 0.5 + 0.5 * sin(iTime), 1.0);
}";

    const WGSL_OK: &str = "fn mainImage(fragCoord: vec2<f32>) -> vec4<f32> {
    let uv = fragCoord / ww.iResolution.xy;
    return vec4<f32>(uv, 0.5 + 0.5 * sin(ww.iTime), 1.0);
}";

    #[test]
    fn globals_layout_is_64_bytes() {
        assert_eq!(std::mem::size_of::<Globals>() as u64, GLOBALS_SIZE);
    }

    #[test]
    fn compiles_shadertoy_glsl() {
        let m = ShaderModules::compile(ShaderLang::Glsl, GLSL_OK).unwrap();
        assert_eq!(m.fragment_entry, "main");
        // #version and precision lines from pasted shaders are tolerated.
        let pasted = format!("#version 300 es\nprecision highp float;\n{GLSL_OK}");
        ShaderModules::compile(ShaderLang::Glsl, &pasted).unwrap();
    }

    #[test]
    fn compiles_wgsl() {
        let m = ShaderModules::compile(ShaderLang::Wgsl, WGSL_OK).unwrap();
        assert_eq!(m.fragment_entry, "ww_fs");
    }

    #[test]
    fn errors_point_at_the_users_line() {
        let bad = "fn mainImage(fragCoord: vec2<f32>) -> vec4<f32> {\n    let x = ;\n    return vec4<f32>(1.0);\n}";
        let err = ShaderModules::compile(ShaderLang::Wgsl, bad)
            .unwrap_err()
            .to_string();
        assert!(err.contains("line 2"), "{err}");

        let bad =
            "void mainImage(out vec4 fragColor, in vec2 fragCoord) {\n    fragColor = vec4(1.0)\n}";
        let err = ShaderModules::compile(ShaderLang::Glsl, bad)
            .unwrap_err()
            .to_string();
        assert!(err.starts_with("shader error"), "{err}");
    }

    #[test]
    fn missing_main_image_and_channels_are_explained() {
        let err = ShaderModules::compile(ShaderLang::Glsl, "void helper() {}")
            .unwrap_err()
            .to_string();
        assert!(err.contains("mainImage"), "{err}");
        let err = ShaderModules::compile(
            ShaderLang::Glsl,
            "void mainImage(out vec4 c, in vec2 p) { c = texture(iChannel0, p); }",
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("iChannel"), "{err}");
    }
}
