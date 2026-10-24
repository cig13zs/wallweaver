<script lang="ts">
  import { onMount } from "svelte";
  import Titlebar from "../components/Titlebar.svelte";
  import Toggle from "../components/Toggle.svelte";
  import Seg from "../components/Seg.svelte";
  import Icon from "../lib/Icon.svelte";
  import { daemon } from "../lib/daemon";
  import type { MotionPref, Settings, ThemePref } from "../lib/daemon/types";
  import { applyAppearance } from "../lib/stores/theme";
  import { status } from "../lib/stores/status";
  import { attempt } from "../lib/stores/toast";

  let s = $state<Settings | null>(null);

  onMount(() => {
    attempt(async () => { s = await (await daemon()).settings(); }, "Couldn't load settings");
  });

  async function save(patch: Partial<Settings>) {
    if (!s) return;
    const next = { ...$state.snapshot(s), ...patch };
    s = next;
    if (patch.theme || patch.motion) applyAppearance({ theme: next.theme, motion: next.motion });
    const saved = await attempt(async () => (await daemon()).updateSettings(next), "Couldn't save settings");
    if (saved) s = saved;
  }
</script>

<Titlebar>
  <h1 class="t-h3 page-title">Settings</h1>
  <span class="spacer" data-tauri-drag-region></span>
</Titlebar>

{#if s}
  <section class="content">
    <div class="wrap">
      <div class="card">
        <h2 class="t-eyebrow sec">Appearance</h2>
        <div class="srow"><div class="grow"><div class="t">Theme</div><div class="d">Dark is the default. System follows your OS.</div></div>
          <Seg label="Theme" value={s.theme} options={[{ value: "system", label: "System", icon: "auto" }, { value: "dark", label: "Dark", icon: "moon" }, { value: "light", label: "Light", icon: "sun" }]} onchange={(v: ThemePref) => save({ theme: v })} />
        </div>
        <div class="srow"><div class="grow"><div class="t">Motion</div><div class="d">Reduce turns off hover lifts and transitions in this window. Wallpapers have their own settings.</div></div>
          <Seg label="Motion" value={s.motion} options={[{ value: "system", label: "Follow system" }, { value: "reduce", label: "Reduce" }, { value: "full", label: "Full" }]} onchange={(v: MotionPref) => save({ motion: v })} />
        </div>
      </div>

      <div class="card">
        <h2 class="t-eyebrow sec">Startup</h2>
        <div class="srow"><div class="grow"><div class="t">Start wallpapers when I sign in</div><div class="d">Starts the small background service, not this window.</div></div>
          <Toggle label="Start wallpapers when I sign in" checked={s.startWithSystem} onchange={(v) => save({ startWithSystem: v })} />
        </div>
        <div class="srow"><div class="grow"><div class="t">Show the tray icon</div><div class="d">Quick controls for pause, next and playlists.</div></div>
          <Toggle label="Show the tray icon" checked={s.trayIcon} onchange={(v) => save({ trayIcon: v })} />
        </div>
        <div class="srow"><div class="grow"><div class="t">Pause all wallpapers</div><div class="d">Works anywhere, even with this window closed.</div></div>
          <span class="kbd big-kbd">{s.pauseHotkey}</span>
        </div>
        <div class="note"><Icon name="info" />Closing this window only closes the manager. Your wallpapers keep running.</div>
      </div>

      <div class="card">
        <h2 class="t-eyebrow sec">Gallery</h2>
        <div class="srow"><div class="grow"><div class="t">Check for updates and pulled packages daily</div><div class="d">Pulled packages are disabled with a note saying why.</div></div>
          <Toggle label="Check for updates and pulled packages daily" checked={s.checkGalleryUpdates} onchange={(v) => save({ checkGalleryUpdates: v })} />
        </div>
        <div class="srow"><div class="grow"><div class="t">Sources</div><div class="d">Anyone can host a gallery. Packages from every source are signature-checked.</div></div>
          <ul class="sources">{#each s.gallerySources as src (src)}<li><Icon name="server" />{src}</li>{/each}</ul>
        </div>
      </div>

      <div class="card">
        <h2 class="t-eyebrow sec">Developer</h2>
        <div class="srow"><div class="grow"><div class="t">Developer mode</div><div class="d">Allows unsigned local packages. They show a warning badge and still run in the sandbox.</div></div>
          <Toggle label="Developer mode" checked={s.developerMode} onchange={(v) => save({ developerMode: v })} />
        </div>
      </div>

      <div class="card about">
        <span class="t-small ink-2">Wallweaver {$status?.version ?? ""} · MIT OR Apache-2.0 · no telemetry</span>
        <button type="button" class="btn btn-secondary btn-sm" onclick={() => attempt(async () => (await daemon()).openLink("source"))}><Icon name="git" />Source code</button>
      </div>
    </div>
  </section>
{/if}

<style>
  .page-title { font-size: 15px; }
  .spacer { align-self: stretch; }
  .wrap { max-width: 820px; display: flex; flex-direction: column; gap: 14px; }
  .sec { padding: 14px 16px 4px; }
  .srow { display: flex; align-items: center; gap: 16px; padding: 12px 16px; border-top: 1px solid var(--line); }
  .sec + .srow { border-top: 0; }
  .t { font-size: 13px; font-weight: 500; }
  .d { font-size: 11.5px; color: var(--ink-3); margin-top: 1px; }
  .big-kbd { font-size: 12px; line-height: 22px; padding: 0 8px; }
  .note { display: flex; align-items: center; gap: 8px; margin: 0 16px 14px; padding: 10px 12px; border-radius: 10px; background: var(--info-soft); color: var(--ink-2); font-size: 12px; }
  .note :global(svg) { width: 15px; height: 15px; color: var(--info); }
  .sources { list-style: none; display: flex; flex-direction: column; gap: 6px; }
  .sources li { display: flex; align-items: center; gap: 8px; height: 28px; padding: 0 10px; border-radius: 8px; background: var(--panel-2); font-size: 12px; }
  .sources :global(svg) { width: 14px; height: 14px; color: var(--ink-3); }
  .about { display: flex; align-items: center; justify-content: space-between; padding: 12px 16px; }
</style>
