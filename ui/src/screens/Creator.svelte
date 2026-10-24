<script lang="ts">
  import { onMount } from "svelte";
  import Titlebar from "../components/Titlebar.svelte";
  import Toggle from "../components/Toggle.svelte";
  import Seg from "../components/Seg.svelte";
  import Icon from "../lib/Icon.svelte";
  import Art from "../lib/art/Art.svelte";
  import type { IconName } from "../lib/icons";
  import { daemon } from "../lib/daemon";
  import type { CreatorAction, CreatorProject, PackResult, Property, StepState } from "../lib/daemon/types";
  import { formatBytes, percentOf } from "../lib/format";
  import { attempt, toast } from "../lib/stores/toast";

  const TEMPLATES: { icon: IconName; label: string; action: CreatorAction }[] = [
    { icon: "film", label: "Video", action: "templateVideo" },
    { icon: "code", label: "Web", action: "templateWeb" },
    { icon: "sparkles", label: "Shader", action: "templateShader" },
    { icon: "audio", label: "Visualizer", action: "templateVisualizer" },
  ];

  let project = $state<CreatorProject | null>(null);
  let selectedFile = $state("wallpaper.json");
  let res = $state<"2560" | "1920" | "portrait">("2560");
  let fakeAudio = $state(true);
  let onBattery = $state(false);
  let tab = $state<"properties" | "json" | "permissions" | "console">("properties");
  let selectedProp = $state("glow");
  let ignored = $state(false);
  let packing = $state(false);
  let packed = $state<PackResult | null>(null);
  let dragOver = $state(false);

  const issues = $derived(ignored ? [] : project?.issues ?? []);
  const permState = $derived<StepState>(issues.length ? "warn" : "done");
  const steps = $derived<{ id: string; title: string; state: StepState }[]>([
    { id: "validate", title: "Validate", state: "done" },
    { id: "perms", title: "Check permissions", state: permState },
    { id: "sign", title: "Sign", state: packed ? "done" : "current" },
    { id: "pack", title: "Pack", state: packed ? "done" : "todo" },
  ]);
  const manifest = $derived(project ? JSON.stringify({
    manifestVersion: 1, id: project.name, version: project.version, type: "shader", entry: "index.html",
    properties: Object.fromEntries(project.properties.map((p) => [p.key, { type: p.type, label: p.label, default: p.value }])),
    permissions: Object.fromEntries(project.declared.map((d) => [d, d === "network" ? [project!.issues[0]?.origin ?? "https://api.forecast.example"] : true])),
  }, null, 2) : "");
  const consoleCount = $derived(issues.length);
  const fps = $derived(onBattery ? 30 : 60);

  onMount(() => {
    attempt(async () => { project = await (await daemon()).creatorProject(); }, "Couldn't open the project");
  });

  async function declare(origin: string) {
    const p = await attempt(async () => (await daemon()).declareOrigin(origin), "Couldn't update wallpaper.json");
    if (p) { project = p; toast(`Declared ${origin.replace(/^https?:\/\//, "")} in wallpaper.json`); }
  }
  async function pack() {
    packing = true;
    const r = await attempt(async () => (await daemon()).packAndSign(), "Packing failed");
    packing = false;
    if (r) { packed = r; toast(`Packed ${r.file} (${formatBytes(r.sizeBytes)}), signed by ${r.signedBy}`); }
  }
  async function run(action: CreatorAction) {
    const msg = await attempt(async () => (await daemon()).creatorAction(action), "That didn't work");
    if (msg) toast(msg, "info");
  }
  function drop(e: DragEvent) {
    e.preventDefault();
    dragOver = false;
    if (e.dataTransfer?.files.length) run("addFiles");
  }
  const sliderPct = (p: Property) => (p.type === "slider" ? percentOf(p.value, p.min, p.max) : 0);
  const options = (p: Property) => {
    if (p.type === "slider") return p.unit === "%" ? `${p.min} – ${p.max} · as %` : `${p.min} – ${p.max} · step ${p.step}`;
    if (p.type === "color") return "Alpha off";
    if (p.type === "bool") return p.needs ? `Needs ${p.needs}` : "";
    return `${p.options.length} options`;
  };
  const defaultText = (p: Property) => (p.type === "combo" ? `"${p.value}"` : String(p.value));
</script>

<Titlebar>
  {#if project}
    <span class="row gap-8"><Icon name="folder" class="tb-ic" /><h1 class="t-h3">{project.name}</h1><span class="badge badge-neutral mono">{project.version}</span></span>
    {#if project.hotReload}<span class="badge badge-ok hot"><span class="dot"></span>Hot reload on · saved {project.savedSecondsAgo} s ago</span>{/if}
  {/if}
  <span class="spacer" data-tauri-drag-region></span>
  <button type="button" class="btn btn-ghost btn-sm" onclick={() => run("openEditor")}><Icon name="code" />Open in editor</button>
  <button type="button" class="btn btn-secondary btn-sm" onclick={() => run("tryOnDesktop")}><Icon name="monitor" />Try on my desktop</button>
  <button type="button" class="btn btn-primary btn-sm" disabled={packing} onclick={pack}><Icon name="package" />Pack &amp; sign</button>
</Titlebar>

{#if project}
  <div class="studio">
    <section class="pane" aria-label="Project files">
      <div class="pane-h"><h2 class="t-eyebrow">Project</h2><span class="spacer"></span><button type="button" class="icon-btn add" aria-label="New file" onclick={() => run("newFile")}><Icon name="plus" /></button></div>
      <label class="drop" class:over={dragOver} ondragover={(e) => { e.preventDefault(); dragOver = true; }} ondragleave={() => (dragOver = false)} ondrop={drop}>
        <div class="ic"><Icon name="upload" /></div>
        <div class="t-small strong">Drop a video, HTML folder or shader</div>
        <div class="t-micro ink-3 types">.mp4 · .webm · index.html · .frag / .glsl</div>
        <input class="sr-only" type="file" multiple accept=".mp4,.webm,.html,.frag,.glsl,.gif,.webp,.png,.jpg" onchange={(e) => { if (e.currentTarget.files?.length) run("addFiles"); }} />
      </label>
      <div class="tpl"><span class="ink-3 tpl-l">Templates</span>
        {#each TEMPLATES as t (t.label)}
          <button type="button" onclick={() => run(t.action)}><Icon name={t.icon} />{t.label}</button>
        {/each}
      </div>
      <div class="divider"></div>
      <ul class="tree" role="listbox" aria-label="Files">
        {#each project.files as f (f.name)}
          <li role="option" aria-selected={selectedFile === f.name} class="node" class:sel={selectedFile === f.name} class:child={f.depth > 0} tabindex={selectedFile === f.name ? 0 : -1}
            onclick={() => (selectedFile = f.name)} onkeydown={(e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); selectedFile = f.name; } }}>
            <Icon name={f.icon as IconName} />{f.name}{#if f.modified}<span class="mod" title="Unsaved changes"></span>{/if}{#if f.sizeBytes !== undefined}<span class="sz">{formatBytes(f.sizeBytes)}</span>{/if}
          </li>
        {/each}
      </ul>
      <div class="t-micro ink-3 foot">{project.fileCount} files · {formatBytes(project.totalBytes)} of {formatBytes(project.limitBytes)}</div>
    </section>

    <section class="pane center" aria-label="Preview and properties">
      <div class="row gap-8">
        <Seg label="Preview size" bind:value={res} options={[{ value: "2560", label: "2560×1440", icon: "monitor" }, { value: "1920", label: "1920×1080" }, { value: "portrait", label: "Portrait" }]} />
        <span class="spacer"></span>
        <span class="row gap-6 t-small ink-2"><Toggle bind:checked={fakeAudio} label="Fake audio" small />Fake audio</span>
        <span class="row gap-6 t-small ink-2 batt"><Toggle bind:checked={onBattery} label="Simulate battery" small />On battery</span>
      </div>
      <div class="pv" class:portrait={res === "portrait"}>
        <Art spec={project.art} label="Live preview" />
        <div class="hud"><span class="badge badge-glass mono">{fps} fps · {onBattery ? "1.2" : "2.1"} ms</span><span class="badge badge-glass mono">GPU {onBattery ? 1 : 3}%</span></div>
        <div class="hud2"><span class="badge badge-glass"><Icon name="refresh" />Reloaded</span></div>
        {#if fakeAudio}
          <div class="wave"><Icon name="audio" /><span class="bars" aria-hidden="true">{#each [6, 11, 15, 9, 13, 7, 12, 5, 10, 14, 8] as h, i (i)}<i style:height="{h}px"></i>{/each}</span>Lo-fi loop</div>
        {/if}
      </div>
      <div class="tabs" role="tablist" aria-label="Project details">
        <button type="button" role="tab" class:on={tab === "properties"} aria-selected={tab === "properties"} onclick={() => (tab = "properties")}>Properties <span class="ink-3">{project.properties.length}</span></button>
        <button type="button" role="tab" class:on={tab === "json"} aria-selected={tab === "json"} onclick={() => (tab = "json")}>wallpaper.json</button>
        <button type="button" role="tab" class:on={tab === "permissions"} aria-selected={tab === "permissions"} onclick={() => (tab = "permissions")}>Permissions</button>
        <button type="button" role="tab" class:on={tab === "console"} aria-selected={tab === "console"} onclick={() => (tab = "console")}>Console {#if consoleCount}<span class="badge badge-warn cnt">{consoleCount}</span>{/if}</button>
      </div>
      <div class="tabpanel" role="tabpanel">
        {#if tab === "properties"}
          <table class="schema">
            <thead><tr><th class="grip-h"><span class="sr-only">Order</span></th><th>Key</th><th>Type</th><th>Label</th><th>Default</th><th>Options</th><th class="shows">Shows as</th></tr></thead>
            <tbody>
              {#each project.properties as p (p.key)}
                <tr class:sel={selectedProp === p.key} onclick={() => (selectedProp = p.key)}>
                  <td class="grip"><Icon name="drag" /></td>
                  <td class="k">{p.key}</td>
                  <td><span class="tp {p.type}">{p.type}</span></td>
                  <td>{p.label}</td>
                  <td class="mono small">{#if p.type === "color"}<span class="row gap-6"><span class="sw14" style:background={p.value}></span>{p.value}</span>{:else}{defaultText(p)}{/if}</td>
                  <td class="ink-3 {p.type === 'slider' ? 'mono tiny' : 't-small'}">{options(p)}</td>
                  <td>
                    {#if p.type === "slider"}<div class="mini-slider" aria-hidden="true"><b style:width="{sliderPct(p)}%"></b><i style:left="{sliderPct(p)}%"></i></div>
                    {:else if p.type === "color"}<span class="row gap-6"><span class="sw22" style:background={p.value}></span><span class="sw22 fade"></span></span>
                    {:else if p.type === "bool"}<span class="toggle mini-t" class:on={p.value} aria-hidden="true"></span>
                    {:else}<span class="select mini-sel"><span>{p.options.find((o) => o.value === p.value)?.label}</span><Icon name="chevdown" /></span>{/if}
                  </td>
                </tr>
              {/each}
            </tbody>
          </table>
          <div class="row gap-8 add-row"><button type="button" class="btn btn-ghost btn-sm add-prop" onclick={() => run("addProperty")}><Icon name="plus" />Add property</button><span class="t-micro ink-3">Wallpapers read these with <span class="mono">wallweaver.onProperties()</span></span></div>
        {:else if tab === "json"}
          <pre class="code">{manifest}</pre>
        {:else if tab === "permissions"}
          <ul class="plist">
            {#each project.declared as d (d)}<li><Icon name="check" class="ok-ic" /><span class="mono">{d}</span></li>{/each}
            {#each issues as i (i.origin)}<li><Icon name="alert" class="warn-ic" /><span class="mono">network</span><span class="ink-3">used by {i.file}:{i.line}, not declared</span></li>{/each}
          </ul>
        {:else}
          <pre class="code">{#each issues as i (i.origin)}warn  {i.file}:{i.line}  {i.call} needs "{i.missing}" in wallpaper.json{"\n"}{:else}No messages.{/each}</pre>
        {/if}
      </div>
    </section>

    <section class="pane right" aria-label="Pack and sign">
      <div class="pane-h"><h2 class="t-h3">Pack &amp; sign</h2><span class="spacer"></span><span class="t-micro ink-3">.wwp · ed25519</span></div>
      <ol class="steps">
        {#each steps as s, n (s.id)}
          <li class="step {s.state === 'current' ? 'cur' : s.state}">
            <span class="n">{#if s.state === "done"}<Icon name="check" />{:else if s.state === "warn"}<Icon name="alert" />{:else}{n + 1}{/if}</span>
            <div>
              <div class="ttl">{s.title}</div>
              {#if s.id === "validate"}
                <div class="d">Manifest matches schema v1. Thumbnail 1280×720. No absolute paths or symlinks.</div>
              {:else if s.id === "perms"}
                <div class="d">Declared: <span class="mono ink-2">{project.declared.join(", ")}</span></div>
                {#each issues as i (i.origin)}
                  <div class="issue">
                    <span class="mono strong">{i.file}:{i.line}</span> calls <code>{i.call}</code> but <code>{i.missing}</code> isn’t declared, so it will be blocked.
                    <div class="row gap-6 issue-btns">
                      <button type="button" class="btn btn-secondary btn-sm" onclick={() => declare(i.origin)}>Declare this origin</button>
                      <button type="button" class="btn btn-ghost btn-sm" onclick={() => (ignored = true)}>Ignore</button>
                    </div>
                  </div>
                {/each}
              {:else if s.id === "sign"}
                <div class="sign-f">
                  <div class="input"><Icon name="key" class="key-ic" /><span class="grow">{project.signer.name} <span class="mono ink-3 fp">{project.signer.fingerprint}</span></span><span class="t-micro ink-3">{project.signer.store}</span></div>
                  <div class="row gap-8">
                    <label class="input ver"><span class="ink-3">v</span><input class="mono bare" aria-label="Version" bind:value={project.version} /></label>
                    <label class="input grow"><span class="sr-only">Release note</span><input class="bare" placeholder="What changed" bind:value={project.releaseNote} /></label>
                  </div>
                </div>
              {:else}
                <div class="d mono tiny">{packed ? packed.file : `${project.name}-${project.version}.wwp`}</div>
              {/if}
            </div>
          </li>
        {/each}
      </ol>
      <div class="actions">
        <button type="button" class="btn btn-primary btn-lg" disabled={packing} onclick={pack}><Icon name="package" />{packing ? "Packing…" : "Pack & sign"}</button>
        <button type="button" class="btn btn-secondary" onclick={() => run("submitToGallery")}><Icon name="git" />Submit to gallery<span class="ink-3 t-small light">opens a pull request</span></button>
      </div>
      <div class="cli" aria-label="Same steps from the terminal"><span class="p">$</span> wallweaver validate<br /><span class="p">$</span> wallweaver pack &amp;&amp; wallweaver sign<br /><span class="c"># same steps, from your terminal</span></div>
    </section>
  </div>
{/if}

<style>
  .studio { flex: 1; display: grid; grid-template-columns: 248px 1fr 344px; min-height: 0; }
  :global(.tb-ic) { width: 16px; height: 16px; color: var(--ink-3); }
  .hot { margin-left: 6px; }
  .hot .dot { width: 6px; height: 6px; border-radius: 50%; background: var(--ok); }
  .spacer { align-self: stretch; }
  .strong { font-weight: 600; color: var(--ink); }
  .pane { border-right: 1px solid var(--line); display: flex; flex-direction: column; min-height: 0; overflow: auto; }
  .pane.right { border-right: 0; border-left: 1px solid var(--line); }
  .pane-h { display: flex; align-items: center; gap: 8px; padding: 14px 16px 10px; }
  .add { width: 24px; height: 24px; }
  .drop { display: block; margin: 4px 14px 12px; border: 1.5px dashed var(--accent-line); border-radius: 14px; padding: 16px 14px; text-align: center; background: var(--accent-soft); cursor: pointer; }
  .drop.over, .drop:hover { background: rgba(165,150,255,0.22); }
  .drop:has(input:focus-visible) { outline: 2px solid var(--accent); outline-offset: 2px; }
  .drop .ic { width: 36px; height: 36px; margin: 0 auto 8px; border-radius: 10px; background: var(--accent); color: #120f22; display: grid; place-items: center; }
  .drop .ic :global(svg) { width: 18px; height: 18px; }
  .types { margin-top: 3px; }
  .tpl { display: flex; flex-wrap: wrap; gap: 6px; padding: 0 14px 14px; align-items: center; }
  .tpl button { height: 24px; padding: 0 9px; border-radius: 7px; display: inline-flex; align-items: center; gap: 5px; font-size: 11.5px; background: var(--panel-2); color: var(--ink-2); }
  .tpl button:hover { background: var(--panel-3); color: var(--ink); }
  .tpl :global(svg) { width: 12px; height: 12px; }
  .tpl-l { font-size: 11.5px; padding-right: 2px; }
  .tree { list-style: none; padding: 4px 8px; font-size: 12.5px; }
  .node { display: flex; align-items: center; gap: 8px; height: 28px; padding: 0 8px; border-radius: 7px; color: var(--ink-2); cursor: pointer; }
  .node.child { padding-left: 28px; }
  .node:hover { background: var(--panel); }
  .node :global(svg) { width: 15px; height: 15px; color: var(--ink-3); flex: none; }
  .node .sz { margin-left: auto; font-family: var(--mono); font-size: 10.5px; color: var(--ink-3); }
  .node.sel { background: var(--panel-2); color: var(--ink); box-shadow: inset 0 0 0 1px var(--line); }
  .node .mod { width: 6px; height: 6px; border-radius: 50%; background: var(--warn); }
  .foot { margin-top: auto; padding: 12px 16px 16px; }
  .center { padding: 14px 18px 0; display: flex; flex-direction: column; gap: 12px; min-width: 0; }
  .batt { margin-left: 6px; }
  .pv { position: relative; aspect-ratio: 16/8.6; border-radius: 16px; overflow: hidden; box-shadow: 0 0 0 1px var(--line); flex: none; background: #15131f; }
  .pv.portrait { aspect-ratio: auto; height: 400px; width: 225px; margin: 0 auto; }
  .pv :global(canvas) { position: absolute; inset: 0; }
  .pv .hud { position: absolute; z-index: 2; left: 12px; top: 12px; display: flex; gap: 6px; }
  .pv .hud2 { position: absolute; z-index: 2; right: 12px; top: 12px; display: flex; gap: 6px; }
  .wave { position: absolute; z-index: 2; left: 12px; bottom: 12px; display: flex; align-items: center; gap: 8px; height: 30px; padding: 0 10px; border-radius: 8px; background: rgba(12,10,20,0.6); backdrop-filter: blur(12px); color: #fff; font-size: 11.5px; box-shadow: inset 0 0 0 1px rgba(255,255,255,0.12); }
  .wave :global(svg) { width: 14px; height: 14px; }
  .bars { display: inline-flex; align-items: center; gap: 2px; height: 16px; }
  .bars i { display: inline-block; width: 3px; border-radius: 2px; background: #8fe3d3; }
  .cnt { height: 18px; margin-left: 4px; }
  .tabpanel { min-height: 0; }
  table.schema { width: 100%; border-collapse: collapse; font-size: 12.5px; }
  .schema th { text-align: left; font-weight: 500; font-size: 11px; color: var(--ink-3); padding: 8px 10px; border-bottom: 1px solid var(--line); letter-spacing: 0.02em; }
  .grip-h { width: 24px; }
  .shows { width: 120px; }
  .schema td { padding: 0 10px; height: 38px; border-bottom: 1px solid var(--line); vertical-align: middle; }
  .schema tr { cursor: pointer; }
  .schema tr:hover td { background: var(--panel); }
  .schema td.k { font-family: var(--mono); font-size: 11.5px; color: var(--ink); }
  .schema .grip :global(svg) { width: 14px; height: 14px; color: var(--ink-3); }
  .small { font-size: 11.5px; }
  .tiny { font-size: 11px; }
  .tp { display: inline-flex; align-items: center; height: 20px; padding: 0 7px; border-radius: 5px; font-size: 11px; font-weight: 600; font-family: var(--mono); }
  .tp.slider { color: #a596ff; background: rgba(165,150,255,0.13); }
  .tp.color { color: #ffb7a8; background: rgba(255,183,168,0.13); }
  .tp.bool { color: #8fe3d3; background: rgba(143,227,211,0.13); }
  .tp.combo { color: #f5c56e; background: rgba(245,197,110,0.13); }
  :global(:root[data-theme="light"]) .tp.slider { color: #5a45e6; }
  :global(:root[data-theme="light"]) .tp.color { color: #c8412f; }
  :global(:root[data-theme="light"]) .tp.bool { color: #11866f; }
  :global(:root[data-theme="light"]) .tp.combo { color: #a8670a; }
  .schema tr.sel td { background: var(--accent-soft); }
  .sw14 { width: 14px; height: 14px; border-radius: 4px; }
  .sw22 { width: 22px; height: 22px; border-radius: 6px; }
  .sw22.fade { background: #9a8cff; }
  .mini-slider { position: relative; width: 90px; height: 4px; border-radius: 4px; background: var(--panel-3); }
  .mini-slider b { position: absolute; left: 0; top: 0; bottom: 0; border-radius: 4px; background: var(--accent); }
  .mini-slider i { position: absolute; top: -4px; width: 12px; height: 12px; border-radius: 50%; background: #fff; transform: translateX(-50%); }
  .mini-t { transform: scale(.85); transform-origin: left; }
  .mini-sel { height: 26px; font-size: 11.5px; width: 104px; }
  .add-row { padding: 6px 0 14px; }
  .add-prop { padding-left: 6px; }
  .code { font-family: var(--mono); font-size: 11.5px; color: var(--ink-2); background: var(--panel); border-radius: 10px; padding: 12px; margin-top: 10px; white-space: pre-wrap; }
  .plist { list-style: none; display: flex; flex-direction: column; gap: 8px; padding: 12px 2px; font-size: 12.5px; }
  .plist li { display: flex; align-items: center; gap: 8px; }
  .plist :global(.ok-ic) { width: 14px; height: 14px; color: var(--ok); }
  .plist :global(.warn-ic) { width: 14px; height: 14px; color: var(--warn); }
  .steps { list-style: none; padding: 6px 16px 0; }
  .step { display: grid; grid-template-columns: 28px 1fr; gap: 12px; position: relative; padding-bottom: 16px; }
  .step::before { content: ""; position: absolute; left: 13.5px; top: 30px; bottom: 2px; width: 1px; background: var(--line-2); }
  .step:last-child::before { display: none; }
  .step .n { width: 28px; height: 28px; border-radius: 50%; display: grid; place-items: center; font-size: 12px; font-weight: 700; background: var(--panel-2); color: var(--ink-3); box-shadow: inset 0 0 0 1px var(--line-2); }
  .step .n :global(svg) { width: 14px; height: 14px; }
  .step.done .n { background: var(--ok-soft); color: var(--ok); box-shadow: inset 0 0 0 1px rgba(111,227,180,0.4); }
  .step.warn .n { background: var(--warn-soft); color: var(--warn); box-shadow: inset 0 0 0 1px rgba(245,197,110,0.4); }
  .step.cur .n { background: var(--accent); color: #120f22; box-shadow: 0 0 0 4px var(--accent-soft); }
  .step .ttl { font-weight: 600; font-size: 13px; line-height: 28px; }
  .step .d { font-size: 12px; color: var(--ink-3); }
  .issue { margin-top: 8px; padding: 10px; border-radius: 10px; background: var(--warn-soft); box-shadow: inset 0 0 0 1px rgba(245,197,110,0.3); font-size: 12px; color: var(--ink-2); }
  .issue code { font-family: var(--mono); font-size: 11px; color: var(--ink); overflow-wrap: anywhere; }
  .issue-btns { margin-top: 8px; }
  .issue-btns .btn { height: 26px; }
  .sign-f { display: flex; flex-direction: column; gap: 8px; margin-top: 4px; }
  .input { display: flex; align-items: center; gap: 8px; height: 32px; padding: 0 10px; border-radius: 8px; background: var(--panel-2); box-shadow: inset 0 0 0 1px var(--line); font-size: 12px; }
  .input:has(input:focus-visible) { box-shadow: inset 0 0 0 1px var(--accent); }
  .input.ver { width: 96px; }
  .bare { border: 0; background: none; outline: none; width: 100%; min-width: 0; font-size: 12px; color: var(--ink); }
  .bare::placeholder { color: var(--ink-3); }
  :global(.key-ic) { width: 14px; height: 14px; color: var(--ink-3); }
  .fp { font-size: 10.5px; }
  .actions { padding: 4px 16px 14px; display: flex; flex-direction: column; gap: 8px; }
  .light { font-weight: 400; }
  .cli { margin: 0 16px 16px; padding: 10px 12px; border-radius: 10px; background: #08070e; box-shadow: inset 0 0 0 1px var(--line); font-family: var(--mono); font-size: 11px; color: #b7b5c6; line-height: 1.6; }
  :global(:root[data-theme="light"]) .cli { background: #1a1726; }
  .cli .p { color: #8fe3d3; }
  .cli .c { color: #7d7a8f; }
</style>
