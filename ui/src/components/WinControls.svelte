<script lang="ts">
  import Icon from "../lib/Icon.svelte";
  import { inShell } from "../lib/daemon";

  // Only the desktop shell has a window to control; in a browser tab these do nothing.
  async function act(what: "minimize" | "toggleMaximize" | "close") {
    if (!inShell()) return;
    const { getCurrentWindow } = await import("@tauri-apps/api/window");
    await getCurrentWindow()[what]();
  }
</script>

<div class="winctl">
  <button type="button" aria-label="Minimize" onclick={() => act("minimize")}><Icon name="minus" /></button>
  <button type="button" aria-label="Maximize" onclick={() => act("toggleMaximize")}><Icon name="square" /></button>
  <button type="button" aria-label="Close" class="close" onclick={() => act("close")}><Icon name="x" /></button>
</div>

<style>
  .winctl button { width: 46px; height: 100%; display: grid; place-items: center; color: var(--ink-3); border-radius: 0; }
  .winctl button:hover { background: var(--panel-2); color: var(--ink); }
  .winctl .close:hover { background: #c42b1c; color: #fff; }
  .winctl :global(svg) { width: 14px; height: 14px; }
</style>
