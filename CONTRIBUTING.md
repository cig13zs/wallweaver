# Contributing to Wallweaver

Thanks for helping. Bug reports, wallpapers, docs fixes and code are all
welcome.

## Before you start

- For anything bigger than a small fix, open an issue first so we can agree
  on the approach.
- Security problems go through private reporting, not issues. See
  [SECURITY.md](SECURITY.md).
- Be kind. The [Code of Conduct](CODE_OF_CONDUCT.md) applies everywhere in
  the project. To report a conduct problem privately, open a draft security
  advisory (Security tab, "Report a vulnerability") with a title starting
  "Conduct report"; only maintainers can see it.

## Building and testing

You need stable Rust (see `rust-version` in `Cargo.toml`), and Node 22 with
pnpm 10 for the manager UI.

```sh
pnpm -C ui install && pnpm -C ui build   # the manager app embeds ui/dist
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

On Linux the manager app needs the WebKitGTK development packages
(`libwebkit2gtk-4.1-dev libgtk-3-dev librsvg2-dev libsoup-3.0-dev`), and the
renderer tests want a Vulkan driver. Without a GPU, `mesa-vulkan-drivers`
gives you a software one (lavapipe). Tests that need a GPU skip themselves
when there is none; set `WW_REQUIRE_GPU=1` to make that a failure instead.

The renderer has a golden image test. If you change shader handling on
purpose, regenerate it with
`WW_BLESS=1 cargo test -p ww-render --test render golden` and look at the new
image before committing it. After changing manifest types, regenerate the
schema with `WW_UPDATE_SCHEMA=1 cargo test -p ww-manifest`.

Fuzzing the package reader needs nightly and cargo-fuzz:

```sh
cd crates/ww-package
cargo +nightly fuzz run unpack
```

## Code guidelines

- Keep dependencies few and small. The daemon runs all day; every crate
  costs memory and attack surface. Explain new dependencies in the PR.
- No `unsafe` without a `// SAFETY:` comment that says why it is sound.
  Windows API calls go through the `windows` crate.
- No `unwrap()` outside tests. Errors shown to users should be plain
  sentences that say what to do.
- Platform code lives behind `cfg` in `ww-desktop`. Code for other platforms
  must keep compiling; CI builds on Windows, macOS and Linux.
- Pure logic (the governor, permissions, pacing) gets unit tests and, where
  it helps, property tests.
- Don't copy code from GPL projects. Reading them to understand an approach is
  fine; write your own implementation.

## Commits and pull requests

- Small, focused commits with short plain messages ("Add slideshow shuffle",
  not "misc fixes").
- One topic per pull request. Say what changed and how you tested it.
- By contributing you agree your work is dual licensed under MIT and
  Apache-2.0, like the rest of the project.

## Contributing wallpapers

Sample wallpapers in `examples/` must be your own work or CC0, with a valid
`wallpaper.json` (`wallweaver validate <folder>`). Don't include anything
based on someone else's characters, logos or brands in this repository. See
[docs/CONTENT-POLICY.md](docs/CONTENT-POLICY.md) for gallery rules.
