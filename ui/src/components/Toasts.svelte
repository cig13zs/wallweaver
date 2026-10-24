<script lang="ts">
  import { toasts } from "../lib/stores/toast";
  import Icon from "../lib/Icon.svelte";
</script>

<div class="toasts" role="status" aria-live="polite">
  {#each $toasts as t (t.id)}
    <div class="toast {t.tone}"><Icon name={t.tone === "bad" ? "alert" : t.tone === "info" ? "info" : "check"} />{t.text}</div>
  {/each}
</div>

<style>
  .toasts { position: fixed; right: 20px; bottom: 20px; z-index: 100; display: flex; flex-direction: column; gap: 8px; align-items: flex-end; pointer-events: none; }
  .toast { display: flex; align-items: center; gap: 8px; max-width: 460px; padding: 10px 14px; border-radius: 12px; background: var(--bg-elev); color: var(--ink); box-shadow: var(--shadow-pop); font-size: 12.5px; animation: rise 0.18s ease-out; }
  .toast :global(svg) { width: 15px; height: 15px; }
  .ok :global(svg) { color: var(--ok); }
  .info :global(svg) { color: var(--info); }
  .bad :global(svg) { color: var(--bad); }
  @keyframes rise { from { transform: translateY(6px); opacity: 0; } }
</style>
