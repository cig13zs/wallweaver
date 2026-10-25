//! Headless rendering tests. They need a graphics adapter; a software one
//! (lavapipe, llvmpipe, WARP) is fine. Without any adapter they print a
//! notice and pass, unless `WW_REQUIRE_GPU=1` is set (CI sets it on runners
//! where a software adapter is installed).

#![allow(clippy::unwrap_used)]

use std::path::Path;
use std::time::Duration;

use image::{Delay, Frame, Rgba, RgbaImage};
use ww_manifest::Fit;
use ww_render::shader::{ShaderLang, ShaderModules};
use ww_render::*;

fn gpu() -> Option<Gpu> {
    match Gpu::new(&GpuOptions::for_tests()) {
        Ok(g) => {
            eprintln!("adapter: {}", g.adapter());
            Some(g)
        }
        Err(e) => {
            if std::env::var_os("WW_REQUIRE_GPU").is_some() {
                panic!("WW_REQUIRE_GPU is set but no adapter is available: {e}");
            }
            eprintln!("skipping GPU test: {e}");
            None
        }
    }
}

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

fn offscreen_player(gpu: &Gpu, w: u32, h: u32) -> Player {
    Player::new(Target::Offscreen(Offscreen::new(gpu, w, h)), 60)
}

fn read(gpu: &Gpu, p: &Player) -> RgbaImage {
    match p.target() {
        Target::Offscreen(o) => o.read_rgba(gpu).unwrap(),
        Target::Window(_) => unreachable!(),
    }
}

fn close(a: Rgba<u8>, b: [u8; 4], tol: u8) -> bool {
    a.0.iter().zip(b).all(|(x, y)| x.abs_diff(y) <= tol)
}

fn assert_px(img: &RgbaImage, x: u32, y: u32, want: [u8; 4], tol: u8) {
    let got = *img.get_pixel(x, y);
    assert!(
        close(got, want, tol),
        "pixel ({x},{y}) is {:?}, want {want:?}",
        got.0
    );
}

const RED: [u8; 4] = [255, 0, 0, 255];
const GREEN: [u8; 4] = [0, 255, 0, 255];
const BLUE: [u8; 4] = [0, 0, 255, 255];
const WHITE: [u8; 4] = [255, 255, 255, 255];

fn checker() -> RgbaImage {
    // 4x2: R G B W / W B G R
    let px = [RED, GREEN, BLUE, WHITE, WHITE, BLUE, GREEN, RED];
    RgbaImage::from_fn(4, 2, |x, y| Rgba(px[(y * 4 + x) as usize]))
}

#[test]
fn still_image_pixels_pass_through_exactly() {
    let Some(gpu) = gpu() else { return };
    let mut p = offscreen_player(&gpu, 4, 2);
    let look = Look {
        fit: Fit::Stretch,
        ..Look::default()
    };
    p.set_content(
        &gpu,
        Content::Still {
            image: checker(),
            look,
        },
        Duration::ZERO,
    )
    .unwrap();
    assert!(p.render(&gpu, Duration::ZERO).unwrap());
    let out = read(&gpu, &p);
    assert_eq!(out, checker());
    assert_eq!(p.gpu_bytes(), 4 * 2 * 4);
}

#[test]
fn contain_letterboxes_with_the_background_color() {
    let Some(gpu) = gpu() else { return };
    let mut p = offscreen_player(&gpu, 8, 8);
    let img = RgbaImage::from_pixel(4, 2, Rgba(RED));
    let look = Look {
        fit: Fit::Contain,
        background: parse_hex_color("#0000ff").unwrap(),
    };
    p.set_content(&gpu, Content::Still { image: img, look }, Duration::ZERO)
        .unwrap();
    p.render(&gpu, Duration::ZERO).unwrap();
    let out = read(&gpu, &p);
    // 4x2 scaled to 8x4, centered: rows 0-1 and 6-7 are background.
    assert_px(&out, 4, 0, BLUE, 0);
    assert_px(&out, 4, 7, BLUE, 0);
    assert_px(&out, 0, 3, RED, 0);
    assert_px(&out, 7, 4, RED, 0);
}

#[test]
fn cover_fills_every_pixel() {
    let Some(gpu) = gpu() else { return };
    let mut p = offscreen_player(&gpu, 8, 8);
    let img = RgbaImage::from_pixel(4, 2, Rgba(GREEN));
    let look = Look {
        fit: Fit::Cover,
        background: parse_hex_color("#ff0000").unwrap(),
    };
    p.set_content(&gpu, Content::Still { image: img, look }, Duration::ZERO)
        .unwrap();
    p.render(&gpu, Duration::ZERO).unwrap();
    let out = read(&gpu, &p);
    assert!(out.pixels().all(|px| close(*px, GREEN, 0)));
}

#[test]
fn still_image_draws_once() {
    let Some(gpu) = gpu() else { return };
    let mut p = offscreen_player(&gpu, 4, 2);
    p.set_content(
        &gpu,
        Content::Still {
            image: checker(),
            look: Look::default(),
        },
        Duration::ZERO,
    )
    .unwrap();
    assert_eq!(p.next_due(Duration::ZERO), Some(Duration::ZERO));
    p.render(&gpu, Duration::ZERO).unwrap();
    assert_eq!(
        p.next_due(ms(1)),
        None,
        "nothing to do after the first frame"
    );
    assert!(p.is_static(ms(1)));
}

fn shader(lang: ShaderLang, code: &str) -> Content {
    Content::Shader(Box::new(ShaderContent {
        modules: ShaderModules::compile(lang, code).unwrap(),
        max_fps: 60,
    }))
}

/// Expected gradient value for pixel column/row `i` of `n`, with pixel
/// centers at i + 0.5.
fn grad(i: u32, n: u32) -> u8 {
    (((i as f32 + 0.5) / n as f32) * 255.0).round() as u8
}

#[test]
fn wgsl_shader_matches_the_math() {
    let Some(gpu) = gpu() else { return };
    let (w, h) = (16, 8);
    let mut p = offscreen_player(&gpu, w, h);
    let code = "fn mainImage(fragCoord: vec2<f32>) -> vec4<f32> {
        return vec4<f32>(fragCoord.x / ww.iResolution.x, fragCoord.y / ww.iResolution.y, 0.5, 1.0);
    }";
    p.set_content(&gpu, shader(ShaderLang::Wgsl, code), Duration::ZERO)
        .unwrap();
    p.render(&gpu, Duration::ZERO).unwrap();
    let out = read(&gpu, &p);
    for y in 0..h {
        for x in 0..w {
            // fragCoord.y counts from the bottom, image rows from the top.
            let want = [grad(x, w), grad(h - 1 - y, h), 128, 255];
            assert_px(&out, x, y, want, 1);
        }
    }
}

#[test]
fn shadertoy_glsl_matches_the_math() {
    let Some(gpu) = gpu() else { return };
    let (w, h) = (16, 8);
    let mut p = offscreen_player(&gpu, w, h);
    let code = "#version 300 es
precision highp float;
void mainImage(out vec4 fragColor, in vec2 fragCoord) {
    vec2 uv = fragCoord / iResolution.xy;
    fragColor = vec4(uv, 0.5, 1.0);
}";
    p.set_content(&gpu, shader(ShaderLang::Glsl, code), Duration::ZERO)
        .unwrap();
    p.render(&gpu, Duration::ZERO).unwrap();
    let out = read(&gpu, &p);
    assert_px(&out, 0, h - 1, [grad(0, w), grad(0, h), 128, 255], 1);
    assert_px(
        &out,
        w - 1,
        0,
        [grad(w - 1, w), grad(h - 1, h), 128, 255],
        1,
    );
}

#[test]
fn shader_time_and_mouse_reach_the_shader() {
    let Some(gpu) = gpu() else { return };
    let mut p = offscreen_player(&gpu, 4, 4);
    let code = "void mainImage(out vec4 c, in vec2 p) {
        c = vec4(clamp(iTime / 2.0, 0.0, 1.0), iMouse.x / 100.0, 0.0, 1.0);
    }";
    p.set_content(&gpu, shader(ShaderLang::Glsl, code), Duration::ZERO)
        .unwrap();
    p.render(&gpu, Duration::ZERO).unwrap();
    assert_px(&read(&gpu, &p), 0, 0, [0, 0, 0, 255], 1);
    // The clock only advances while drawing, in steps of at most 250 ms.
    for i in 1..=4 {
        p.render(&gpu, ms(250 * i)).unwrap();
    }
    p.set_mouse(Some([50.0, 0.0, 0.0, 0.0]));
    p.render(&gpu, ms(1250)).unwrap();
    // 1.25 s / 2 = 0.625 → 159; mouse 50/100 → 128.
    assert_px(&read(&gpu, &p), 0, 0, [159, 128, 0, 255], 2);
    // Without the permission the host passes None and the shader sees zero.
    p.set_mouse(None);
    p.render(&gpu, ms(1250)).unwrap();
    assert_eq!(read(&gpu, &p).get_pixel(0, 0).0[1], 0);
}

const PLASMA: &str = "void mainImage(out vec4 fragColor, in vec2 fragCoord) {
    vec2 uv = fragCoord / iResolution.xy;
    float v = sin(uv.x * 10.0 + iTime) + sin((uv.y * 10.0 + iTime) / 2.0)
            + sin((uv.x * 10.0 + uv.y * 10.0 + iTime) / 2.0);
    vec2 c = uv * 10.0 + vec2(sin(iTime / 3.0), cos(iTime / 2.0)) * 5.0;
    v += sin(sqrt(c.x * c.x + c.y * c.y + 1.0) + iTime);
    fragColor = vec4(0.5 + 0.5 * sin(v), 0.5 + 0.5 * sin(v + 2.094), 0.5 + 0.5 * sin(v + 4.188), 1.0);
}";

/// Golden image check for a non-trivial shader. Regenerate with
/// `WW_BLESS=1 cargo test -p ww-render --test render golden`.
#[test]
fn golden_plasma() {
    let Some(gpu) = gpu() else { return };
    let mut p = offscreen_player(&gpu, 64, 36);
    p.set_content(&gpu, shader(ShaderLang::Glsl, PLASMA), Duration::ZERO)
        .unwrap();
    // Advance the clock to exactly 1 s in 250 ms steps.
    for i in 0..=4 {
        p.render(&gpu, ms(250 * i)).unwrap();
    }
    let out = read(&gpu, &p);
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden/plasma-64x36-t1.png");
    if std::env::var_os("WW_BLESS").is_some() {
        out.save(&path).unwrap();
        return;
    }
    let want = image::open(&path).unwrap().into_rgba8();
    assert_eq!(want.dimensions(), out.dimensions());
    // Different GPUs round transcendental functions differently; allow a
    // small per-channel difference.
    let worst = want
        .pixels()
        .zip(out.pixels())
        .flat_map(|(a, b)| {
            a.0.iter()
                .zip(b.0)
                .map(|(x, y)| x.abs_diff(y))
                .collect::<Vec<_>>()
        })
        .max()
        .unwrap_or(0);
    assert!(worst <= 6, "golden image differs by up to {worst}");
}

fn write_gif(path: &Path, colors: &[[u8; 4]], delay_ms: u32, size: u32) {
    let file = std::fs::File::create(path).unwrap();
    let mut enc = image::codecs::gif::GifEncoder::new(file);
    enc.set_repeat(image::codecs::gif::Repeat::Infinite)
        .unwrap();
    let frames = colors.iter().map(|c| {
        Frame::from_parts(
            RgbaImage::from_pixel(size, size, Rgba(*c)),
            0,
            0,
            Delay::from_numer_denom_ms(delay_ms, 1),
        )
    });
    enc.encode_frames(frames).unwrap();
}

fn play_gif(gpu: &Gpu, budget: AnimationBudget, expect_mode: &str) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("rgb.gif");
    write_gif(&path, &[RED, GREEN, BLUE], 100, 8);
    let opts = LoadOptions {
        budget,
        ..LoadOptions::for_size(8, 8)
    };
    let content = load_file(&path, &opts).unwrap();
    assert_eq!(content.kind(), "animated");
    let mut p = offscreen_player(gpu, 8, 8);
    p.set_content(gpu, content, Duration::ZERO).unwrap();
    assert_eq!(p.scene().unwrap().animation_mode(), Some(expect_mode));

    // Drive the player like the render loop would, sampling the color
    // after each frame.
    let mut seen = Vec::new();
    let mut now = Duration::ZERO;
    while now < ms(700) {
        if let Some(due) = p.next_due(now)
            && due <= now
        {
            p.render(gpu, now).unwrap();
            let px = read(gpu, &p).get_pixel(4, 4).0;
            if seen.last() != Some(&px) {
                seen.push(px);
            }
        }
        now += ms(5);
        // Streamed frames come from a worker thread running in real time.
        std::thread::sleep(ms(1));
    }
    // Red, green, blue, then around again.
    assert!(seen.len() >= 6, "{seen:?}");
    let pos = seen.iter().position(|c| *c == RED).unwrap();
    assert_eq!(&seen[pos..pos + 3], &[RED, GREEN, BLUE], "{seen:?}");
}

#[test]
fn gif_plays_from_a_gpu_texture_array() {
    let Some(gpu) = gpu() else { return };
    play_gif(&gpu, AnimationBudget::default(), "gpu-array");
}

#[test]
fn gif_over_gpu_budget_uploads_from_ram() {
    let Some(gpu) = gpu() else { return };
    play_gif(
        &gpu,
        AnimationBudget {
            gpu_bytes: 100,
            cpu_bytes: 1 << 20,
        },
        "cpu-upload",
    );
}

#[test]
fn gif_over_every_budget_is_streamed() {
    let Some(gpu) = gpu() else { return };
    play_gif(
        &gpu,
        AnimationBudget {
            gpu_bytes: 100,
            cpu_bytes: 100,
        },
        "streamed",
    );
}

#[test]
fn big_images_are_shrunk_to_the_screen_before_upload() {
    let Some(gpu) = gpu() else { return };
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("big.png");
    RgbaImage::from_pixel(1600, 1200, Rgba(GREEN))
        .save(&path)
        .unwrap();
    let content = load_file(&path, &LoadOptions::for_size(320, 180)).unwrap();
    let mut p = offscreen_player(&gpu, 320, 180);
    p.set_content(&gpu, content, Duration::ZERO).unwrap();
    // Cover on 320x180 needs 320x240 of a 4:3 image, not 1600x1200.
    assert_eq!(p.gpu_bytes(), 320 * 240 * 4);
}

#[test]
fn slideshow_changes_on_schedule_with_one_image_on_the_gpu() {
    let Some(gpu) = gpu() else { return };
    let dir = tempfile::tempdir().unwrap();
    let mut paths = Vec::new();
    for (i, c) in [RED, GREEN, BLUE].iter().enumerate() {
        let p = dir.path().join(format!("{i}.png"));
        RgbaImage::from_pixel(4, 4, Rgba(*c)).save(&p).unwrap();
        paths.push(p);
    }
    let first = load_still(&paths[0], Fit::Cover, (4, 4)).unwrap();
    let content = Content::Slideshow(SlideshowContent {
        images: paths,
        interval: ms(200),
        shuffle: false,
        first,
        look: Look::default(),
        target_size: (4, 4),
    });
    let mut p = offscreen_player(&gpu, 4, 4);
    p.set_content(&gpu, content, Duration::ZERO).unwrap();
    let mut colors = Vec::new();
    let mut frames = 0;
    let start = std::time::Instant::now();
    // Real time, because the next image is decoded on a worker thread.
    while start.elapsed() < ms(700) {
        let now = start.elapsed();
        if let Some(due) = p.next_due(now)
            && due <= now
        {
            p.render(&gpu, now).unwrap();
            frames += 1;
            colors.push(read(&gpu, &p).get_pixel(0, 0).0);
        }
        std::thread::sleep(ms(5));
    }
    colors.dedup();
    assert_eq!(&colors[..3], &[RED, GREEN, BLUE], "{colors:?}");
    assert!(frames <= 6, "only redraws on change, drew {frames}");
    assert_eq!(p.gpu_bytes(), 4 * 4 * 4, "one image resident at a time");
}

#[test]
fn render_loop_pauses_to_zero_and_stop_frees_the_gpu() {
    if gpu().is_none() {
        return;
    }
    let lp = RenderLoop::spawn(GpuOptions::for_tests());
    assert!(lp.stats().unwrap().adapter.is_none(), "no GPU until needed");
    lp.attach(
        0,
        TargetSpec::Offscreen {
            width: 32,
            height: 32,
        },
        30,
    )
    .unwrap();
    lp.load(0, shader(ShaderLang::Glsl, PLASMA)).unwrap();
    std::thread::sleep(ms(400));
    let running = lp.stats().unwrap().slots[0].frames.frames;
    assert!(running >= 5, "shader should be drawing, drew {running}");

    lp.set_fps(0, 0);
    std::thread::sleep(ms(100));
    let paused_at = lp.stats().unwrap().slots[0].frames.frames;
    std::thread::sleep(ms(400));
    let paused_later = lp.stats().unwrap().slots[0].frames.frames;
    assert_eq!(paused_at, paused_later, "paused means no frames");

    lp.set_fps(0, 30);
    std::thread::sleep(ms(300));
    assert!(lp.stats().unwrap().slots[0].frames.frames > paused_later);

    lp.clear(0);
    let s = lp.stats().unwrap();
    assert_eq!(s.slots[0].gpu_bytes, 0);
    assert_eq!(s.slots[0].kind, None);
    assert!(s.adapter.is_some());

    lp.detach(0);
    let s = lp.stats().unwrap();
    assert!(s.slots.is_empty());
    assert!(s.adapter.is_none(), "the device closes with the last slot");
}

#[test]
fn render_loop_draws_a_still_image_once() {
    if gpu().is_none() {
        return;
    }
    let lp = RenderLoop::spawn(GpuOptions::for_tests());
    lp.attach(
        3,
        TargetSpec::Offscreen {
            width: 4,
            height: 2,
        },
        60,
    )
    .unwrap();
    lp.load(
        3,
        Content::Still {
            image: checker(),
            look: Look {
                fit: Fit::Stretch,
                ..Look::default()
            },
        },
    )
    .unwrap();
    std::thread::sleep(ms(300));
    assert_eq!(lp.stats().unwrap().slots[0].frames.frames, 1);
    assert_eq!(lp.snapshot(3).unwrap(), checker());
    lp.resize(3, 8, 4);
    std::thread::sleep(ms(100));
    assert_eq!(
        lp.stats().unwrap().slots[0].frames.frames,
        2,
        "redraw on resize"
    );
}

#[test]
fn unsupported_types_explain_themselves() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("clip.mp4");
    std::fs::write(&path, b"not really").unwrap();
    let err = load_file(&path, &LoadOptions::for_size(8, 8)).unwrap_err();
    assert!(err.to_string().contains("video"), "{err}");
    let path = dir.path().join("notes.txt");
    std::fs::write(&path, b"x").unwrap();
    assert!(load_file(&path, &LoadOptions::for_size(8, 8)).is_err());
}

#[test]
fn decoder_limits_reject_absurd_images() {
    // A PNG header claiming 100000 x 100000 pixels.
    let mut png = Vec::new();
    png.extend_from_slice(b"\x89PNG\r\n\x1a\n");
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(b"IHDR");
    ihdr.extend_from_slice(&100_000u32.to_be_bytes());
    ihdr.extend_from_slice(&100_000u32.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    png.extend_from_slice(&13u32.to_be_bytes());
    png.extend_from_slice(&ihdr);
    png.extend_from_slice(&crc32(&ihdr).to_be_bytes());
    let err = decode_still(&png, Fit::Cover, (8, 8), "huge.png").unwrap_err();
    assert!(matches!(err, RenderError::Decode { .. }), "{err}");
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for &b in data {
        crc ^= u32::from(b);
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xedb8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

#[test]
fn bundled_examples_load_and_draw() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    let mut seen = 0;
    for entry in std::fs::read_dir(&root).unwrap() {
        let dir = entry.unwrap().path();
        let Ok(text) = std::fs::read_to_string(dir.join("wallpaper.json")) else {
            continue;
        };
        let manifest = ww_manifest::Manifest::from_json(&text)
            .unwrap_or_else(|e| panic!("{}: {e}", dir.display()));
        let content = load_package(&manifest, &dir, &LoadOptions::for_size(320, 180))
            .unwrap_or_else(|e| panic!("{}: {e}", dir.display()));
        seen += 1;
        if let Some(gpu) = gpu() {
            let mut p = offscreen_player(&gpu, 320, 180);
            p.set_content(&gpu, content, Duration::ZERO).unwrap();
            assert!(p.render(&gpu, Duration::ZERO).unwrap(), "{}", dir.display());
            let img = read(&gpu, &p);
            assert!(
                img.pixels().any(|px| px.0[..3] != [0, 0, 0]),
                "{} drew nothing",
                dir.display()
            );
        }
    }
    assert!(seen >= 4, "found {seen} examples");
}
