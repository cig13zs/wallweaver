<script lang="ts">
  import Icon from "../lib/Icon.svelte";
  import Logo from "../lib/Logo.svelte";
  import Art from "../lib/art/Art.svelte";
  import type { IconName } from "../lib/icons";
  import { href, type ScreenName } from "../lib/stores/router";
  import { status, setPausedAll } from "../lib/stores/status";
  import { attempt } from "../lib/stores/toast";
  import { pluralize } from "../lib/format";

  let { active, rail = false }: { active: ScreenName; rail?: boolean } = $props();

  type Item = { id: ScreenName; icon: IconName; label: string; count?: "library" | "playlists" };
  const groups: { label?: string; items: Item[] }[] = [
    { items: [
      { id: "discover", icon: "compass", label: "Discover" },
      { id: "library", icon: "grid", label: "Library", count: "library" },
      { id: "playlists", icon: "playlist", label: "Playlists", count: "playlists" },
      { id: "creator", icon: "sparkles", label: "Creator Studio" },
    ] },
    { label: "System", items: [
      { id: "performance", icon: "gauge", label: "Performance" },
      { id: "permissions", icon: "shield", label: "Permissions" },
      { id: "themes", icon: "palette", label: "Themes" },
      { id: "settings", icon: "settings", label: "Settings" },
    ] },
    { label: "Project", items: [{ id: "supporter", icon: "heart", label: "Support Wallweaver" }] },
  ];
  const current = (id: ScreenName) => id === active || (id === "discover" && active === "wallpaper");
  const count = (c: Item["count"]) => c === "library" ? $status?.libraryCount : c === "playlists" ? $status?.playlistCount : undefined;
</script>

<nav class="sidebar" aria-label="Main">
  <div class="brand"><Logo size={28} /><span class="word">Wallweaver</span>{#if $status}<span class="ver">{$status.version}</span>{/if}</div>
  {#each groups as g (g.label ?? "main")}
    {#if g.label}<div class="nav-label t-eyebrow" aria-hidden="true">{g.label}</div>{/if}
    {#each g.items as it (it.id)}
      <a class="nav-item" class:active={current(it.id)} href={href(it.id)} aria-current={current(it.id) ? "page" : undefined} title={rail ? it.label : undefined}>
        <Icon name={it.icon} /><span>{it.label}</span>
        {#if count(it.count) !== undefined}<span class="count" aria-label="{count(it.count)} items">{count(it.count)}</span>{/if}
      </a>
    {/each}
  {/each}
  {#if $status?.nowPlaying}
    {@const np = $status.nowPlaying}
    <div class="now-card">
      <div class="thumb now-thumb"><Art spec={np.art} /></div>
      <div class="now-text">
        <div class="t-small now-title">{np.title}</div>
        <div class="t-micro ink-3">{$status.paused ? "Paused" : `On ${pluralize(np.displays, "display")}`}</div>
      </div>
      <button type="button" class="icon-btn now-btn" aria-label={$status.paused ? "Resume all wallpapers" : "Pause all wallpapers"}
        onclick={() => attempt(() => setPausedAll(!$status?.paused), "Couldn't reach the wallpaper service")}>
        <Icon name={$status.paused ? "play" : "pause"} />
      </button>
    </div>
  {/if}
</nav>

<style>
  .sidebar { overflow: hidden; }
  .now-thumb { width: 52px; height: 34px; border-radius: 6px; }
  .now-text { min-width: 0; }
  .now-title { font-weight: 600; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .now-btn { width: 26px; height: 26px; }
</style>
