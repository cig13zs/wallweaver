# Video backend design

Video is the most expensive wallpaper type, so it gets the most care. It is
not in this build; this is the plan the `ww-render::video` traits are shaped
around.

## Goals

- Hardware decode, with decoded frames staying on the GPU. No copy back to
  system memory, no CPU color conversion.
- A 4K60 H.264/HEVC/VP9/AV1 loop under 3% CPU on a machine with a hardware
  decoder.
- Paused means the decoder is paused; stopped means it is closed and its
  surfaces freed.

## Per platform

| Platform | Decoder | Getting frames into wgpu |
|---|---|---|
| Windows | Media Foundation source reader with a DXGI device manager | NV12 into a shareable D3D11 texture, opened in the D3D12 device through a shared NT handle, wrapped with wgpu's DX12 interop |
| macOS | VideoToolbox decompression session | IOSurface-backed `CVPixelBuffer`s turned into Metal textures with `CVMetalTextureCache` |
| Linux | VA-API | export as DMA-BUF, import into Vulkan as external memory (`VK_EXT_external_memory_dma_buf`) |
| fallback | libmpv render API | for codecs the platform decoder lacks; costs one GPU copy |

## In the renderer

- A `VideoDecoder` produces `VideoFrame`s (GPU textures with a presentation
  time). The video backend samples NV12 or P010 planes directly and converts
  to RGB in the fragment shader.
- Pacing uses presentation timestamps through the same `NextChange::At`
  path as animated images, so the frame cap and pausing work unchanged.
- Looping rewinds the decoder at end of stream; short clips can be kept as a
  ring of decoded surfaces to avoid re-decoding.
- The governor's Throttled state lowers the presented frame rate, not the
  decode rate, unless the clip's frame rate is above the cap, in which case
  frames are dropped before conversion.

## Why not a web view `<video>`

It works everywhere, but it brings a browser engine along for one video
element. The point of the native path is that a video wallpaper costs the
decoder, one shader and nothing else.
