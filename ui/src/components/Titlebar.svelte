<script lang="ts">
  import type { Snippet } from "svelte";
  import WinControls from "./WinControls.svelte";
  let { children, class: cls = "" }: { children: Snippet; class?: string } = $props();
</script>

<!-- The whole bar drags the window in the desktop shell; controls inside it stay clickable. -->
<header class="titlebar {cls}" data-tauri-drag-region>
  {@render children()}
  <WinControls />
</header>
