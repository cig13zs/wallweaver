<script lang="ts">
  import Titlebar from "../components/Titlebar.svelte";
  import Icon from "../lib/Icon.svelte";
  import type { IconName } from "../lib/icons";
  import { href, type Route } from "../lib/stores/router";

  let { route }: { route: Route } = $props();
  const INFO: Record<string, { title: string; icon: IconName; text: string }> = {
    library: { title: "Library", icon: "grid", text: "Everything you've installed or made, with filters for type, cost and display." },
    playlists: { title: "Playlists", icon: "playlist", text: "Rotate wallpapers on a timer or by time of day, per display." },
    permissions: { title: "Permissions", icon: "shield", text: "Every permission each wallpaper holds, with one-click revoke." },
    themes: { title: "Themes", icon: "palette", text: "Accent colours and fonts that wallpapers and this window can share." },
  };
  const info = $derived(INFO[route.name] ?? INFO.library!);
</script>

<Titlebar>
  <h1 class="t-h3 page-title">{info.title}</h1>
  <span class="spacer" data-tauri-drag-region></span>
</Titlebar>
<section class="content">
  <div class="empty">
    <span class="ic"><Icon name={info.icon} /></span>
    <h2 class="t-h2">{info.title} is next</h2>
    <p class="t-body ink-2">{info.text}</p>
    <a class="btn btn-secondary" href={href("discover")}><Icon name="compass" />Back to Discover</a>
  </div>
</section>

<style>
  .page-title { font-size: 15px; }
  .spacer { align-self: stretch; }
  .empty { max-width: 420px; margin: 80px auto 0; display: flex; flex-direction: column; align-items: center; gap: 12px; text-align: center; }
  .ic { width: 48px; height: 48px; border-radius: 14px; display: grid; place-items: center; background: var(--accent-soft); color: var(--accent); }
  .ic :global(svg) { width: 22px; height: 22px; }
</style>
