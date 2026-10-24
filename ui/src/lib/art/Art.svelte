<script lang="ts">
  import { onMount } from "svelte";
  import type { ArtSpec } from "../daemon/types";
  import { renderArt } from "./cache";

  let { spec, label = "" }: { spec: ArtSpec; label?: string } = $props();
  let canvas: HTMLCanvasElement;
  let visible = $state(false);
  let size = $state({ w: 0, h: 0 });

  onMount(() => {
    const io = new IntersectionObserver((entries) => {
      if (entries.some((e) => e.isIntersecting)) { visible = true; io.disconnect(); }
    }, { rootMargin: "200px" });
    io.observe(canvas);
    const ro = new ResizeObserver(() => {
      const r = canvas.getBoundingClientRect();
      const w = Math.round(r.width), h = Math.round(r.height);
      if (Math.abs(w - size.w) > 1 || Math.abs(h - size.h) > 1) size = { w, h };
    });
    ro.observe(canvas);
    return () => { io.disconnect(); ro.disconnect(); };
  });

  $effect(() => {
    const { w, h } = size;
    const s = spec;
    if (!visible || w < 1 || h < 1) return;
    let cancelled = false;
    const dpr = Math.min(2, window.devicePixelRatio || 1);
    renderArt(s, w, h, dpr).then((src) => {
      if (cancelled) return;
      canvas.width = src.width;
      canvas.height = src.height;
      canvas.getContext("2d")?.drawImage(src, 0, 0);
      canvas.dataset.drawn = "1";
    });
    return () => { cancelled = true; };
  });
</script>

<canvas bind:this={canvas} class="art" role={label ? "img" : undefined} aria-label={label || undefined} aria-hidden={label ? undefined : "true"}></canvas>
