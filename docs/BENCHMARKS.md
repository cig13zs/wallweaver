# Benchmarks

Speed is a feature here, so it gets measured, and the numbers we publish are
only ones we actually measured, labeled with the machine they came from.

## Running the harness

```sh
cargo build --release -p wallweaver -p ww-daemon
target/release/wallweaver bench                  # built-in test wallpapers
target/release/wallweaver bench examples/rain-gif my-shader.frag
```

Options: `--seconds N` (running time per case, default 10),
`--paused-seconds N` (default 3), `--size WxH` (default 1920x1080),
`--fps N` (frame cap, default 60), `--out DIR` (default `bench-results`),
`--daemon PATH` / `--no-daemon`, `--software` (allow a software GPU).
It writes `bench.json` and `bench.md`.

The built-in cases are generated on the fly so no assets are needed: a
3840x2160 PNG shown on a 1920x1080 target, a three-image 2560x1440 slideshow
changing every 5 seconds, a 30-frame 480x270 GIF at 25 fps, and a plasma
shader (Shadertoy-style GLSL).

## What is measured, and how

For each case the harness starts a fresh render loop and draws to an
offscreen target of the chosen size, using the same render thread, pacing
and backends as the daemon.

| Column | Meaning |
|---|---|
| Frames (total) | frames drawn during the timed run (after a 0.5 s warm-up), and since load including warm-up. A still image draws exactly one frame. |
| Avg fps | timed frames / timed seconds |
| CPU % running | process CPU time (user + system) / wall time over the run; 100% = one core |
| CPU % paused | the same, with the wallpaper paused by the governor path (frame cap 0) |
| Frames while paused | should be 0 |
| RSS before / peak / after stop | resident memory before loading, highest 10 Hz sample while running, and after the wallpaper is stopped and the GPU device closed |
| GPU ms/frame | GPU timestamps around the render pass, when the adapter supports them |
| Textures | texture memory the scene holds (estimate; drivers may pad) |

With `--daemon` (default when `ww-daemon` sits next to `wallweaver`), it
also starts `ww-daemon --headless` on a private socket and reads the daemon's
own resident memory over IPC: idle, then 3 seconds after setting each case.

Caveats that matter:

- **Software GPUs.** On llvmpipe, lavapipe or WARP, GPU work runs on the CPU,
  so CPU figures are far higher than on any real GPU, and the driver itself
  (an LLVM JIT) adds over 100 MiB of resident memory once a device is open.
  The report flags software adapters.
- **Headless vs. desktop.** Offscreen targets skip the compositor and vsync.
  On a real desktop the swap chain is FIFO (vsync) with one frame of latency.
- **RSS is not the whole story.** Memory held by the GPU driver in video
  memory doesn't show up in RSS on discrete GPUs; the Textures column covers
  what Wallweaver allocates itself.

## Results

### 2026-10-25: Linux x86_64 cloud VM, software Vulkan (llvmpipe)

Machine: 4-core Intel Xeon @ 2.10 GHz VM, 16 GiB RAM, Ubuntu 24.04, no GPU.
Adapter: llvmpipe (LLVM 20.1.2, 256 bits) through Mesa 25.2.8 lavapipe
(Vulkan). Target 1920x1080, 60 fps cap. `wallweaver bench --software`,
10 s running and 3 s paused per case. Raw data:
[benchmarks/2026-10-25-linux-x86_64-llvmpipe.json](benchmarks/2026-10-25-linux-x86_64-llvmpipe.json).

| Case | Backend | Frames (total) | Avg fps | CPU % running | CPU % paused | Frames while paused | RSS before (MiB) | RSS peak (MiB) | RSS after stop (MiB) | GPU ms/frame | Textures (MiB) |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| image 4K -> screen | image | 0 (1) | 0.0 | 0.1 | 0.00 | 0 | 23.1 | 192.1 | 83.7 | n/a | 7.9 |
| slideshow (5 s) | slideshow | 2 (3) | 0.2 | 6.6 | 0.00 | 0 | 83.7 | 238.6 | 110.4 | 12.764 | 7.9 |
| GIF 480x270, 25 fps | animated (gpu-array) | 250 (265) | 24.9 | 64.1 | 0.00 | 0 | 110.4 | 200.4 | 110.8 | 7.796 | 14.8 |
| shader (plasma) | shader | 299 (315) | 29.7 | 99.2 | 0.00 | 0 | 110.8 | 155.2 | 111.0 | 12.577 | 0.0 |

| ww-daemon (headless) | RSS (MiB) |
|---|---:|
| idle, no wallpaper | 5.0 |
| image 4K -> screen | 143.0 |
| slideshow (5 s) | 195.5 |
| GIF 480x270, 25 fps | 226.5 |
| shader (plasma) | 196.8 |

Also measured on the same VM: `ww-daemon` on a real X11 desktop (Xvfb with
openbox, X11 host, one 1280x720 monitor) used **5.5 MiB** resident with no
wallpaper set, and paused the shader wallpaper when a window went fullscreen.

What this run shows, and what it doesn't:

- It shows the parts that don't depend on the GPU: the idle daemon is about
  5 MiB; every wallpaper type does no work and uses 0.00% CPU while paused; a
  still image draws once; a slideshow draws only when the picture changes;
  the GIF stays on the GPU as a texture array and plays at its own 25 fps;
  the shader holds its 30 fps default cap.
- It does not show real-world CPU, GPU or memory for running wallpapers: on a
  software adapter the "GPU" is the CPU, and the driver dominates memory.
  Numbers from real Windows and Linux hardware will be added here when they
  have been measured.

## Comparing with other wallpaper apps fairly

To compare Wallweaver with Wallpaper Engine, Lively or anything else, keep
everything but the app the same:

1. **Same machine, same state.** Same OS build, GPU driver, power plan and
   monitor setup. Plugged in. Reboot between apps, close everything else, and
   let the system idle for a few minutes first.
2. **Same content.** Use the same source file in each app: the same image,
   the same GIF, the same video file (same codec, resolution and frame rate),
   or the same shader ported faithfully. Note where an app can't show a type
   natively (for example, a shader that runs inside a web page in one app).
3. **Same settings.** Same frame cap, same quality or scaling options, audio
   off, same pause rules. Close every manager or settings window during the
   measurement; they are separate processes in most apps.
4. **Count every process.** Wallpaper apps run helper processes (renderers,
   web views, video players). Sum CPU and memory over the whole process tree
   of each app, including its web view or player children.
5. **Measure the same way for everyone.** On Windows use Performance Monitor
   or `typeperf` with `\Process(*)\% Processor Time`,
   `\Process(*)\Working Set - Private` and `\GPU Engine(*)\Utilization Percentage`
   (per process id), sampled once a second. PresentMon gives frame times. On
   Linux use `pidstat -r -u -h 1` and the GPU vendor's tools. Use the same
   tool for all apps rather than each app's own meter.
6. **Run long enough.** Warm up for 60 seconds, measure for 5 minutes, repeat
   three times and report the median. Measure running, covered by a
   maximized window, with a fullscreen game, and paused.
7. **Publish everything.** Hardware, driver versions, app versions and
   settings, the content files (or how to get them), and the raw samples.
   Report numbers that make Wallweaver look bad too.
