Wallweaver 0.1.0-alpha.1 on linux x86_64, Intel(R) Xeon(R) Processor @ 2.10GHz (4 cores, 16091 MiB RAM), adapter: llvmpipe (LLVM 20.1.2, 256 bits) (Vulkan, Cpu), driver llvmpipe Mesa 25.2.8-0ubuntu0.24.04.4 (LLVM 20.1.2) (software: GPU work runs on the CPU, so CPU numbers are much higher than on real hardware). Target 1920x1080 at a 60 fps cap.

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
