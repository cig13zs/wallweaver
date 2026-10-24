<script lang="ts">
  import { onMount, type Component } from "svelte";
  import Titlebar from "../components/Titlebar.svelte";
  import WallpaperCard from "../components/WallpaperCard.svelte";
  import CostMeter from "../components/CostMeter.svelte";
  import Toggle from "../components/Toggle.svelte";
  import Icon from "../lib/Icon.svelte";
  import Art from "../lib/art/Art.svelte";
  import type { IconName } from "../lib/icons";
  import { daemon } from "../lib/daemon";
  import type { Featured, GallerySort, WallpaperDetail, WallpaperSummary } from "../lib/daemon/types";
  import { avatarGradient, formatPct, initials } from "../lib/format";
  import { href, navigate, type Route } from "../lib/stores/router";
  import { status } from "../lib/stores/status";
  import { attempt } from "../lib/stores/toast";
  import { applyEverywhere } from "../lib/actions";

  let { route }: { route: Route } = $props();

  const CHIPS: { id: string; label: string; dot?: string; square?: boolean; icon?: IconName }[] = [
    { id: "all", label: "All" },
    { id: "anime", label: "Anime", dot: "#ff9cc0" },
    { id: "nature", label: "Nature", dot: "#6fe3b4" },
    { id: "space", label: "Space", dot: "#a596ff" },
    { id: "minimal", label: "Minimal", dot: "#b9b5c9" },
    { id: "audio", label: "Audio-reactive", icon: "audio" },
    { id: "interactive", label: "Interactive", icon: "mouse" },
    { id: "pixel", label: "Pixel", dot: "#f5c56e", square: true },
    { id: "cyberpunk", label: "Cyberpunk", dot: "#36e6ff" },
  ];

  let featured = $state<Featured | null>(null);
  let slide = $state(0);
  let heroes = $state<Record<string, WallpaperDetail>>({});
  let items = $state<WallpaperSummary[] | null>(null);
  let text = $state("");
  let category = $state("all");
  let lowImpactOnly = $state(false);
  let sort = $state<GallerySort>("trending");
  let liked = $state<Record<string, boolean>>({});
  let showCost = $state(true);
  let bellOpen = $state(false);
  let Consent = $state<Component<any> | null>(null);

  const heroId = $derived(featured?.hero[slide]?.id);
  const hero = $derived(heroId ? heroes[heroId] : undefined);

  onMount(async () => {
    await attempt(async () => {
      const d = await daemon();
      featured = await d.featured();
      const [first] = featured.hero;
      if (first) heroes[first.id] = await d.wallpaper(first.id);
      showCost = (await d.performance()).settings.showCostBadges;
    }, "Couldn't load the gallery");
  });

  // Load each hero slide's details the first time it is shown.
  $effect(() => {
    const id = heroId;
    if (!id || heroes[id]) return;
    daemon().then((d) => d.wallpaper(id)).then((w) => { heroes[id] = w; }).catch(() => {});
  });

  // Re-query whenever a filter changes; a short debounce keeps typing smooth.
  $effect(() => {
    const q = { text, category, lowImpactOnly, sort };
    const t = setTimeout(() => {
      attempt(async () => { items = await (await daemon()).gallery(q); }, "Couldn't load the gallery");
    }, q.text ? 120 : 0);
    return () => clearTimeout(t);
  });

  // ?install=<id> opens the consent dialog for that wallpaper.
  const installId = $derived(route.query.install);
  $effect(() => {
    if (installId && !Consent) import("../components/ConsentDialog.svelte").then((m) => { Consent = m.default; });
  });

  function setWallpaper(w: Pick<WallpaperSummary, "id" | "title" | "installed">) {
    if (w.installed) applyEverywhere(w);
    else navigate(href("discover", [], { install: w.id }));
  }
  function closeConsent(installed: boolean) {
    navigate(href("discover"));
    if (installed) attempt(async () => { items = await (await daemon()).gallery({ text, category, lowImpactOnly, sort }); });
  }
  function like(w: { id: string }) {
    liked[w.id] = !liked[w.id];
  }
  function go(delta: number) {
    const n = featured?.hero.length ?? 0;
    if (n) slide = (slide + delta + n) % n;
  }

  // Arrow keys move between cards like a grid; Tab still walks every control.
  function gridKeys(e: KeyboardEvent) {
    const keys: Record<string, number> = { ArrowLeft: -1, ArrowRight: 1 };
    const grid = e.currentTarget as HTMLElement;
    const cols = getComputedStyle(grid).gridTemplateColumns.split(" ").length;
    const step = keys[e.key] ?? (e.key === "ArrowDown" ? cols : e.key === "ArrowUp" ? -cols : 0);
    if (!step) return;
    const links = [...grid.querySelectorAll<HTMLElement>("[data-card-link]")];
    const i = links.indexOf(document.activeElement as HTMLElement);
    if (i < 0) return;
    const next = links[i + step];
    if (next) { e.preventDefault(); next.focus(); }
  }
</script>

<Titlebar>
  <label class="search">
    <Icon name="search" />
    <span class="sr-only">Search</span>
    <input class="grow" type="search" placeholder="Search wallpapers, creators, tags…" bind:value={text} data-search />
    <span class="kbd" aria-hidden="true">Ctrl K</span>
  </label>
  <a class="btn btn-ghost btn-sm import" href={href("creator")}><Icon name="download" />Import local wallpapers</a>
  <span class="spacer" data-tauri-drag-region></span>
  {#if $status}
    <span class="status">
      <span class="d" class:off={$status.paused}></span>
      {$status.paused ? "All paused" : `${$status.displaysLive} displays live`}
      <span class="ink-3">·</span> <span class="mono tnum">{formatPct($status.cpuPct)}</span> CPU
    </span>
  {/if}
  <div class="bell">
    <button type="button" class="icon-btn" aria-label="Notifications" aria-expanded={bellOpen} onclick={() => (bellOpen = !bellOpen)}><Icon name="bell" /></button>
    {#if bellOpen}<div class="pop" role="status">You're all caught up.</div>{/if}
  </div>
</Titlebar>

<section class="content">
  <div class="hero-wrap">
    <div class="hero" aria-roledescription="carousel" aria-label="Featured wallpapers">
      {#if hero}
        <Art spec={hero.art} label="{hero.title} preview" />
        <div class="hero-body">
          <div class="row gap-10"><span class="live-pill"><span class="pulse"></span>Featured this week</span></div>
          <h1 class="t-display hero-title">{hero.title}</h1>
          <p>{featured?.hero[slide]?.blurb}</p>
          <div class="hero-meta">
            <span class="avatar hero-av" style:background={avatarGradient(hero.author)}>{initials(hero.author)}</span>
            <span class="hero-author">{hero.author}</span><span>·</span><span class="cap">{hero.kind}</span><span>·</span><span>{hero.license}</span><span>·</span>
            <CostMeter cost={hero.cost} extra="{formatPct(hero.gpuPct, 0)} GPU" />
          </div>
          <div class="row gap-8">
            <button type="button" class="btn btn-primary btn-lg" onclick={() => hero && setWallpaper(hero)}><Icon name="monitor" />Set as wallpaper</button>
            <a class="btn btn-glass btn-lg" href={href("wallpaper", [hero.id])}><Icon name="sliders" />Customize</a>
            <button type="button" class="btn btn-glass btn-lg sq" aria-label="Like {hero.title}" aria-pressed={!!liked[hero.id]} class:liked={liked[hero.id]} onclick={() => hero && like(hero)}><Icon name="heart" /></button>
          </div>
        </div>
        <div class="hero-ctl">
          <div class="dots" role="group" aria-label="Choose slide">
            {#each featured?.hero ?? [] as h, i (h.id)}
              <button type="button" class:on={i === slide} aria-label="Slide {i + 1}" aria-current={i === slide ? "true" : undefined} onclick={() => (slide = i)}></button>
            {/each}
          </div>
          <button type="button" class="btn btn-glass btn-sm sq30" aria-label="Previous slide" onclick={() => go(-1)}><Icon name="chevleft" /></button>
          <button type="button" class="btn btn-glass btn-sm sq30" aria-label="Next slide" onclick={() => go(1)}><Icon name="chevright" /></button>
        </div>
      {/if}
    </div>
    <div class="side-feat">
      {#each featured?.side ?? [] as s (s.id)}
        {@const w = items?.find((x) => x.id === s.id)}
        <a class="mini" href={href("wallpaper", [s.id])}>
          <Art spec={s.art} />
          <div class="top"><span class="badge badge-glass">{s.badge}</span></div>
          {#if w}
            <div class="lbl">
              <div><div class="mini-title">{w.title}</div><div class="t-micro mini-sub">{w.author} · <span class="cap">{w.category}</span> · <span class="cap">{w.kind}</span></div></div>
              <CostMeter cost={w.cost} />
            </div>
          {/if}
        </a>
      {/each}
    </div>
  </div>

  <div class="filters">
    <div class="chips" role="group" aria-label="Filter by category">
      {#each CHIPS as c (c.id)}
        <button type="button" class="chip" class:active={category === c.id} aria-pressed={category === c.id} onclick={() => (category = c.id)}>
          {#if c.dot}<span class="dot" class:square={c.square} style:background={c.dot}></span>{/if}
          {#if c.icon}<Icon name={c.icon} />{/if}{c.label}
        </button>
      {/each}
    </div>
    <span class="spacer"></span>
    <span class="row gap-8 t-small ink-2 nowrap"><Toggle bind:checked={lowImpactOnly} label="Low impact only" small />Low impact only</span>
    <select class="select sort" bind:value={sort} aria-label="Sort by">
      <option value="trending">Trending</option>
      <option value="new">Newest</option>
      <option value="likes">Most liked</option>
      <option value="lightest">Lightest first</option>
    </select>
  </div>

  {#if items && items.length === 0}
    <div class="empty">Nothing matches. Try another tag or turn off "Low impact only".</div>
  {:else}
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="grid" onkeydown={gridKeys}>
      {#each items ?? [] as w (w.id)}
        <WallpaperCard {w} {showCost} liked={!!liked[w.id]} onset={setWallpaper} onlike={like} />
      {/each}
    </div>
  {/if}
</section>

{#if installId && Consent}
  <Consent id={installId} onclose={closeConsent} />
{/if}

<style>
  .search { cursor: text; }
  .search input { border: 0; background: none; outline: none; min-width: 0; color: var(--ink); }
  .search input::placeholder { color: var(--ink-3); }
  .search input::-webkit-search-cancel-button { display: none; }
  .search:focus-within { box-shadow: inset 0 0 0 1px var(--accent-line), 0 0 0 3px var(--accent-soft); }
  .import { gap: 6px; }
  .spacer { align-self: stretch; }
  .status { display: inline-flex; align-items: center; gap: 8px; font-size: 12px; color: var(--ink-2); height: 30px; padding: 0 12px; border-radius: 99px; background: var(--panel); box-shadow: inset 0 0 0 1px var(--line); white-space: nowrap; }
  .status .d { width: 7px; height: 7px; border-radius: 50%; background: var(--ok); }
  .status .d.off { background: var(--warn); }
  .bell { position: relative; }
  .pop { position: absolute; right: 0; top: 40px; z-index: 20; padding: 10px 14px; border-radius: 10px; background: var(--bg-elev); box-shadow: var(--shadow-pop); font-size: 12px; white-space: nowrap; }

  .hero-wrap { display: grid; grid-template-columns: 1fr 292px; gap: 14px; height: 292px; }
  .hero { position: relative; border-radius: var(--r-xl); overflow: hidden; box-shadow: var(--shadow-1); background: #15131f; }
  .hero :global(canvas) { position: absolute; inset: 0; }
  .hero::after { content: ""; position: absolute; inset: 0; pointer-events: none; background: linear-gradient(90deg, rgba(6,6,14,0.78) 0%, rgba(6,6,14,0.35) 42%, rgba(6,6,14,0) 70%); }
  :global(:root[data-theme="light"]) .hero::after { background: linear-gradient(90deg, rgba(6,6,14,0.7) 0%, rgba(6,6,14,0.3) 42%, rgba(6,6,14,0) 70%); }
  .hero-body { position: absolute; z-index: 2; left: 32px; bottom: 28px; top: 28px; width: 440px; display: flex; flex-direction: column; color: #fff; }
  .hero-title { margin-top: 14px; }
  .hero-body p { color: rgba(255,255,255,0.78); font-size: 13.5px; margin-top: 10px; max-width: 400px; }
  .hero-meta { display: flex; gap: 8px; align-items: center; margin-top: auto; margin-bottom: 18px; color: rgba(255,255,255,0.8); font-size: 12px; }
  .hero-av { width: 20px; height: 20px; font-size: 9px; }
  .hero-author { color: #fff; font-weight: 600; }
  .cap { text-transform: capitalize; }
  .hero-ctl { position: absolute; right: 20px; bottom: 20px; z-index: 2; display: flex; align-items: center; gap: 10px; }
  .dots { display: flex; gap: 6px; padding: 0 6px; }
  .dots button { width: 6px; height: 6px; padding: 0; border-radius: 99px; background: rgba(255,255,255,0.35); display: block; transition: width 0.2s; }
  .dots button.on { width: 22px; background: #fff; }
  .sq { width: 42px; padding: 0; }
  .sq30 { width: 30px; padding: 0; }
  .liked { color: #ff9cc0; }
  .live-pill { display: inline-flex; align-items: center; gap: 6px; font-size: 11px; font-weight: 600; color: #fff; letter-spacing: 0.04em; text-transform: uppercase; }
  .live-pill .pulse { width: 7px; height: 7px; border-radius: 50%; background: #6fe3b4; box-shadow: 0 0 0 4px rgba(111,227,180,0.25); }
  .side-feat { display: grid; grid-template-rows: 1fr 1fr; gap: 14px; }
  .mini { position: relative; border-radius: var(--r-lg); overflow: hidden; background: #15131f; }
  .mini :global(canvas) { position: absolute; inset: 0; }
  .mini::after { content: ""; position: absolute; inset: 0; background: linear-gradient(0deg, rgba(6,6,14,0.75), rgba(6,6,14,0) 60%); }
  .mini:focus-visible { outline-offset: 2px; border-radius: var(--r-lg); }
  .mini .lbl { position: absolute; z-index: 2; left: 14px; right: 14px; bottom: 12px; color: #fff; display: flex; align-items: flex-end; justify-content: space-between; }
  .mini .top { position: absolute; z-index: 2; left: 12px; top: 12px; }
  .mini-title { font-weight: 650; font-size: 15px; }
  .mini-sub { opacity: 0.75; }

  .filters { display: flex; align-items: center; gap: 8px; margin: 18px 0 16px; }
  .chips { display: flex; gap: 8px; flex-wrap: wrap; }
  .dot.square { border-radius: 1px; }
  .nowrap { white-space: nowrap; }
  .sort { width: 150px; }
  .grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(196px, 1fr)); gap: 18px 16px; }
  .empty { padding: 48px 0; text-align: center; color: var(--ink-3); }
</style>
