<script lang="ts">
  import { onMount, type Component } from "svelte";
  import Sidebar from "./components/Sidebar.svelte";
  import Toasts from "./components/Toasts.svelte";
  import { route, navigate, href, type ScreenName } from "./lib/stores/router";
  import { refreshStatus, setPausedAll, status } from "./lib/stores/status";
  import { applyAppearance, watchSystemTheme } from "./lib/stores/theme";
  import { attempt } from "./lib/stores/toast";
  import { daemon } from "./lib/daemon";

  type Loader = () => Promise<{ default: Component<any> }>;
  // Each screen is its own chunk; only the shell ships in the first bundle.
  const screens: Record<ScreenName, Loader> = {
    discover: () => import("./screens/Discover.svelte"),
    wallpaper: () => import("./screens/Detail.svelte"),
    performance: () => import("./screens/Performance.svelte"),
    creator: () => import("./screens/Creator.svelte"),
    supporter: () => import("./screens/Supporter.svelte"),
    settings: () => import("./screens/Settings.svelte"),
    library: () => import("./screens/Placeholder.svelte"),
    playlists: () => import("./screens/Placeholder.svelte"),
    permissions: () => import("./screens/Placeholder.svelte"),
    themes: () => import("./screens/Placeholder.svelte"),
  };

  const loading = $derived(screens[$route.name]());
  let statusError = $state("");

  onMount(() => {
    const stop = watchSystemTheme();
    const check = () => refreshStatus().then(() => { statusError = ""; }).catch((e) => { statusError = String(e); });
    check();
    daemon().then((d) => d.settings()).then((s) => applyAppearance({ theme: s.theme, motion: s.motion })).catch(() => {});
    const poll = setInterval(check, 5000);
    return () => { stop(); clearInterval(poll); };
  });

  function onKey(e: KeyboardEvent) {
    const mod = e.ctrlKey || e.metaKey;
    if (mod && !e.altKey && e.key.toLowerCase() === "k") {
      e.preventDefault();
      const search = document.querySelector<HTMLInputElement>("[data-search]");
      if (search) search.focus();
      else { navigate(href("discover")); setTimeout(() => document.querySelector<HTMLInputElement>("[data-search]")?.focus(), 150); }
    } else if (mod && e.altKey && e.key.toLowerCase() === "p") {
      e.preventDefault();
      attempt(() => setPausedAll(!$status?.paused), "Couldn't reach the wallpaper service");
    }
  }
</script>

<svelte:window onkeydown={onKey} />

<a class="skip" href="#main">Skip to content</a>
<div class="app" class:rail={$route.name === "creator"}>
  <Sidebar active={$route.name} rail={$route.name === "creator"} />
  <main class="main" id="main" tabindex="-1">
    {#if statusError}
      <div class="offline" role="alert">Can't reach the wallpaper service. Wallpapers keep running; this window will retry.</div>
    {/if}
    {#await loading}
      <div class="loading" aria-busy="true"></div>
    {:then mod}
      {#key $route.name + $route.params.join("/")}
        <mod.default route={$route} />
      {/key}
    {:catch err}
      <div class="loading" role="alert">This screen failed to load: {String(err)}</div>
    {/await}
  </main>
</div>
<Toasts />

<style>
  .skip { position: absolute; left: 8px; top: -40px; z-index: 200; padding: 8px 12px; border-radius: 8px; background: var(--accent); color: #120f22; font-weight: 600; }
  .skip:focus { top: 8px; }
  .loading { flex: 1; display: grid; place-items: center; color: var(--ink-3); }
  .offline { padding: 8px 24px; background: var(--warn-soft); color: var(--warn); font-size: 12px; }
  .main:focus { outline: none; }
</style>
