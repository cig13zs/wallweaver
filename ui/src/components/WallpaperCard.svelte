<script lang="ts">
  import Icon from "../lib/Icon.svelte";
  import Art from "../lib/art/Art.svelte";
  import CostMeter from "./CostMeter.svelte";
  import type { WallpaperSummary } from "../lib/daemon/types";
  import { avatarGradient, formatCount, formatPct } from "../lib/format";
  import { href } from "../lib/stores/router";

  let { w, liked = false, showCost = true, onset, onlike }: {
    w: WallpaperSummary; liked?: boolean; showCost?: boolean; onset: (w: WallpaperSummary) => void; onlike: (w: WallpaperSummary) => void;
  } = $props();
  const kindLabel = $derived(w.kind[0]!.toUpperCase() + w.kind.slice(1));
</script>

<article class="card-w" aria-label={w.title}>
  <div class="thumb">
    <Art spec={w.art} />
    <a class="cover" href={href("wallpaper", [w.id])} data-card-link aria-label="{w.title} by {w.author}, {kindLabel}"></a>
    {#if showCost}
      <div class="tl"><span class="base"><CostMeter cost={w.cost} /></span><span class="gpu-chip"><CostMeter cost={w.cost} extra="{formatPct(w.gpuPct, 0)} GPU" /></span></div>
    {/if}
    <div class="tr">
      <span class="badge badge-glass previewing"><span class="pulse"></span>Previewing</span>
      <span class="rest">
        {#each w.uses as u (u)}<span class="ico-b" title={u === "audio" ? "Reacts to audio" : "Follows the mouse"}><Icon name={u} /></span>{/each}
        {#if w.installed}<span class="ico-b installed" title="In your library"><Icon name="check" /><span class="sr-only">Installed</span></span>{/if}
      </span>
    </div>
    <div class="overlay">
      <div class="row gap-6">
        <button type="button" class="btn btn-primary btn-sm grow" onclick={() => onset(w)}><Icon name="monitor" />Set as wallpaper</button>
        <a class="ico-b sq" href={href("wallpaper", [w.id])} aria-label="Customize {w.title}"><Icon name="sliders" /></a>
        <button type="button" class="ico-b sq" class:liked aria-label="Like {w.title}" aria-pressed={liked} onclick={() => onlike(w)}><Icon name="heart" /></button>
      </div>
    </div>
  </div>
  <div class="meta">
    <div class="who">
      <div class="ttl">{w.title}</div>
      <div class="by"><span class="av" style:background={avatarGradient(w.author)}></span>{w.author} · {kindLabel}</div>
    </div>
    <span class="stat"><Icon name="heart" /><span class="sr-only">Likes:</span>{formatCount(w.likes + (liked ? 1 : 0))}</span>
  </div>
</article>

<style>
  .card-w { position: relative; }
  .card-w .thumb { aspect-ratio: 16/9.2; box-shadow: 0 0 0 1px var(--line); transition: transform 0.18s ease, box-shadow 0.18s ease; }
  .cover { position: absolute; inset: 0; z-index: 1; border-radius: inherit; }
  .cover:focus-visible { outline-offset: -2px; }
  .tl { position: absolute; left: 8px; top: 8px; z-index: 2; display: flex; gap: 6px; pointer-events: none; }
  .tr { position: absolute; right: 8px; top: 8px; z-index: 2; display: flex; gap: 6px; pointer-events: none; }
  .rest { display: flex; gap: 6px; }
  .meta { display: flex; align-items: center; gap: 8px; padding: 9px 2px 0; }
  .who { min-width: 0; }
  .ttl { font-weight: 600; font-size: 13px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .by { font-size: 11.5px; color: var(--ink-3); display: flex; align-items: center; gap: 5px; }
  .av { width: 14px; height: 14px; border-radius: 50%; flex: none; }
  .stat { margin-left: auto; font-size: 11.5px; color: var(--ink-3); display: flex; gap: 4px; align-items: center; }
  .stat :global(svg) { width: 13px; height: 13px; }
  .ico-b { width: 22px; height: 22px; display: grid; place-items: center; border-radius: 99px; background: rgba(12,10,20,0.6); color: #fff; backdrop-filter: blur(10px); box-shadow: inset 0 0 0 1px rgba(255,255,255,0.12); }
  .ico-b :global(svg) { width: 12px; height: 12px; }
  .ico-b.installed { background: rgba(111,227,180,.9); color: #0b2a1e; }
  .ico-b.sq { width: 28px; height: 28px; border-radius: 8px; pointer-events: auto; }
  .ico-b.sq:hover { background: rgba(255,255,255,0.18); }
  .ico-b.liked { color: #ff9cc0; }
  .previewing, .gpu-chip, .overlay { display: none; }
  .pulse { width: 6px; height: 6px; border-radius: 50%; background: #6fe3b4; }
  .overlay { position: absolute; inset: 0; z-index: 2; background: linear-gradient(0deg, rgba(8,6,16,0.82) 0%, rgba(8,6,16,0) 55%); flex-direction: column; justify-content: flex-end; padding: 10px; gap: 8px; pointer-events: none; }
  .overlay .btn { pointer-events: auto; }
  /* Hover or keyboard focus: lift, ring, show the quick actions */
  .card-w:hover .thumb, .card-w:focus-within .thumb { transform: translateY(-3px) scale(1.035); box-shadow: 0 0 0 2px var(--accent), 0 18px 40px rgba(0,0,0,0.5); z-index: 3; }
  .card-w:hover .overlay, .card-w:focus-within .overlay { display: flex; }
  .card-w:hover .previewing, .card-w:focus-within .previewing { display: inline-flex; }
  .card-w:hover .rest, .card-w:focus-within .rest { display: none; }
  .card-w:hover .base, .card-w:focus-within .base { display: none; }
  .card-w:hover .gpu-chip, .card-w:focus-within .gpu-chip { display: inline-flex; }
  @media (prefers-reduced-motion: reduce) {
    .card-w:hover .thumb, .card-w:focus-within .thumb { transform: none; }
  }
  :global(:root[data-motion="reduce"]) .card-w:hover .thumb, :global(:root[data-motion="reduce"]) .card-w:focus-within .thumb { transform: none; }
</style>
