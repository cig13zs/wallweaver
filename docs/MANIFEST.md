# wallpaper.json

Every Wallweaver package has a `wallpaper.json` at its root. Parsing is
strict: unknown fields are errors, so a typo never silently does nothing.
Run `wallweaver validate <folder>` to see every problem at once. A JSON Schema
for editor completion is at [`schema/wallpaper.schema.json`](../schema/wallpaper.schema.json)
(`wallweaver schema` prints it); the Rust validator is stricter than the
schema.

## Example

```json
{
  "$schema": "https://raw.githubusercontent.com/cig13zs/wallweaver/main/schema/wallpaper.schema.json",
  "manifestVersion": 1,
  "id": "mira.aurora-loom",
  "name": "Aurora Loom",
  "version": "1.2.0",
  "type": "shader",
  "entry": "aurora.frag",
  "author": { "name": "mira", "url": "https://example.com/mira" },
  "license": "CC-BY-4.0",
  "description": "Slow northern-light curtains over a quiet ridge.",
  "tags": ["space", "calm"],
  "preview": "preview.webp",
  "render": { "maxFps": 30 },
  "permissions": { "input.mouse": true }
}
```

## Fields

| Field | Required | Rules |
|---|---|---|
| `manifestVersion` | yes | `1` |
| `id` | yes | 3 to 64 characters: lowercase letters, digits, dots, single dashes. `creator.name` style is recommended. |
| `name` | yes | 1 to 80 characters |
| `version` | yes | semantic version, e.g. `1.2.0` |
| `type` | yes | see [Types](#types) |
| `entry` | yes | package-relative path to the main file; its extension must match the type |
| `author.name` | yes | 1 to 80 characters |
| `author.url` | no | `https://` only |
| `license` | yes | SPDX expression (`MIT`, `CC-BY-4.0`, `LicenseRef-MyTerms`) |
| `description` | no | up to 500 characters |
| `tags` | no | up to 10, lowercase letters, digits and dashes, up to 24 characters each |
| `preview` | no | `.png`, `.jpg` or `.webp` thumbnail |
| `properties` | no | user-adjustable settings (web wallpapers; native types will follow) |
| `permissions` | no | what the wallpaper asks to use; everything else is denied |
| `slideshow` | slideshow only | images and timing |
| `render` | no | fit, frame cap, background color |

Paths everywhere use `/`, must be relative, printable ASCII, no `..`, no
`:*?"<>|` or backslash, no Windows reserved names (`CON`, `nul.txt`), at most
240 bytes and 16 levels deep. These rules make a package unpack identically on
every OS.

## Types

| `type` | `entry` | Drawn by |
|---|---|---|
| `image` | `.png .jpg .jpeg .webp` | native renderer |
| `slideshow` | one of `slideshow.images` | native renderer |
| `animated` | `.gif .webp .png .apng` | native renderer |
| `shader` | `.wgsl .frag .glsl` | native renderer |
| `video` | `.mp4 .webm` | planned native hardware decoder |
| `web` | `.html .htm` | web view (planned) |
| `theme` | `.json` | not drawn; colors and fonts for the manager and wallpapers |

## render

```json
"render": { "fit": "cover", "maxFps": 30, "background": "#101820" }
```

- `fit`: `cover` (default, fill and crop), `contain` (letterbox), `stretch`,
  `center`.
- `maxFps`: 1 to 240. Animated images use their own frame timing up to this
  cap. Shaders default to 30. The user's governor cap still applies on top.
- `background`: letterbox color, `#RRGGBB`.

## slideshow

```json
"type": "slideshow",
"entry": "dawn.jpg",
"slideshow": { "images": ["dawn.jpg", "noon.jpg", "dusk.jpg"], "interval": 600, "shuffle": false }
```

- `images`: 1 to 1000 image paths. `entry` must be one of them; it is shown
  first.
- `interval`: seconds per image, 5 to 86400. Default 300.
- `shuffle`: random order after the first image.

## Shaders

Both languages use the Shadertoy contract. The pixel coordinate has its
origin at the bottom left.

GLSL (paste from Shadertoy works if the shader doesn't use `iChannel`
textures):

```glsl
void mainImage(out vec4 fragColor, in vec2 fragCoord) {
    vec2 uv = fragCoord / iResolution.xy;
    fragColor = vec4(uv, 0.5 + 0.5 * sin(iTime), 1.0);
}
```

WGSL (uniforms are fields of `ww`):

```wgsl
fn mainImage(fragCoord: vec2<f32>) -> vec4<f32> {
    let uv = fragCoord / ww.iResolution.xy;
    return vec4<f32>(uv, 0.5 + 0.5 * sin(ww.iTime), 1.0);
}
```

Uniforms: `iResolution` (pixels), `iTime`, `iTimeDelta` (seconds),
`iFrame`, `iFrameRate`, `iMouse` (pixels; zero unless `input.mouse` is
granted), `iDate` (year, month 0-11, day, seconds since midnight, UTC).
`#version` and `precision` lines are ignored. Compile errors point at the
line in your file. Output alpha is ignored.

## permissions

| Key | Value | What the user is told |
|---|---|---|
| `audio` | `true` | See what's playing (64 levels, never the microphone or the audio itself) |
| `system.stats` | list of `cpu`, `gpu`, `ram`, `net` | Read total usage once a second |
| `weather` | `true` | City-level weather from Wallweaver |
| `calendar` | `true` | Titles and times of upcoming events from a calendar the user picks |
| `network` | list of origins, at most 8 | Contact exactly these sites |
| `input.mouse` | `true` | Pointer position while the desktop is visible; no clicks or keys |
| `script.eval` | `true` | Allow `eval()` in web wallpapers; flagged as a risk |

Network origins must be `https://` or `wss://`, a public domain name (no IP
addresses, no `localhost`, no `.local`/`.lan`/`.internal` names), no path,
no wildcards, no credentials.

Declaring a permission only makes it possible. The user approves each one,
can revoke it later, and sees a new consent screen when an update asks for
more. Wallpapers you make yourself can be granted extra things the manifest
format doesn't allow for shared packages, such as any website or the local
network; shared packages never can.

## Packages (.wwp)

A `.wwp` file is a zip archive:

```
.wwp/CHECKSUMS    one line per file: sha256, size, path (sorted, canonical)
.wwp/SIGNATURE    ed25519 signature over CHECKSUMS, with the signer's public key
wallpaper.json
...your files
```

`wallweaver pack` builds it deterministically (sorted, fixed timestamps), so
the same folder always gives the same bytes. Size limits: 500 MB for video,
200 MB for slideshows, 50 MB for everything else, 2000 files. Secret key
files (`.wwkey`) are refused if they are found in the folder being packed.
