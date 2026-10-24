<script lang="ts" generics="T extends string | number">
  import Icon from "../lib/Icon.svelte";
  import type { IconName } from "../lib/icons";

  let { options, value = $bindable(), label, onchange = undefined, wide = false }: {
    options: { value: T; label: string; icon?: IconName }[]; value: T; label: string; onchange?: (v: T) => void; wide?: boolean;
  } = $props();
  let root: HTMLDivElement;

  function pick(v: T) {
    if (v === value) return;
    value = v;
    onchange?.(v);
  }
  function key(e: KeyboardEvent) {
    const dir = e.key === "ArrowRight" || e.key === "ArrowDown" ? 1 : e.key === "ArrowLeft" || e.key === "ArrowUp" ? -1 : 0;
    if (!dir) return;
    e.preventDefault();
    const i = options.findIndex((o) => o.value === value);
    const next = options[(i + dir + options.length) % options.length];
    if (!next) return;
    pick(next.value);
    queueMicrotask(() => root.querySelector<HTMLElement>('[aria-checked="true"]')?.focus());
  }
</script>

<div class="seg" class:wide role="radiogroup" aria-label={label} bind:this={root} onkeydown={key} tabindex="-1">
  {#each options as o (o.value)}
    <button type="button" role="radio" aria-checked={o.value === value} tabindex={o.value === value ? 0 : -1} class:on={o.value === value} onclick={() => pick(o.value)}>
      {#if o.icon}<Icon name={o.icon} />{/if}{o.label}
    </button>
  {/each}
</div>

<style>
  .wide { display: flex; width: 100%; }
  .wide > button { flex: 1; justify-content: center; }
</style>
