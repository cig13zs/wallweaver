//! Benchmark harness behind `wallweaver bench`.
//!
//! For each case it starts a fresh render loop, draws to an offscreen
//! target the size of a monitor, and measures:
//!
//! * frames drawn over the run, and the average frame rate
//! * CPU use of the whole process while running and while paused
//!   (CPU time divided by wall time; 100% is one core)
//! * resident memory: before, peak while running, and after the wallpaper
//!   is stopped
//! * GPU time per frame from GPU timestamps, when the adapter has them
//! * frames drawn while paused (should be zero)
//!
//! It can also start `ww-daemon --headless` and read the daemon's own
//! memory use over IPC, idle and with each case loaded.
//!
//! Numbers depend heavily on the machine and graphics driver. The report
//! records both, and on a software adapter (llvmpipe, WARP) GPU work is CPU
//! work, so CPU figures are not comparable with hardware runs.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde::Serialize;
use ww_desktop::process;
use ww_render::{Content, GpuOptions, LoadOptions, RenderLoop, TargetSpec};

/// What to measure.
#[derive(Debug, Clone)]
pub struct BenchConfig {
    /// Seconds of running measurement per case.
    pub seconds: f64,
    /// Seconds of paused measurement per case.
    pub paused_seconds: f64,
    /// Target size.
    pub width: u32,
    /// Target height.
    pub height: u32,
    /// Frame cap passed to the renderer (the governor's max fps).
    pub fps_cap: u32,
    /// Cases to run.
    pub cases: Vec<Case>,
    /// Adapter options.
    pub gpu: GpuOptions,
    /// Path to `ww-daemon`, to also measure the daemon's memory.
    pub daemon: Option<PathBuf>,
}

/// One wallpaper to measure.
#[derive(Debug, Clone)]
pub struct Case {
    /// Label in the report.
    pub name: String,
    /// A file or package folder.
    pub path: PathBuf,
}

/// Machine description recorded with every report.
#[derive(Debug, Clone, Serialize)]
pub struct Machine {
    /// Operating system.
    pub os: String,
    /// CPU architecture.
    pub arch: String,
    /// CPU model, if known.
    pub cpu: String,
    /// Logical cores.
    pub cores: usize,
    /// Installed memory in MiB, if known.
    pub memory_mib: Option<u64>,
}

/// Results for one case.
#[derive(Debug, Clone, Serialize)]
pub struct CaseResult {
    /// Case label.
    pub name: String,
    /// Renderer backend used.
    pub backend: String,
    /// How animation frames were stored, for animated images.
    pub animation_mode: Option<String>,
    /// Running seconds measured.
    pub seconds: f64,
    /// Frames drawn during the measured run (after a half-second warm-up).
    pub frames: u64,
    /// Frames drawn since the wallpaper was loaded, warm-up included. A
    /// still image shows 1 here and 0 in `frames`.
    pub frames_total: u64,
    /// Average frames per second while running.
    pub fps: f64,
    /// Process CPU while running, percent of one core.
    pub cpu_percent: f64,
    /// Process CPU while paused, percent of one core.
    pub paused_cpu_percent: f64,
    /// Frames drawn while paused.
    pub paused_frames: u64,
    /// Resident memory before the wallpaper was loaded, MiB.
    pub rss_before_mib: f64,
    /// Highest resident memory sampled while running, MiB.
    pub rss_peak_mib: f64,
    /// Resident memory after the wallpaper was stopped and the device closed, MiB.
    pub rss_after_stop_mib: f64,
    /// Average GPU time per frame in milliseconds, if the adapter has timestamps.
    pub gpu_ms_per_frame: Option<f64>,
    /// Texture memory the scene reported holding, MiB.
    pub gpu_textures_mib: f64,
    /// Seconds spent decoding before the run (not part of the CPU figures).
    pub load_seconds: f64,
}

/// Memory of a running `ww-daemon`.
#[derive(Debug, Clone, Serialize)]
pub struct DaemonResult {
    /// Daemon RSS with no wallpaper, MiB.
    pub idle_rss_mib: Option<f64>,
    /// Daemon RSS a few seconds after each case was set, MiB.
    pub cases: Vec<(String, Option<f64>)>,
}

/// A full report.
#[derive(Debug, Clone, Serialize)]
pub struct Report {
    /// Wallweaver version.
    pub version: String,
    /// Machine.
    pub machine: Machine,
    /// Graphics adapter.
    pub adapter: String,
    /// True for software adapters.
    pub software_adapter: bool,
    /// Target size.
    pub resolution: String,
    /// Frame cap.
    pub fps_cap: u32,
    /// Per-case results.
    pub results: Vec<CaseResult>,
    /// Daemon memory, if measured.
    pub daemon: Option<DaemonResult>,
}

const MIB: f64 = 1024.0 * 1024.0;

fn rss_mib() -> f64 {
    process::rss_bytes().map_or(0.0, |b| b as f64 / MIB)
}

fn cpu() -> Duration {
    process::cpu_time().unwrap_or_default()
}

/// Describe this machine.
pub fn machine() -> Machine {
    let cpu = std::fs::read_to_string("/proc/cpuinfo")
        .ok()
        .and_then(|t| {
            t.lines()
                .find(|l| l.starts_with("model name"))
                .and_then(|l| l.split_once(':'))
                .map(|(_, v)| v.trim().to_string())
        })
        .or_else(|| std::env::var("PROCESSOR_IDENTIFIER").ok())
        .unwrap_or_else(|| "unknown".into());
    let memory_mib = std::fs::read_to_string("/proc/meminfo").ok().and_then(|t| {
        t.lines()
            .find(|l| l.starts_with("MemTotal:"))
            .and_then(|l| l.split_whitespace().nth(1))
            .and_then(|kb| kb.parse::<u64>().ok())
            .map(|kb| kb / 1024)
    });
    Machine {
        os: std::env::consts::OS.into(),
        arch: std::env::consts::ARCH.into(),
        cpu,
        cores: std::thread::available_parallelism().map_or(1, |n| n.get()),
        memory_mib,
    }
}

fn load(path: &Path, opts: &LoadOptions) -> Result<Content, String> {
    if path.is_dir() {
        let text =
            std::fs::read_to_string(path.join("wallpaper.json")).map_err(|e| e.to_string())?;
        let manifest = ww_manifest::Manifest::from_json(&text).map_err(|e| e.to_string())?;
        ww_render::load_package(&manifest, path, opts).map_err(|e| e.to_string())
    } else {
        ww_render::load_file(path, opts).map_err(|e| e.to_string())
    }
}

/// Run every case and build the report.
pub fn run(config: &BenchConfig, mut progress: impl FnMut(&str)) -> Result<Report, String> {
    let mut results = Vec::new();
    let mut adapter = None;
    let opts = LoadOptions::for_size(config.width, config.height);
    for case in &config.cases {
        progress(&format!("{}: loading", case.name));
        let lp = RenderLoop::spawn(config.gpu.clone());
        // Let the thread settle, then take the baseline.
        std::thread::sleep(Duration::from_millis(200));
        let rss_before = rss_mib();
        let t_load = Instant::now();
        let content = load(&case.path, &opts).map_err(|e| format!("{}: {e}", case.name))?;
        let load_seconds = t_load.elapsed().as_secs_f64();
        let backend = content.kind().to_string();
        lp.attach(
            0,
            TargetSpec::Offscreen {
                width: config.width,
                height: config.height,
            },
            config.fps_cap,
        )
        .map_err(|e| e.to_string())?;
        lp.load(0, content).map_err(|e| e.to_string())?;
        let stats = lp.stats().ok_or("render loop stopped")?;
        if adapter.is_none() {
            adapter = stats.adapter.clone();
        }

        progress(&format!("{}: running {:.0} s", case.name, config.seconds));
        // Warm up for half a second (first frame, pipeline compile).
        std::thread::sleep(Duration::from_millis(500));
        let f0 = lp.stats().map(|s| s.slots[0].frames).unwrap_or_default();
        let (c0, t0) = (cpu(), Instant::now());
        let mut rss_peak = rss_mib();
        let end = t0 + Duration::from_secs_f64(config.seconds);
        while Instant::now() < end {
            std::thread::sleep(Duration::from_millis(100));
            rss_peak = rss_peak.max(rss_mib());
        }
        let wall = t0.elapsed().as_secs_f64();
        let cpu_running = (cpu() - c0).as_secs_f64() / wall * 100.0;
        let s1 = lp.stats().ok_or("render loop stopped")?;
        let slot = s1.slots[0].clone();
        let frames = slot.frames.frames - f0.frames;
        let timed = slot.frames.timed_frames - f0.timed_frames;
        let gpu_ms =
            (timed > 0).then(|| (slot.frames.gpu_ns - f0.gpu_ns) as f64 / timed as f64 / 1e6);

        progress(&format!(
            "{}: paused {:.0} s",
            case.name, config.paused_seconds
        ));
        lp.set_fps(0, 0);
        std::thread::sleep(Duration::from_millis(200));
        let pf0 = lp.stats().map(|s| s.slots[0].frames.frames).unwrap_or(0);
        let (pc0, pt0) = (cpu(), Instant::now());
        std::thread::sleep(Duration::from_secs_f64(config.paused_seconds));
        let pwall = pt0.elapsed().as_secs_f64();
        let paused_cpu = (cpu() - pc0).as_secs_f64() / pwall * 100.0;
        let paused_frames = lp.stats().map(|s| s.slots[0].frames.frames).unwrap_or(0) - pf0;

        lp.detach(0);
        let _ = lp.stats();
        drop(lp);
        std::thread::sleep(Duration::from_millis(300));
        let rss_after = rss_mib();

        results.push(CaseResult {
            name: case.name.clone(),
            backend,
            animation_mode: slot.animation_mode.map(str::to_string),
            seconds: wall,
            frames,
            frames_total: slot.frames.frames,
            fps: frames as f64 / wall,
            cpu_percent: cpu_running,
            paused_cpu_percent: paused_cpu,
            paused_frames,
            rss_before_mib: rss_before,
            rss_peak_mib: rss_peak,
            rss_after_stop_mib: rss_after,
            gpu_ms_per_frame: gpu_ms,
            gpu_textures_mib: slot.gpu_bytes as f64 / MIB,
            load_seconds,
        });
    }
    let daemon = match &config.daemon {
        Some(path) => Some(measure_daemon(path, config, &mut progress)?),
        None => None,
    };
    let adapter = adapter.ok_or("no case ran")?;
    Ok(Report {
        version: env!("CARGO_PKG_VERSION").into(),
        machine: machine(),
        adapter: adapter.to_string(),
        software_adapter: adapter.software,
        resolution: format!("{}x{}", config.width, config.height),
        fps_cap: config.fps_cap,
        results,
        daemon,
    })
}

/// Start `ww-daemon --headless` on a private socket, read its memory idle
/// and with each case set, then shut it down.
fn measure_daemon(
    path: &Path,
    config: &BenchConfig,
    progress: &mut impl FnMut(&str),
) -> Result<DaemonResult, String> {
    let dir = std::env::temp_dir().join(format!("ww-bench-{}", std::process::id()));
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let socket = dir.join("run/daemon.sock");
    let mut cmd = std::process::Command::new(path);
    cmd.arg(format!("--headless={}x{}", config.width, config.height))
        .arg("--data-dir")
        .arg(dir.join("data"))
        .arg("--socket")
        .arg(&socket)
        .stderr(std::process::Stdio::null());
    if config.gpu.allow_software {
        cmd.arg("--software");
    }
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("starting {}: {e}", path.display()))?;
    let endpoint = ww_ipc::Endpoint::Path(socket);
    let mut client = None;
    for _ in 0..100 {
        if let Ok(c) = ww_ipc::Client::connect(&endpoint, "wallweaver-bench") {
            client = Some(c);
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let Some(mut client) = client else {
        let _ = child.kill();
        return Err("the daemon did not start".into());
    };
    let rss = |c: &mut ww_ipc::Client| -> Option<f64> {
        match c.request(ww_ipc::Request::Status) {
            Ok(ww_ipc::Response::Status(s)) => s.rss_bytes.map(|b| b as f64 / MIB),
            _ => None,
        }
    };
    progress("daemon: idle");
    std::thread::sleep(Duration::from_secs(2));
    let idle = rss(&mut client);
    let mut cases = Vec::new();
    for case in &config.cases {
        progress(&format!("daemon: {}", case.name));
        let source = if case.path.is_dir() {
            ww_ipc::WallpaperSource::Folder {
                path: case.path.to_string_lossy().into_owned(),
            }
        } else {
            ww_ipc::WallpaperSource::File {
                path: case.path.to_string_lossy().into_owned(),
            }
        };
        let ok = client.request(ww_ipc::Request::SetWallpaper {
            monitor: ww_ipc::MonitorTarget::All,
            source,
        });
        std::thread::sleep(Duration::from_secs(3));
        cases.push((case.name.clone(), ok.ok().and_then(|_| rss(&mut client))));
        let _ = client.request(ww_ipc::Request::ClearWallpaper {
            monitor: ww_ipc::MonitorTarget::All,
        });
        std::thread::sleep(Duration::from_millis(500));
    }
    let _ = client.request(ww_ipc::Request::Shutdown);
    let _ = child.wait();
    let _ = std::fs::remove_dir_all(&dir);
    Ok(DaemonResult {
        idle_rss_mib: idle,
        cases,
    })
}

/// Generate the built-in test wallpapers into `dir`: a 4K image, a
/// three-image slideshow package, a 30-frame GIF and a plasma shader.
pub fn builtin_cases(dir: &Path) -> Result<Vec<Case>, String> {
    use image::{Delay, Frame, Rgba, RgbaImage};
    let io = |e: std::io::Error| e.to_string();
    std::fs::create_dir_all(dir).map_err(io)?;

    let gradient = |w: u32, h: u32, shift: u32| {
        RgbaImage::from_fn(w, h, |x, y| {
            Rgba([
                ((x * 255 / w + shift) % 256) as u8,
                (y * 255 / h) as u8,
                ((x + y + shift) % 256) as u8,
                255,
            ])
        })
    };
    let image_path = dir.join("still-3840x2160.png");
    gradient(3840, 2160, 0)
        .save(&image_path)
        .map_err(|e| e.to_string())?;

    let show = dir.join("slideshow");
    std::fs::create_dir_all(&show).map_err(io)?;
    for i in 0..3 {
        gradient(2560, 1440, i * 80)
            .save(show.join(format!("{i}.png")))
            .map_err(|e| e.to_string())?;
    }
    std::fs::write(
        show.join("wallpaper.json"),
        r#"{
  "manifestVersion": 1,
  "id": "bench.slideshow",
  "name": "Bench slideshow",
  "version": "1.0.0",
  "type": "slideshow",
  "entry": "0.png",
  "author": { "name": "bench" },
  "license": "CC0-1.0",
  "slideshow": { "images": ["0.png", "1.png", "2.png"], "interval": 5 }
}
"#,
    )
    .map_err(io)?;

    let gif_path = dir.join("anim-480x270-30f.gif");
    {
        let file = std::fs::File::create(&gif_path).map_err(io)?;
        let mut enc = image::codecs::gif::GifEncoder::new_with_speed(file, 30);
        enc.set_repeat(image::codecs::gif::Repeat::Infinite)
            .map_err(|e| e.to_string())?;
        let frames = (0..30).map(|i| {
            Frame::from_parts(
                gradient(480, 270, i * 8),
                0,
                0,
                Delay::from_numer_denom_ms(40, 1),
            )
        });
        enc.encode_frames(frames).map_err(|e| e.to_string())?;
    }

    let shader_path = dir.join("plasma.frag");
    std::fs::write(&shader_path, PLASMA).map_err(io)?;

    Ok(vec![
        Case {
            name: "image 4K -> screen".into(),
            path: image_path,
        },
        Case {
            name: "slideshow (5 s)".into(),
            path: show,
        },
        Case {
            name: "GIF 480x270, 25 fps".into(),
            path: gif_path,
        },
        Case {
            name: "shader (plasma)".into(),
            path: shader_path,
        },
    ])
}

/// The plasma shader used by the built-in shader case.
pub const PLASMA: &str = "void mainImage(out vec4 fragColor, in vec2 fragCoord) {
    vec2 uv = fragCoord / iResolution.xy;
    float v = sin(uv.x * 10.0 + iTime) + sin((uv.y * 10.0 + iTime) / 2.0)
            + sin((uv.x * 10.0 + uv.y * 10.0 + iTime) / 2.0);
    vec2 c = uv * 10.0 + vec2(sin(iTime / 3.0), cos(iTime / 2.0)) * 5.0;
    v += sin(sqrt(c.x * c.x + c.y * c.y + 1.0) + iTime);
    fragColor = vec4(0.5 + 0.5 * sin(v), 0.5 + 0.5 * sin(v + 2.094), 0.5 + 0.5 * sin(v + 4.188), 1.0);
}
";

fn opt(v: Option<f64>, digits: usize) -> String {
    v.map_or("n/a".into(), |x| format!("{x:.digits$}"))
}

/// Markdown summary of a report.
pub fn to_markdown(r: &Report) -> String {
    let m = &r.machine;
    let mut out = String::new();
    out.push_str(&format!(
        "Wallweaver {} on {} {}, {} ({} cores{}), adapter: {}{}. Target {} at a {} fps cap.\n\n",
        r.version,
        m.os,
        m.arch,
        m.cpu,
        m.cores,
        m.memory_mib.map_or(String::new(), |mb| format!(", {mb} MiB RAM")),
        r.adapter,
        if r.software_adapter {
            " (software: GPU work runs on the CPU, so CPU numbers are much higher than on real hardware)"
        } else {
            ""
        },
        r.resolution,
        r.fps_cap
    ));
    out.push_str("| Case | Backend | Frames (total) | Avg fps | CPU % running | CPU % paused | Frames while paused | RSS before (MiB) | RSS peak (MiB) | RSS after stop (MiB) | GPU ms/frame | Textures (MiB) |\n");
    out.push_str("|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|\n");
    for c in &r.results {
        let backend = match &c.animation_mode {
            Some(m) => format!("{} ({m})", c.backend),
            None => c.backend.clone(),
        };
        out.push_str(&format!(
            "| {} | {} | {} ({}) | {:.1} | {:.1} | {:.2} | {} | {:.1} | {:.1} | {:.1} | {} | {:.1} |\n",
            c.name,
            backend,
            c.frames,
            c.frames_total,
            c.fps,
            c.cpu_percent,
            c.paused_cpu_percent,
            c.paused_frames,
            c.rss_before_mib,
            c.rss_peak_mib,
            c.rss_after_stop_mib,
            opt(c.gpu_ms_per_frame, 3),
            c.gpu_textures_mib
        ));
    }
    if let Some(d) = &r.daemon {
        out.push_str("\n| ww-daemon (headless) | RSS (MiB) |\n|---|---:|\n");
        out.push_str(&format!(
            "| idle, no wallpaper | {} |\n",
            opt(d.idle_rss_mib, 1)
        ));
        for (name, rss) in &d.cases {
            out.push_str(&format!("| {name} | {} |\n", opt(*rss, 1)));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_cases_load() {
        let dir = tempfile::tempdir().unwrap();
        let cases = builtin_cases(dir.path()).unwrap();
        assert_eq!(cases.len(), 4);
        let opts = LoadOptions::for_size(640, 360);
        let kinds: Vec<_> = cases
            .iter()
            .map(|c| load(&c.path, &opts).unwrap().kind())
            .collect();
        assert_eq!(kinds, ["image", "slideshow", "animated", "shader"]);
    }

    #[test]
    fn markdown_has_one_row_per_case() {
        let r = Report {
            version: "0".into(),
            machine: machine(),
            adapter: "test".into(),
            software_adapter: true,
            resolution: "8x8".into(),
            fps_cap: 60,
            results: vec![CaseResult {
                name: "x".into(),
                backend: "shader".into(),
                animation_mode: None,
                seconds: 1.0,
                frames: 30,
                frames_total: 31,
                fps: 30.0,
                cpu_percent: 5.0,
                paused_cpu_percent: 0.0,
                paused_frames: 0,
                rss_before_mib: 1.0,
                rss_peak_mib: 2.0,
                rss_after_stop_mib: 1.5,
                gpu_ms_per_frame: None,
                gpu_textures_mib: 0.0,
                load_seconds: 0.1,
            }],
            daemon: None,
        };
        let md = to_markdown(&r);
        assert!(md.contains("| x | shader | 30 (31) |"), "{md}");
        assert!(md.contains("software"));
        assert!(serde_json::to_string(&r).unwrap().contains("\"fps\":30.0"));
    }
}
