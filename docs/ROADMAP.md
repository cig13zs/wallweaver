# Roadmap

Dates are deliberately left out; versions ship when they meet their bar.
Windows, macOS and Linux reach v1.0 together.

## v0.1: the native core (in progress)

- [x] `wallpaper.json` format, strict validation, JSON Schema
- [x] `.wwp` packages: deterministic packing, ed25519 signing, defensive
      unpacking, fuzz targets
- [x] Permission model: default deny, consent text, CSP and network filter
- [x] Power governor: run, throttle, pause, stop, with property tests
- [x] Native renderer: images, slideshows, GIF/WebP/APNG, GLSL and WGSL shaders
- [x] Daemon and versioned IPC; CLI (`validate`, `pack`, `sign`, `verify`,
      `keygen`, `bench`, daemon control)
- [x] Linux X11 desktop host
- [x] Windows desktop host (classic and 24H2 layouts), tray icon: written,
      needs testing on Windows 10 22H2 and Windows 11 23H2, 24H2 and 25H2
- [ ] Manager window wired to the daemon over IPC
- [ ] Drop-a-file import in the manager window (the daemon already accepts
      single files)
- [ ] First benchmark run on real Windows and Linux hardware, published in
      `docs/benchmarks/`

## v0.2: video and Linux Wayland

- [ ] Hardware video decode into GPU textures (Media Foundation, VA-API),
      libmpv fallback ([design](VIDEO.md))
- [ ] Wayland host via `wlr-layer-shell` (KDE Plasma, Sway, Hyprland and
      other wlroots compositors); GNOME needs a Shell extension
- [ ] Audio levels and system stats data feeds
- [ ] Multi-monitor span mode, playlists and schedules

## v0.3: web wallpapers and the gallery

- [ ] Sandboxed web view host: CSP from approved permissions, request
      filtering, no navigation, per-package storage, suspended when paused
- [ ] Web wallpapers: HTML/CSS/JS, WebGL, WebGPU, p5.js, three.js, Godot web
      exports, any website with explicit network permission
- [ ] Gallery client: browse, install with the consent screen, report,
      revocation list checked daily
- [ ] Add-your-own gallery sources: anyone can host a signed index
- [ ] Gallery index entries may point at creator-hosted files

## v0.4: macOS and creation tools

- [ ] macOS host (desktop-level window per screen, VideoToolbox decode)
- [ ] Remix with credit: fork any wallpaper whose license allows it; the
      original creator stays credited in the manifest and the gallery
- [ ] `wallweaver init` templates and a live-reload preview
- [ ] Lottie and Rive playback

## v0.5 to v0.9: hardening

- [ ] Public "try to break it" pack of hostile packages, each blocked by an
      automated test
- [ ] Official collabs shelf with verified badges for licensed brand and
      studio wallpapers
- [ ] Live2D and Spine
- [ ] Signed installers and auto-update on all three desktop platforms
- [ ] Accessibility pass: reduced motion, flashing-content safety

## v1.0

- [ ] Windows, macOS and Linux (X11 and Wayland) at feature parity for
      native, video and web wallpapers
- [ ] Budgets met and published from real hardware (see
      [ARCHITECTURE.md](ARCHITECTURE.md#budgets))
- [ ] Gallery with moderation, takedowns and revocation in production

## v1.x

- [ ] Android live wallpaper (also covers ChromeOS)

## Not planned

- iOS live wallpapers: iOS has no API for third-party live wallpapers. iOS
  gets a companion app only (browse, favorite, send to your computer).
- Executable or native-plugin wallpapers from the gallery.
