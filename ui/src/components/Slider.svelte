<script lang="ts">
  import { percentOf } from "../lib/format";
  let { value = $bindable(0), min = 0, max = 1, step = 0.01, label, valueText = undefined, ticks = undefined, activeTick = -1, onchange = undefined }: {
    value?: number; min?: number; max?: number; step?: number; label: string; valueText?: string;
    ticks?: string[]; activeTick?: number; onchange?: (v: number) => void;
  } = $props();
  const pct = $derived(percentOf(value, min, max));
</script>

<div class="slider" class:has-ticks={ticks}>
  <div class="track"></div>
  <div class="fill" style:width="{pct}%"></div>
  <div class="thumb" style:left="{pct}%"></div>
  <input type="range" {min} {max} {step} bind:value aria-label={label} aria-valuetext={valueText} onchange={() => onchange?.(value)} />
  {#if ticks}
    <div class="ticks" aria-hidden="true">
      {#each ticks as t, i (i)}<span class:on={i === activeTick}>{t}</span>{/each}
    </div>
  {/if}
</div>

<style>
  .slider input { position: absolute; inset: -4px 0; width: 100%; height: 26px; margin: 0; opacity: 0; cursor: pointer; }
  .slider:has(input:focus-visible) .thumb { box-shadow: 0 0 0 4px var(--accent), 0 2px 6px rgba(0, 0, 0, 0.4); }
  .has-ticks { margin-bottom: 16px; }
  .ticks .on { color: var(--ink); }
</style>
