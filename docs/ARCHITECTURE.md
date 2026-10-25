# Architecture

Wallweaver is native first. Everything that can be drawn without a browser
engine is drawn by a small Rust renderer; a web view exists only for web
wallpapers. The app is split into two processes so that the part that runs
all day stays small.

```
 ┌───────────────────────────────┐        ┌──────────────────────────────────┐
 │ Manager window (Tauri+Svelte) │  IPC   │ ww-daemon (always running)       │
 │ app/, ui/                     │◄──────►│                                  │
 │ runs only while it is open    │ local  │  main thread                     │
 └───────────────────────────────┘ socket │   DesktopHost  (windows, signals)│
                                          │   Governor     (run/pause/stop)  │
 ┌───────────────────────────────┐        │   permissions, settings, tray    │
 │ wallweaver CLI                │◄──────►│  render thread                   │
 │ validate/pack/sign/bench/...  │        │   wgpu: image, slideshow,        │
 └───────────────────────────────┘        │   animated, shader backends      │
                                          │  loader threads (decode only)    │
                                          └──────────────────────────────────┘
```

## Crates

| Crate | Role | Talks to the OS? |
|---|---|---|
| `ww-manifest` | `wallpaper.json` types, strict parsing, validation, JSON Schema | no |
| `ww-package` | `.wwp` format: pack, sign (ed25519), verify, unpack safely | file system only |
| `ww-permissions` | default-deny grants, consent text, CSP, network filter | no |
| `ww-governor` | decides Running / Throttled / Paused / Stopped per monitor | no |
| `ww-render` | wgpu renderer, decoders, frame pacing, render thread | GPU |
| `ww-desktop` | `DesktopHost` per OS, battery, process stats | yes |
| `ww-ipc` | versioned UI ↔ daemon protocol over a local socket | sockets |
| `ww-daemon` | wires the above together | yes |
| `ww-bench` | benchmark harness | GPU |
| `wallweaver` | command-line tool | yes |

The pure-logic crates (manifest, permissions, governor, pacing in the
renderer) have no OS dependencies and carry most of the tests, including
property tests for the governor and the permission model.

## The daemon's threads

- **Main thread.** Owns the desktop host and the governor. It sleeps in the
  host's native wait (`MsgWaitForMultipleObjects` on Windows, `poll` on the
  X connection on Linux). It wakes for window-system events, IPC requests
  (the IPC threads call the host's waker), finished loads, the governor's next
  deadline, and, only while a wallpaper is set, a once-a-second read of
  fullscreen, battery and lock state. With no wallpaper set it doesn't wake.
- **Render thread.** Draws every monitor. Each slot reports when its next
  frame is due; the thread sleeps until the earliest one or until a command
  arrives. Paused slots have no due time. When nothing is due, it blocks on
  its command channel.
- **Loader threads.** Decode and shrink images, decode animations, compile
  shaders. The render thread never decodes.
- **IPC threads.** One accepts connections; each connection has a reader.

## Rendering

- **Pacing** (`ww-render/src/pacing.rs`): the next frame is
  `max(when the content changes, last frame + 1/fps cap)`, or never. Still
  images change never, slideshows at the interval, animations at their frame
  delays, shaders every frame. A frame cap of 0 means paused.
- **Images and slideshows:** decoded with size and allocation limits, shrunk
  to the screen size on the CPU (never enlarged), uploaded once. A slideshow
  keeps one image on the GPU and decodes the next one ahead of time.
- **Animated images:** if all frames fit the GPU budget (96 MiB by default)
  and the layer limit, they are uploaded once as a texture array and playback
  is a uniform change. Over that, frames stay in RAM (64 MiB budget) and are
  uploaded when shown. Over both, a worker thread decodes frames as needed,
  two ahead.
- **Shaders:** WGSL or GLSL is wrapped, parsed and validated with naga on the
  loader thread, so a broken shader gives a line-numbered error instead of a
  crash. The shader clock only advances while frames are drawn.
- **Video:** interface only for now; see [VIDEO.md](VIDEO.md).
- **Presentation:** FIFO (vsync) swap chains with a frame latency of one, so
  the GPU never queues work ahead. On Windows, wgpu's DX12 backend waits on the
  swap chain's waitable object.
- **Stop:** a stopped monitor's slot is detached, which drops its surface,
  textures and pipelines. When the last slot goes, the GPU device is closed
  and (on Linux with glibc) freed heap is returned to the OS.

## Desktop hosts

- **Windows:** the wallpaper window is a child of the wallpaper layer behind
  the icons. On the classic layout that is the `WorkerW` Explorer creates when
  `Progman` receives message `0x052C`. On Windows 11 24H2 and later, detected
  by `WS_EX_NOREDIRECTIONBITMAP` on `Progman` rather than by build number, the
  window is made layered and parented to `Progman` below `SHELLDLL_DefView`.
  A hidden top-level window receives `WM_DISPLAYCHANGE`, power broadcasts and
  `TaskbarCreated` (Explorer restarts), and hosts the tray icon.
- **Linux X11:** one `_NET_WM_WINDOW_TYPE_DESKTOP` window per RandR monitor,
  with an empty input shape so clicks reach the desktop icons.
- **Wayland and macOS:** documented stubs that return a clear error. The
  plans are in the module docs (`wlr-layer-shell` background surfaces; a
  borderless window at the desktop window level).

## Budgets

These are the targets the design is held to. Measured numbers, with the
machine they came from, are in [BENCHMARKS.md](BENCHMARKS.md).

| Situation | Target |
|---|---|
| Daemon idle, no wallpaper | under 15 MB RAM |
| Static or shader wallpaper | under 40 MB RAM on a hardware GPU |
| 4K60 video with hardware decode | under 3% CPU |
| Paused or covered | 0% CPU and GPU |
| Stopped | GPU memory released |

## IPC

Length-prefixed JSON frames (4-byte little-endian length, at most 1 MiB) over
a Unix socket in a private folder or a named pipe restricted to its owner.
A connection starts with a version handshake; unknown event and response
kinds decode as `Unknown`, and unknown requests get an `unsupported` error,
so the manager and the daemon can be updated separately. See
`crates/ww-ipc/src/lib.rs`.
