<script lang="ts">
  import { onMount, type Component } from "svelte";
  import Titlebar from "../components/Titlebar.svelte";
  import CostMeter from "../components/CostMeter.svelte";
  import Toggle from "../components/Toggle.svelte";
  import Slider from "../components/Slider.svelte";
  import Seg from "../components/Seg.svelte";
  import Icon from "../lib/Icon.svelte";
  import Art from "../lib/art/Art.svelte";
  import type { IconName } from "../lib/icons";
  import { daemon } from "../lib/daemon";
  import type { Display, Property, SpanMode, WallpaperDetail } from "../lib/daemon/types";
  import { avatarGradient, displayList, formatBytes, formatCount, formatDate, formatPct, formatResolution, formatSliderValue } from "../lib/format";
  import { href, navigate, type Route } from "../lib/stores/router";
  import { refreshStatus } from "../lib/stores/status";
  import { attempt, toast } from "../lib/stores/toast";

  let { route }: { route: Route } = $props();
  const id = $derived(route.params[0] ?? "");

  let w = $state<WallpaperDetail | null>(null);
  let displays = $state<Display[]>([]);
  let selected = $state<number[]>([]);
  let previewOn = $state(0);
  let mode = $state<SpanMode>("each");
  let paletteId = $state("");
  let values = $state<Record<string, number | string | boolean>>({});
  let tab = $state<"customize" | "presets" | "details">("customize");
  let hideDesktop = $state(false);
  let liked = $state(false);
  let saved = $state(false);
  let following = $state(false);
  let now = $state(new Date());
  let previewEl = $state<HTMLDivElement>();
  let Consent = $state<Component<any> | null>(null);

  const allProps = $derived(w?.properties ?? []);
  const colors = $derived(allProps.filter((p) => p.type === "color"));
  const motion = $derived(allProps.filter((p) => p.type === "slider" || p.type === "combo"));
  const toggles = $derived(allProps.filter((p) => p.type === "bool"));
  const previewDisplay = $derived(displays.find((d) => d.id === previewOn));
  const kindLabel = $derived(w ? w.kind[0]!.toUpperCase() + w.kind.slice(1) : "");
  const estimate = $derived(w ? Math.max(1, Math.round(w.gpuPct * Math.max(1, selected.length) / 2)) : 0);
  const installId = $derived(route.query.install);

  onMount(() => {
    attempt(async () => {
      const d = await daemon();
      const [detail, ds] = await Promise.all([d.wallpaper(id), d.displays()]);
      w = detail;
      displays = ds;
      selected = ds.filter((x) => x.wallpaperId === id).map((x) => x.id);
      if (selected.length === 0) selected = ds.filter((x) => x.main).map((x) => x.id);
      previewOn = ds.find((x) => x.main)?.id ?? ds[0]?.id ?? 0;
      resetValues();
    }, "Couldn't load this wallpaper");
    const t = setInterval(() => (now = new Date()), 30_000);
    return () => clearInterval(t);
  });

  $effect(() => {
    if (installId && !Consent) import("../components/ConsentDialog.svelte").then((m) => { Consent = m.default; });
  });

  function resetValues() {
    if (!w) return;
    values = Object.fromEntries(w.properties.map((p) => [p.key, p.value]));
    paletteId = w.paletteId;
  }
  function resetMotion() {
    if (!w) return;
    for (const p of w.properties) if (p.type === "slider" || p.type === "combo") values[p.key] = p.value;
  }
  function toggleDisplay(n: number) {
    selected = selected.includes(n) ? selected.filter((x) => x !== n) : [...selected, n].sort((a, b) => a - b);
  }
  function pickPalette(pid: string) {
    paletteId = pid;
    const pal = w?.palettes.find((p) => p.id === pid);
    if (!pal) return;
    colors.forEach((c, i) => { if (pal.colors[i]) values[c.key] = pal.colors[i]!; });
  }
  function paletteKeys(e: KeyboardEvent) {
    if (!w) return;
    const dir = e.key === "ArrowRight" || e.key === "ArrowDown" ? 1 : e.key === "ArrowLeft" || e.key === "ArrowUp" ? -1 : 0;
    if (!dir) return;
    e.preventDefault();
    const i = w.palettes.findIndex((p) => p.id === paletteId);
    const next = w.palettes[(i + dir + w.palettes.length) % w.palettes.length];
    if (next) pickPalette(next.id);
    const group = e.currentTarget as HTMLElement;
    queueMicrotask(() => group.querySelector<HTMLElement>('[aria-checked="true"]')?.focus());
  }
  async function apply() {
    if (!w) return;
    if (!w.installed) { navigate(href("wallpaper", [w.id], { install: "1" })); return; }
    if (selected.length === 0) { toast("Pick at least one display first", "info"); return; }
    const res = await attempt(async () => (await daemon()).apply({ wallpaperId: w!.id, displays: selected, mode, values: { ...values }, paletteId }), "Couldn't apply");
    if (res) { await refreshStatus().catch(() => {}); toast(`${w.title} is on ${displayList(res.applied)}`); }
  }
  async function savePreset() {
    if (!w) return;
    const name = `My preset ${w.presets + 1}`;
    const n = await attempt(async () => (await daemon()).savePreset({ wallpaperId: w!.id, name, values: { ...values }, paletteId }), "Couldn't save the preset");
    if (n !== undefined) { w.presets = n; toast(`Saved "${name}"`); }
  }
  function closeConsent(installed: boolean) {
    navigate(href("wallpaper", [id]));
    if (installed && w) w.installed = true;
  }
  function fullscreen() {
    if (document.fullscreenElement) document.exitFullscreen().catch(() => {});
    else previewEl?.requestFullscreen?.().catch(() => {});
  }
  function back() {
    if (history.length > 1) history.back(); else navigate(href("discover"));
  }
  const clock = $derived(now.toLocaleTimeString("en-GB", { hour: "2-digit", minute: "2-digit" }));
  const day = $derived(now.toLocaleDateString("en-GB", { weekday: "short", day: "numeric", month: "short" }).replace(",", ""));
  const sliderText = (p: Property) => (p.type === "slider" ? formatSliderValue(Number(values[p.key] ?? p.value), p.unit) : "");
</script>

<Titlebar>
  <button type="button" class="icon-btn boxed back" aria-label="Back" onclick={back}><Icon name="chevleft" /></button>
  <nav class="crumbs t-small" aria-label="Breadcrumb">
    <a class="ink-3" href={href("discover")}>Discover</a><Icon name="chevright" class="crumb-ic" /><span class="cur" aria-current="page">{w?.title ?? ""}</span>
  </nav>
  <span class="spacer" data-tauri-drag-region></span>
  <a class="search compact" href={href("discover")} aria-label="Search the gallery"><Icon name="search" /><span class="grow">Search</span><span class="kbd">Ctrl K</span></a>
</Titlebar>

{#if w}
  <div class="split">
    <section class="left" aria-label="Preview and details">
      <div class="preview" bind:this={previewEl}>
        <Art spec={w.art} label="{w.title} on display {previewOn}" />
        {#if !hideDesktop}
          <div class="desk-icons" aria-hidden="true">
            <div><span><Icon name="folder" /></span>Projects</div>
            <div><span><Icon name="image" /></span>Photos</div>
            <div><span><Icon name="trash" /></span>Recycle</div>
          </div>
        {/if}
        <div class="pv-top">
          <label class="glass-sel">
            <Icon name="monitor" />
            <span class="sr-only">Preview on</span>
            <select bind:value={previewOn}>
              {#each displays as d (d.id)}<option value={d.id}>Preview on Display {d.id} · {formatResolution(d.width, d.height)}</option>{/each}
            </select>
            <Icon name="chevdown" />
          </label>
          <button type="button" class="glass-sel" aria-pressed={hideDesktop} onclick={() => (hideDesktop = !hideDesktop)}><Icon name="eye" />{hideDesktop ? "Show desktop" : "Hide desktop"}</button>
          <button type="button" class="glass-sel sq" aria-label="Fullscreen preview" onclick={fullscreen}><Icon name="maximize" /></button>
        </div>
        <div class="pv-bl"><span class="badge badge-glass live"><span class="dot"></span>Live · <span class="mono">{previewDisplay && previewDisplay.width < previewDisplay.height ? "30" : "60"} fps</span> · <span class="mono">{formatPct(w.gpuPct + 0.1)}</span> GPU</span></div>
        {#if !hideDesktop}
          <div class="taskbar" aria-hidden="true"><i></i><i></i><i class="hl"></i><i></i><i></i><div class="clock">{clock}<br />{day}</div></div>
        {/if}
      </div>

      <div class="row gap-12 title-row">
        <div class="grow">
          <div class="row gap-10 wrap">
            <h1 class="t-h1">{w.title}</h1>
            {#if w.signed}<span class="badge badge-ok"><Icon name="badge" />Signed · verified</span>{:else}<span class="badge badge-warn"><Icon name="alert" />Unsigned</span>{/if}
            <span class="badge badge-neutral">{kindLabel}</span><span class="badge badge-neutral mono">v{w.version}</span>
          </div>
          <p class="t-body ink-2 desc">{w.description}</p>
        </div>
        <button type="button" class="icon-btn boxed" class:on={liked} aria-label="Like" aria-pressed={liked} onclick={() => (liked = !liked)}><Icon name="heart" /></button>
        <button type="button" class="icon-btn boxed" class:on={saved} aria-label="Save for later" aria-pressed={saved} onclick={() => (saved = !saved)}><Icon name="bookmark" /></button>
        <button type="button" class="icon-btn boxed" aria-label="Report this wallpaper" title="Report" onclick={() => attempt(async () => (await daemon()).openLink("report", w!.id))}><Icon name="flag" /></button>
      </div>

      <div class="info-grid">
        <div class="card card-pad creator">
          <div class="row gap-12">
            <span class="avatar big-av" style:background={avatarGradient(w.creator.handle)}>{w.creator.initials}</span>
            <div class="grow">
              <div class="row gap-6"><span class="t-h3">{w.creator.handle}</span>{#if w.creator.verified}<span class="verified" title="Verified creator"><Icon name="badge" /><span class="sr-only">Verified</span></span>{/if}</div>
              <div class="t-small ink-3">{w.creator.wallpapers} wallpapers · {formatCount(w.creator.installs)} installs · creator since {w.creator.since}</div>
            </div>
            <button type="button" class="btn btn-secondary btn-sm" aria-pressed={following} onclick={() => (following = !following)}><Icon name={following ? "check" : "plus"} />{following ? "Following" : "Follow"}</button>
          </div>
          <div class="row gap-8">
            <button type="button" class="btn tip grow" onclick={() => attempt(async () => (await daemon()).openLink("tip", w!.creator.handle))}><Icon name="coffee" />Tip {w.creator.handle}<Icon name="external" class="ext" /></button>
            <button type="button" class="btn btn-secondary" onclick={() => attempt(async () => (await daemon()).openLink("sponsor", w!.creator.handle))}><Icon name="heart" />Sponsor</button>
          </div>
          <div class="t-micro ink-3">Tips go straight to the creator. Wallweaver takes nothing.</div>
        </div>
        <div class="card card-pad">
          <dl class="kv">
            <dt>License</dt><dd><span class="mono">{w.license}</span>{#if w.remixAllowed}<span class="ink-3">· remix allowed</span>{/if}</dd>
            <dt>Uses</dt><dd>{#if w.uses.length}<Icon name={w.uses[0]!} class="accent-ic" />{/if}{w.usesLabel}</dd>
            <dt>Cost</dt><dd><CostMeter cost={w.cost} variant="panel" /><span class="ink-3">~{formatPct(w.gpuPct, 0)} GPU · ~{w.watts} W</span></dd>
            <dt>Size</dt><dd class="mono">{formatBytes(w.sizeBytes)}</dd>
            <dt>Updated</dt><dd>{formatDate(w.updated)}</dd>
          </dl>
        </div>
      </div>

      <div>
        <div class="row more-h"><h2 class="t-h3">More from {w.creator.handle}</h2><span class="spacer"></span><span class="t-small ink-3">See all {w.creator.wallpapers}</span></div>
        <div class="more">
          {#each w.moreFromCreator as m (m.id)}
            <a class="thumb" href={href("wallpaper", [m.id])} aria-label={m.title}><Art spec={m.art} /></a>
          {/each}
        </div>
      </div>
    </section>

    <aside class="inspector" aria-label="Customize">
      <div class="tabs-wrap">
        <div class="tabs" role="tablist" aria-label="Inspector">
          <button type="button" role="tab" class:on={tab === "customize"} aria-selected={tab === "customize"} onclick={() => (tab = "customize")}>Customize</button>
          <button type="button" role="tab" class:on={tab === "presets"} aria-selected={tab === "presets"} onclick={() => (tab = "presets")}>Presets <span class="ink-3">{w.presets}</span></button>
          <button type="button" role="tab" class:on={tab === "details"} aria-selected={tab === "details"} onclick={() => (tab = "details")}>Details</button>
        </div>
      </div>
      <div class="insp-body" role="tabpanel">
        {#if tab === "customize"}
          <div class="insp-sec">
            <div class="sec-h"><span class="t-eyebrow" id="displays-h">Displays</span><span class="t-micro ink-3">Click a screen to select</span></div>
            <div class="monitors" role="group" aria-labelledby="displays-h">
              {#each displays as d (d.id)}
                <button type="button" class="mon" class:sel={selected.includes(d.id)} aria-pressed={selected.includes(d.id)}
                  aria-label="Display {d.id}, {formatResolution(d.width, d.height)}{d.main ? ', main' : ''}"
                  style:left="{d.layout.x}px" style:top="{d.layout.y}px" style:width="{d.layout.w}px" style:height="{d.layout.h}px"
                  onclick={() => toggleDisplay(d.id)}>
                  <Art spec={selected.includes(d.id) ? w.art : d.art} />
                  <span class="num">{d.id}</span>
                  <span class="res">{d.width < d.height ? "Portrait" : formatResolution(d.width, d.height)}{d.main ? " · main" : ""}</span>
                </button>
              {/each}
            </div>
            <Seg wide label="How to use multiple displays" bind:value={mode} options={[{ value: "each", label: "Each display" }, { value: "span", label: "Span across" }, { value: "same", label: "Same on all" }]} />
          </div>

          {#if colors.length}
            <div class="insp-sec">
              <div class="sec-h"><span class="t-eyebrow" id="colors-h">Colors</span><button type="button" class="t-micro linkish" onclick={() => pickPalette(w!.palettes[0]?.id ?? "")}>Match theme</button></div>
              <!-- svelte-ignore a11y_interactive_supports_focus -->
              <div class="row gap-10" role="radiogroup" aria-labelledby="colors-h" onkeydown={paletteKeys}>
                {#each w.palettes as p (p.id)}
                  <button type="button" role="radio" class="pal" class:sel={paletteId === p.id} aria-checked={paletteId === p.id} aria-label="{p.id} palette" tabindex={paletteId === p.id ? 0 : -1} style:background={p.css} onclick={() => pickPalette(p.id)}></button>
                {/each}
              </div>
              <div class="row gap-8">
                {#each colors as c (c.key)}
                  <label class="color-row grow">
                    <span class="sw" style:background={String(values[c.key])}></span>
                    <span class="grow">{c.label}</span>
                    <span class="mono ink-3 hex">{String(values[c.key]).toUpperCase()}</span>
                    <input class="sr-only" type="color" value={String(values[c.key]).toLowerCase()} oninput={(e) => (values[c.key] = e.currentTarget.value.toUpperCase())} />
                  </label>
                {/each}
              </div>
            </div>
          {/if}

          {#if motion.length}
            <div class="insp-sec">
              <div class="sec-h"><span class="t-eyebrow">Motion</span><button type="button" class="t-micro ink-3 linkish plain" onclick={resetMotion}>Reset</button></div>
              {#each motion as p (p.key)}
                {#if p.type === "slider"}
                  <div class="field">
                    <div class="field-head"><span>{p.label}</span><span class="val">{sliderText(p)}</span></div>
                    <Slider label={p.label} min={p.min} max={p.max} step={p.step} valueText={sliderText(p)}
                      value={Number(values[p.key])} onchange={(v) => (values[p.key] = v)} />
                  </div>
                {:else if p.type === "combo"}
                  <div class="row gap-10">
                    <label class="t-small ink-2 combo-l" for="prop-{p.key}">{p.label}</label>
                    <select class="select grow" id="prop-{p.key}" bind:value={values[p.key]}>
                      {#each p.options as o (o.value)}<option value={o.value}>{o.label}</option>{/each}
                    </select>
                  </div>
                {/if}
              {/each}
            </div>
          {/if}

          {#if toggles.length}
            <div class="insp-sec toggles">
              {#each toggles as p (p.key)}
                {#if p.type === "bool"}
                  <div class="trow">
                    <Icon name={p.icon as IconName} class="trow-ic" />
                    <span class="grow">{p.label}</span>
                    {#if p.needs}<span class="badge badge-accent needs">{p.needs}</span>{/if}
                    <Toggle label={p.label} checked={Boolean(values[p.key])} onchange={(v) => (values[p.key] = v)} />
                  </div>
                {/if}
              {/each}
            </div>
          {/if}
        {:else if tab === "presets"}
          <div class="insp-sec">
            <button type="button" class="preset-row" onclick={() => { resetValues(); toast("Back to the creator's settings", "info"); }}>
              <span class="t-small strong">Creator default</span><span class="t-micro ink-3">As {w.creator.handle} shipped it</span>
            </button>
            <p class="t-small ink-3">{w.presets} saved {w.presets === 1 ? "preset" : "presets"} for this wallpaper. Save the current look with "Save preset" below.</p>
          </div>
        {:else}
          <div class="insp-sec">
            <dl class="kv">
              <dt>Type</dt><dd>{kindLabel}</dd>
              <dt>Version</dt><dd class="mono">{w.version}</dd>
              <dt>Creator</dt><dd>{w.creator.handle}</dd>
              <dt>License</dt><dd class="mono">{w.license}</dd>
              <dt>Permissions</dt><dd>{w.usesLabel}</dd>
              <dt>Signature</dt><dd>{w.signed ? "ed25519, verified" : "Unsigned"}</dd>
            </dl>
          </div>
        {/if}
      </div>
      <div class="insp-foot">
        <div class="row gap-8 t-small ink-2"><CostMeter cost={w.cost} variant="status" />Estimated <span class="mono tnum strong">~{estimate}% GPU</span> on {selected.length} {selected.length === 1 ? "display" : "displays"}</div>
        <div class="row gap-8">
          <button type="button" class="btn btn-secondary" onclick={savePreset}>Save preset</button>
          <button type="button" class="btn btn-primary grow btn-lg" onclick={apply}>
            <Icon name={w.installed ? "check" : "download"} />{w.installed ? `Apply to ${displayList(selected)}` : "Install and apply"}
          </button>
        </div>
      </div>
    </aside>
  </div>
{/if}

{#if installId && Consent && w}
  <Consent id={w.id} onclose={closeConsent} />
{/if}

<style>
  .back { width: 30px; height: 30px; }
  .crumbs { display: flex; align-items: center; gap: 12px; }
  .crumbs a:hover { color: var(--ink); }
  .crumbs :global(.crumb-ic) { width: 14px; height: 14px; color: var(--ink-3); }
  .cur { font-weight: 600; }
  .spacer { align-self: stretch; }
  .search.compact { width: 280px; }
  .split { flex: 1; display: grid; grid-template-columns: 1fr 392px; min-height: 0; }
  .left { padding: 20px 24px 24px; display: flex; flex-direction: column; gap: 16px; min-width: 0; overflow: auto; }
  .preview { position: relative; aspect-ratio: 16/9; border-radius: var(--r-xl); overflow: hidden; box-shadow: var(--shadow-1), 0 0 0 1px var(--line); flex: none; background: #15131f; }
  .preview :global(canvas) { position: absolute; inset: 0; }
  .desk-icons { position: absolute; left: 16px; top: 16px; display: grid; gap: 14px; z-index: 2; }
  .desk-icons div { width: 54px; display: flex; flex-direction: column; align-items: center; gap: 4px; font-size: 9.5px; color: #fff; text-shadow: 0 1px 2px rgba(0,0,0,.6); }
  .desk-icons span { width: 30px; height: 30px; border-radius: 7px; background: rgba(255,255,255,0.16); box-shadow: inset 0 0 0 1px rgba(255,255,255,0.2); display: grid; place-items: center; }
  .desk-icons :global(svg) { width: 15px; height: 15px; }
  .taskbar { position: absolute; left: 0; right: 0; bottom: 0; height: 34px; z-index: 2; background: rgba(14,14,24,0.55); backdrop-filter: blur(20px); border-top: 1px solid rgba(255,255,255,0.08); display: flex; align-items: center; justify-content: center; gap: 8px; color: #fff; }
  .taskbar i { width: 22px; height: 22px; border-radius: 5px; background: rgba(255,255,255,0.14); display: block; }
  .taskbar i.hl { background: linear-gradient(135deg,#ffb7a8,#8fe3d3); }
  .taskbar .clock { position: absolute; right: 14px; font-size: 10px; text-align: right; line-height: 1.2; opacity: .9; }
  .pv-top { position: absolute; top: 14px; right: 14px; z-index: 3; display: flex; gap: 8px; }
  .pv-bl { position: absolute; left: 14px; bottom: 46px; z-index: 3; }
  .live { height: 26px; padding: 0 10px; }
  .live .dot { width: 6px; height: 6px; border-radius: 50%; background: #6fe3b4; }
  .glass-sel { display: inline-flex; align-items: center; gap: 8px; height: 30px; padding: 0 10px; border-radius: 8px; background: rgba(12,10,20,0.55); color: #fff; backdrop-filter: blur(14px); box-shadow: inset 0 0 0 1px rgba(255,255,255,0.14); font-size: 12px; position: relative; }
  .glass-sel:hover { background: rgba(12,10,20,0.7); }
  .glass-sel :global(svg) { width: 14px; height: 14px; opacity: .85; }
  .glass-sel select { appearance: none; border: 0; background: none; color: #fff; font-size: 12px; cursor: pointer; padding-right: 22px; margin-right: -22px; }
  .glass-sel select option { color: var(--ink); background: var(--bg-elev); }
  .glass-sel:has(select:focus-visible) { outline: 2px solid var(--accent); outline-offset: 2px; }
  .glass-sel.sq { width: 30px; padding: 0; justify-content: center; }
  .title-row { align-items: flex-start; }
  .wrap { flex-wrap: wrap; }
  .desc { margin-top: 6px; max-width: 620px; }
  .icon-btn.on { color: var(--accent); }
  .info-grid { display: grid; grid-template-columns: 1.1fr 1fr; gap: 14px; align-items: start; }
  .creator { display: flex; flex-direction: column; gap: 14px; }
  .big-av { width: 42px; height: 42px; font-size: 14px; }
  .verified { color: var(--accent); display: inline-flex; }
  .verified :global(svg) { width: 14px; height: 14px; }
  .tip { background: linear-gradient(135deg, rgba(255,183,168,0.16), rgba(198,168,255,0.14)); box-shadow: inset 0 0 0 1px rgba(255,183,168,0.35); color: var(--ink); }
  .tip:hover { background: linear-gradient(135deg, rgba(255,183,168,0.24), rgba(198,168,255,0.2)); }
  .tip :global(.ext) { width: 13px; height: 13px; opacity: .6; }
  .kv { display: grid; grid-template-columns: 96px 1fr; row-gap: 9px; font-size: 12.5px; }
  .kv dt { color: var(--ink-3); }
  .kv dd { color: var(--ink); display: flex; align-items: center; gap: 6px; }
  .kv :global(.accent-ic) { width: 14px; height: 14px; color: var(--accent); }
  .more-h { margin-bottom: 10px; }
  .more { display: grid; grid-template-columns: repeat(5, 1fr); gap: 12px; }
  .more .thumb { aspect-ratio: 16/9; box-shadow: 0 0 0 1px var(--line); }
  .more .thumb:hover { box-shadow: 0 0 0 2px var(--accent); }

  .inspector { border-left: 1px solid var(--line); background: linear-gradient(180deg, rgba(255,255,255,0.02), rgba(255,255,255,0.008)); display: flex; flex-direction: column; min-height: 0; }
  :global(:root[data-theme="light"]) .inspector { background: rgba(255,255,255,0.6); }
  .tabs-wrap { padding: 0 20px; }
  .insp-body { flex: 1; overflow: auto; padding: 0 20px; }
  .insp-sec { padding: 14px 0; border-bottom: 1px solid var(--line); display: flex; flex-direction: column; gap: 14px; }
  .insp-sec:last-child { border-bottom: 0; }
  .insp-sec.toggles { gap: 12px; }
  .sec-h { display: flex; align-items: center; justify-content: space-between; }
  .linkish { color: var(--accent); padding: 0; }
  .linkish.plain { color: var(--ink-3); }
  .linkish:hover { text-decoration: underline; }
  .monitors { position: relative; height: 104px; border-radius: var(--r-md); background: var(--panel); box-shadow: inset 0 0 0 1px var(--line); }
  .mon { position: absolute; border-radius: 5px; overflow: hidden; box-shadow: 0 0 0 1px var(--line-2); padding: 0; background: #15131f; }
  .mon :global(canvas) { position: absolute; inset: 0; }
  .mon.sel { box-shadow: 0 0 0 2px var(--accent), 0 6px 18px rgba(0,0,0,0.4); }
  .mon:focus-visible { outline-offset: 3px; }
  .mon .num { position: absolute; left: 5px; top: 5px; z-index: 2; width: 17px; height: 17px; border-radius: 5px; font-size: 10px; font-weight: 700; display: grid; place-items: center; background: rgba(10,8,18,0.7); color: #fff; }
  .mon.sel .num { background: var(--accent); color: #120f22; }
  .mon .res { position: absolute; left: 0; right: 0; bottom: 0; z-index: 2; font-size: 9px; color: rgba(255,255,255,0.85); padding: 3px 5px; background: linear-gradient(0deg, rgba(0,0,0,0.6), transparent); font-family: var(--mono); text-align: left; white-space: nowrap; overflow: hidden; }
  .pal { width: 40px; height: 26px; border-radius: 8px; box-shadow: inset 0 0 0 1px rgba(255,255,255,0.12); }
  .pal.sel { box-shadow: 0 0 0 2px var(--bg), 0 0 0 4px var(--accent); }
  .color-row { display: flex; align-items: center; gap: 10px; height: 34px; padding: 0 10px 0 6px; border-radius: var(--r-sm); background: var(--panel-2); box-shadow: inset 0 0 0 1px var(--line); font-size: 12.5px; cursor: pointer; position: relative; }
  .color-row:hover { background: var(--panel-3); }
  .color-row:has(input:focus-visible) { outline: 2px solid var(--accent); outline-offset: 2px; }
  .color-row .sw { width: 22px; height: 22px; border-radius: 6px; box-shadow: inset 0 0 0 1px rgba(255,255,255,0.2); }
  .hex { font-size: 11.5px; }
  .combo-l { width: 96px; }
  .trow { display: flex; align-items: center; gap: 10px; font-size: 12.5px; color: var(--ink-2); }
  .trow .grow { color: var(--ink); }
  .trow :global(.trow-ic) { width: 16px; height: 16px; color: var(--ink-3); }
  .needs { height: 20px; }
  .preset-row { display: flex; flex-direction: column; align-items: flex-start; gap: 2px; padding: 10px 12px; border-radius: 10px; background: var(--panel); box-shadow: inset 0 0 0 1px var(--line); text-align: left; }
  .preset-row:hover { background: var(--panel-2); }
  .strong { color: var(--ink); font-weight: 600; }
  .insp-foot { border-top: 1px solid var(--line); padding: 14px 20px 18px; display: flex; flex-direction: column; gap: 12px; background: var(--bg-elev); }
  :global(:root:not([data-theme="light"])) .insp-foot { background: rgba(14,13,22,0.9); }
</style>
