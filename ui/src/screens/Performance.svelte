<script lang="ts">
  import { onMount } from "svelte";
  import Titlebar from "../components/Titlebar.svelte";
  import Toggle from "../components/Toggle.svelte";
  import Slider from "../components/Slider.svelte";
  import Seg from "../components/Seg.svelte";
  import Icon from "../lib/Icon.svelte";
  import Art from "../lib/art/Art.svelte";
  import type { IconName } from "../lib/icons";
  import { daemon } from "../lib/daemon";
  import type { PerfSettings, PerfSnapshot, PresetId } from "../lib/daemon/types";
  import { FPS_STOPS, fpsLabel, formatPct, pluralize, sparkPath } from "../lib/format";
  import { setPausedAll, status } from "../lib/stores/status";
  import { attempt, toast } from "../lib/stores/toast";

  let perf = $state<PerfSnapshot | null>(null);
  let addingApp = $state(false);
  let newExe = $state("");
  let newAction = $state("Stop");

  const live = $derived(perf ? perf.displays.filter((d) => d.state === "running").length : 0);
  const fpsIndex = $derived(perf ? Math.max(0, FPS_STOPS.indexOf(perf.settings.fpsCap as (typeof FPS_STOPS)[number])) : 0);

  onMount(() => {
    const load = () => attempt(async () => { perf = await (await daemon()).performance(); }, "Couldn't read performance data");
    load();
    const t = setInterval(load, 1000);
    return () => clearInterval(t);
  });

  async function setPreset(id: PresetId) {
    const p = await attempt(async () => (await daemon()).setPreset(id), "Couldn't change the preset");
    if (p) perf = p;
  }
  function presetKeys(e: KeyboardEvent) {
    if (!perf) return;
    const dir = e.key === "ArrowDown" || e.key === "ArrowRight" ? 1 : e.key === "ArrowUp" || e.key === "ArrowLeft" ? -1 : 0;
    if (!dir) return;
    e.preventDefault();
    const i = perf.presets.findIndex((p) => p.id === perf!.preset);
    const next = perf.presets[(i + dir + perf.presets.length) % perf.presets.length];
    if (!next) return;
    const group = e.currentTarget as HTMLElement;
    setPreset(next.id).then(() => group.querySelector<HTMLElement>('[aria-checked="true"]')?.focus());
  }
  async function setRule(id: string, action: string) {
    const p = await attempt(async () => (await daemon()).setPauseRule(id, action), "Couldn't change the rule");
    if (p) perf = p;
  }
  async function saveSettings(patch: Partial<PerfSettings>) {
    if (!perf) return;
    const p = await attempt(async () => (await daemon()).setPerfSettings({ ...perf!.settings, ...patch }), "Couldn't save");
    if (p) perf = p;
  }
  async function addApp(e: SubmitEvent) {
    e.preventDefault();
    const exe = newExe.trim();
    if (!exe) return;
    const p = await attempt(async () => (await daemon()).setAppRule(exe, newAction), "Couldn't add the app");
    if (p) { perf = p; newExe = ""; addingApp = false; }
  }
  async function identify() {
    await attempt(async () => (await daemon()).identifyDisplays());
    toast("Showing a number on each display", "info");
  }
</script>

<Titlebar>
  <h1 class="t-h3 page-title">Performance &amp; power</h1>
  <span class="spacer" data-tauri-drag-region></span>
  {#if perf}
    <span class="badge {$status?.paused ? 'badge-warn' : 'badge-ok'} big-badge"><Icon name={$status?.paused ? "pause" : "leaf"} />{$status?.paused ? "All paused" : `Running light · ${live} of ${perf.displays.length} displays live`}</span>
  {/if}
  <button type="button" class="btn btn-secondary btn-sm" aria-pressed={!!$status?.paused} onclick={() => attempt(() => setPausedAll(!$status?.paused))}>
    <Icon name={$status?.paused ? "play" : "pause"} />{$status?.paused ? "Resume all" : "Pause all"} <span class="kbd">Ctrl Alt P</span>
  </button>
</Titlebar>

{#if perf}
  <section class="content">
    <div class="meters">
      {#each [
        { label: "CPU", icon: "cpu", num: perf.cpu.value.toFixed(1), unit: "%", ctx: `Wallweaver only · system ${perf.cpu.systemPct}%`, m: perf.cpu },
        { label: "GPU", icon: "layers", num: perf.gpu.value.toFixed(1), unit: "%", ctx: `${perf.gpu.adapter} · ${pluralize(perf.gpu.shaders, "shader")}`, m: perf.gpu },
        { label: "Memory", icon: "package", num: String(Math.round(perf.memory.value)), unit: "MB", ctx: `Budget ${perf.memory.budgetMb} MB · ${pluralize(perf.memory.webviews, "webview")}`, m: perf.memory },
        { label: "Power", icon: "battery", num: perf.power.value.toFixed(1), unit: "W", ctx: `${perf.power.pluggedIn ? "Plugged in" : "On battery"} · battery ${perf.power.batteryPct}%`, m: perf.power },
      ] as meter (meter.label)}
        <div class="card meter">
          <div class="lbl"><Icon name={meter.icon as IconName} />{meter.label}</div>
          <div class="num tnum" aria-live="off">{meter.num}<small>{meter.unit}</small></div>
          <div class="ctx">{meter.ctx}</div>
          <svg class="spark" viewBox="0 0 128 44" preserveAspectRatio="none" role="img" aria-label="{meter.label} over the last minute">
            <path d={sparkPath(meter.m.history)} fill="none" stroke="var(--accent)" vector-effect="non-scaling-stroke" stroke-width="2" stroke-linejoin="round" stroke-linecap="round" />
            <path d="{sparkPath(meter.m.history)} V44 H0Z" fill="var(--accent)" opacity=".10" />
          </svg>
        </div>
      {/each}
    </div>

    <div class="cols">
      <div class="col">
        <div class="card">
          <div class="card-h"><h2 class="t-h3">Displays</h2><span class="t-small ink-3">last 60 s</span><span class="spacer"></span><button type="button" class="t-small linkish" onclick={identify}>Identify displays</button></div>
          {#each perf.displays as d (d.displayId)}
            <div class="disp">
              <div class="thumb" class:paused={d.state !== "running"}><Art spec={d.art} /></div>
              <div>
                <div class="t-small strong">Display {d.displayId} · {d.wallpaperTitle}</div>
                <div class="t-micro ink-3">{d.detail}{#if d.reason}{" "}<span class="ink-2">{d.reason}</span>{/if}</div>
              </div>
              {#if d.state === "running"}<span class="badge badge-ok"><Icon name="play" />Running</span>
              {:else if d.state === "paused"}<span class="badge badge-warn"><Icon name="pause" />Paused</span>
              {:else}<span class="badge badge-neutral"><Icon name="square" />Stopped</span>{/if}
              <span class="fps">{d.fps} fps<br /><span class="ink-3">{formatPct(d.gpuPct)} GPU</span></span>
            </div>
          {/each}
        </div>

        <div class="card rules">
          <div class="card-h"><h2 class="t-h3">Pause rules</h2><span class="t-small ink-3">Checked every {perf.checkIntervalMs} ms, resumes within 1 s</span></div>
          {#each perf.rules as r (r.id)}
            <div class="rule">
              <span class="ic"><Icon name={r.icon as IconName} /></span>
              <div><div class="t" id="rule-{r.id}">{r.title}</div><div class="s">{r.detail}</div></div>
              <select class="select" aria-labelledby="rule-{r.id}" value={r.action} onchange={(e) => setRule(r.id, e.currentTarget.value)}>
                {#each r.options as o (o)}<option value={o}>{o}</option>{/each}
              </select>
            </div>
          {/each}
          <div class="apprules">
            <span class="t-small ink-3 ar-l">App rules</span>
            {#each perf.appRules as a (a.exe)}
              <span class="apprule"><Icon name={a.icon as IconName} /><span class="mono">{a.exe}</span><span class="ink-3" aria-hidden="true">→</span><span class="sr-only">then</span>{a.action}</span>
            {/each}
            {#if addingApp}
              <form class="row gap-6" onsubmit={addApp}>
                <input class="input mono" placeholder="app.exe" aria-label="Program name" bind:value={newExe} />
                <select class="select small-sel" aria-label="Action" bind:value={newAction}>
                  <option>Stop</option><option>Pause</option><option>Keep running</option>
                </select>
                <button class="btn btn-secondary btn-sm" type="submit">Add</button>
                <button class="btn btn-ghost btn-sm" type="button" onclick={() => (addingApp = false)}>Cancel</button>
              </form>
            {:else}
              <button type="button" class="btn btn-ghost btn-sm" onclick={() => (addingApp = true)}><Icon name="plus" />Add app</button>
            {/if}
          </div>
        </div>
      </div>

      <div class="col">
        <div class="card presets-card">
          <div class="card-h"><h2 class="t-h3" id="preset-h">Power preset</h2><span class="spacer"></span><span class="t-small ink-3">Custom changes are kept per preset</span></div>
          <!-- svelte-ignore a11y_interactive_supports_focus -->
          <div class="presets" role="radiogroup" aria-labelledby="preset-h" onkeydown={presetKeys}>
            {#each perf.presets as p (p.id)}
              <button type="button" role="radio" class="preset" class:on={perf.preset === p.id} aria-checked={perf.preset === p.id} tabindex={perf.preset === p.id ? 0 : -1} onclick={() => setPreset(p.id)}>
                <span class="ic"><Icon name={p.icon as IconName} /></span>
                <div><div class="t-small strong">{p.title}{#if p.recommended}<span class="badge badge-accent rec">Recommended</span>{/if}</div><div class="t-micro ink-3">{p.detail}</div></div>
                <span class="radio" aria-hidden="true"></span>
              </button>
            {/each}
          </div>
        </div>

        <div class="card card-pad tuning">
          <div class="field">
            <div class="field-head"><span class="t-h3 strong">Frame rate cap</span><span class="val">{fpsLabel(perf.settings.fpsCap)}</span></div>
            <Slider label="Frame rate cap" min={0} max={FPS_STOPS.length - 1} step={1} value={fpsIndex} valueText={fpsLabel(perf.settings.fpsCap)}
              ticks={FPS_STOPS.map((f) => (f === 0 ? "Max" : String(f)))} activeTick={fpsIndex}
              onchange={(i) => saveSettings({ fpsCap: FPS_STOPS[i] ?? 60 })} />
          </div>
          <div class="row gap-10"><span class="t-small ink-2 grow" id="scale-l">Render scale</span>
            <Seg label="Render scale" value={perf.settings.renderScale} options={[{ value: 50, label: "50%" }, { value: 75, label: "75%" }, { value: 100, label: "100%" }]} onchange={(v) => saveSettings({ renderScale: v })} />
          </div>
          <div class="row gap-10"><span class="t-small ink-2 grow">Hardware video decoding</span>
            <span class="badge {perf.settings.hwDecodeAvailable ? 'badge-ok' : 'badge-neutral'} small-badge">{perf.settings.hwDecodeAvailable ? "Available" : "Not available"}</span>
            <Toggle label="Hardware video decoding" checked={perf.settings.hwDecode} onchange={(v) => saveSettings({ hwDecode: v })} />
          </div>
          <div class="row gap-10"><label class="t-small ink-2 grow" for="away">Pause when I'm away for</label>
            <select id="away" class="select away" value={perf.settings.awayMinutes} onchange={(e) => saveSettings({ awayMinutes: Number(e.currentTarget.value) })}>
              {#each [0, 2, 5, 10, 30] as m (m)}<option value={m}>{m === 0 ? "Never" : `${m} minutes`}</option>{/each}
            </select>
          </div>
          <div class="row gap-10"><span class="t-small ink-2 grow">Show cost badges in Discover</span>
            <Toggle label="Show cost badges in Discover" checked={perf.settings.showCostBadges} onchange={(v) => saveSettings({ showCostBadges: v })} />
          </div>
        </div>
      </div>
    </div>
  </section>
{/if}

<style>
  .page-title { font-size: 15px; }
  .spacer { align-self: stretch; }
  .big-badge { height: 28px; padding: 0 12px; font-size: 12px; }
  .content { display: flex; flex-direction: column; gap: 16px; padding-top: 22px; }
  .meters { display: grid; grid-template-columns: repeat(4, 1fr); gap: 14px; }
  .meter { padding: 14px 16px 12px; position: relative; overflow: hidden; }
  .meter .lbl { display: flex; align-items: center; gap: 7px; font-size: 12px; color: var(--ink-2); font-weight: 500; }
  .meter .lbl :global(svg) { width: 14px; height: 14px; color: var(--ink-3); }
  .meter .num { font-size: 26px; font-weight: 650; letter-spacing: -0.02em; margin-top: 6px; }
  .meter .num small { font-size: 13px; font-weight: 500; color: var(--ink-3); margin-left: 2px; letter-spacing: 0; }
  .meter .ctx { font-size: 11.5px; color: var(--ink-3); }
  .meter svg.spark { display: block; width: 100%; height: 34px; margin-top: 10px; overflow: visible; }
  .cols { display: grid; grid-template-columns: 1.38fr 1fr; gap: 16px; flex: 1; min-height: 0; }
  .col { display: flex; flex-direction: column; gap: 16px; min-height: 0; }
  .card-h { display: flex; align-items: center; gap: 10px; padding: 14px 16px 10px; }
  .linkish { color: var(--accent); padding: 0; }
  .linkish:hover { text-decoration: underline; }
  .strong { font-weight: 600; color: var(--ink); }
  .disp { display: grid; grid-template-columns: 64px 1fr auto 76px; gap: 14px; align-items: center; padding: 10px 16px; border-top: 1px solid var(--line); }
  .disp .thumb { width: 64px; height: 38px; border-radius: 7px; }
  .disp .thumb.paused::after { content: ""; position: absolute; inset: 0; background: rgba(8,8,14,0.55); }
  .disp .fps { text-align: right; font-family: var(--mono); font-size: 12px; color: var(--ink-2); }
  .rules { flex: 1; }
  .rule { display: grid; grid-template-columns: 30px 1fr 188px; gap: 12px; align-items: center; padding: 9px 16px; border-top: 1px solid var(--line); }
  .rule .ic { width: 30px; height: 30px; border-radius: 8px; display: grid; place-items: center; background: var(--panel-2); color: var(--ink-2); }
  .rule .ic :global(svg) { width: 15px; height: 15px; }
  .rule .t { font-size: 13px; font-weight: 500; }
  .rule .s { font-size: 11.5px; color: var(--ink-3); }
  .apprules { display: flex; flex-wrap: wrap; gap: 6px; padding: 10px 16px 14px; border-top: 1px solid var(--line); align-items: center; }
  .ar-l { margin-right: 4px; }
  .apprule { display: inline-flex; align-items: center; gap: 6px; height: 26px; padding: 0 10px; border-radius: 8px; background: var(--panel-2); font-size: 12px; }
  .apprule .mono { font-size: 11px; }
  .apprule :global(svg) { width: 13px; height: 13px; color: var(--ink-3); }
  .input { height: 28px; padding: 0 10px; border-radius: 8px; border: 0; background: var(--panel-2); box-shadow: inset 0 0 0 1px var(--line); font-size: 12px; width: 140px; }
  .small-sel { height: 28px; }
  .presets-card { padding-bottom: 14px; }
  .presets { display: flex; flex-direction: column; gap: 8px; padding: 0 14px; }
  .preset { display: grid; grid-template-columns: 34px 1fr auto; gap: 12px; align-items: center; padding: 11px 12px; border-radius: 12px; box-shadow: inset 0 0 0 1px var(--line); background: var(--panel); text-align: left; }
  .preset:hover:not(.on) { background: var(--panel-2); }
  .preset .ic { width: 34px; height: 34px; border-radius: 10px; display: grid; place-items: center; background: var(--panel-2); color: var(--ink-2); }
  .preset .ic :global(svg) { width: 17px; height: 17px; }
  .preset.on { box-shadow: inset 0 0 0 1.5px var(--accent); background: var(--accent-soft); }
  .preset.on .ic { background: var(--accent); color: #120f22; }
  .rec { height: 18px; margin-left: 8px; }
  .radio { width: 18px; height: 18px; border-radius: 50%; box-shadow: inset 0 0 0 1.5px var(--line-2); }
  .preset.on .radio { box-shadow: inset 0 0 0 5px var(--accent); }
  .tuning { flex: 1; display: flex; flex-direction: column; gap: 18px; }
  .small-badge { height: 20px; }
  .away { width: 130px; }
</style>
