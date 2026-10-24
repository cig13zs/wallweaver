<script lang="ts">
  import { onMount } from "svelte";
  import Icon from "../lib/Icon.svelte";
  import Art from "../lib/art/Art.svelte";
  import type { IconName } from "../lib/icons";
  import { daemon } from "../lib/daemon";
  import type { InstallRequest, PermissionKey } from "../lib/daemon/types";
  import { formatBytes } from "../lib/format";
  import { refreshStatus } from "../lib/stores/status";
  import { attempt, toast } from "../lib/stores/toast";

  let { id, onclose }: { id: string; onclose: (installed: boolean) => void } = $props();

  let dlg: HTMLDialogElement;
  let req = $state<InstallRequest | null>(null);
  let allow = $state<Record<string, boolean>>({});
  let showManifest = $state(false);
  let busy = $state(false);

  const total = $derived(req?.permissions.length ?? 0);
  const allowed = $derived(req ? req.permissions.filter((p) => allow[p.key]).length : 0);
  const manifest = $derived(req ? JSON.stringify({ permissions: Object.fromEntries(req.permissions.map((p) => [p.key, p.tech.includes(":") ? p.tech.split(":")[1]!.trim() : true])) }, null, 2) : "");

  onMount(() => {
    dlg.showModal();
    attempt(async () => {
      req = await (await daemon()).installRequest(id);
      allow = Object.fromEntries(req.permissions.map((p) => [p.key, true]));
    }, "Couldn't read this wallpaper's permissions");
  });

  function setAllow(key: PermissionKey, v: boolean) {
    allow[key] = v;
  }

  function choiceKeys(e: KeyboardEvent, key: PermissionKey) {
    if (["ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown"].includes(e.key)) {
      e.preventDefault();
      setAllow(key, !allow[key]);
      const group = e.currentTarget as HTMLElement;
      queueMicrotask(() => group.querySelector<HTMLElement>('[aria-checked="true"]')?.focus());
    }
  }

  async function install() {
    if (!req) return;
    busy = true;
    const granted = req.permissions.filter((p) => allow[p.key]).map((p) => p.key);
    const res = await attempt(async () => (await daemon()).install(req!.wallpaperId, granted), "Install failed");
    busy = false;
    if (!res) return;
    await refreshStatus().catch(() => {});
    toast(total ? `${req.title} installed with ${res.granted.length} of ${total} permissions allowed` : `${req.title} installed`);
    close(true);
  }

  function close(installed = false) {
    if (dlg.open) dlg.close();
    onclose(installed);
  }
</script>

<dialog bind:this={dlg} class="modal" aria-labelledby="consent-title" oncancel={(e) => { e.preventDefault(); close(); }}>
  {#if req}
    <div class="m-head">
      <div class="thumb head-thumb"><Art spec={req.art} /></div>
      <div class="grow">
        <div class="t-eyebrow accent">Before you install</div>
        <h2 class="t-h2 title" id="consent-title">{req.title} {total ? "wants to:" : "asks for nothing extra"}</h2>
        <div class="t-small ink-3 sub">
          by {req.author} · v{req.version} · {req.kindLabel} · {formatBytes(req.sizeBytes)} ·
          <button type="button" class="linkish" aria-expanded={showManifest} onclick={() => (showManifest = !showManifest)}>View wallpaper.json</button>
        </div>
      </div>
      <button type="button" class="icon-btn close-x" aria-label="Close" onclick={() => close()}><Icon name="x" /></button>
    </div>

    {#if showManifest}<pre class="manifest mono">{manifest}</pre>{/if}

    {#if req.signature}
      <div class="sig">
        <Icon name="badge" />
        <div class="grow"><span class="strong">Signed by {req.signature.signer}</span>{req.signature.unchanged ? " · unchanged since signing" : ""}{req.signature.listed ? " · listed in the gallery" : ""}</div>
        <span class="mono fp">ed25519 {req.signature.fingerprint}</span>
      </div>
    {:else}
      <div class="sig unsigned"><Icon name="alert" /><div class="grow"><span class="strong">Not signed.</span> Only install this if you trust where it came from.</div></div>
    {/if}

    <div class="perms">
      {#each req.permissions as p (p.key)}
        <div class="perm" class:denied={!allow[p.key]}>
          <span class="ic"><Icon name={p.icon as IconName} /></span>
          <div>
            <div class="pt" id="perm-{p.key}">{p.title}{#if p.target}{" "}<b>{p.target}</b>{/if}</div>
            <div class="psub">{p.detail}{#if p.ifDenied}<br /><span class="ink-2">{p.ifDenied}</span>{/if}</div>
            <span class="tech">{p.tech}</span>
          </div>
          <!-- svelte-ignore a11y_interactive_supports_focus -->
          <div class="ad" role="radiogroup" aria-labelledby="perm-{p.key}" onkeydown={(e) => choiceKeys(e, p.key)}>
            <button type="button" role="radio" class="allow" class:on={allow[p.key]} aria-checked={!!allow[p.key]} tabindex={allow[p.key] ? 0 : -1} onclick={() => setAllow(p.key, true)}>{#if allow[p.key]}<Icon name="check" />{/if}Allow</button>
            <button type="button" role="radio" class="deny" class:on={!allow[p.key]} aria-checked={!allow[p.key]} tabindex={allow[p.key] ? -1 : 0} onclick={() => setAllow(p.key, false)}>{#if !allow[p.key]}<Icon name="x" />{/if}Deny</button>
          </div>
        </div>
      {/each}
    </div>

    <div class="blocked">
      <div class="row gap-8"><Icon name="lock" class="lock" /><span class="t-h3 bl-title">Always blocked, for every wallpaper</span><span class="spacer"></span><span class="t-micro ink-3">Enforced by Wallweaver, not by trust</span></div>
      <ul class="bl-chips">
        {#each req.alwaysBlocked as b (b.label)}<li><Icon name={b.icon as IconName} />{b.label}</li>{/each}
      </ul>
    </div>

    <div class="m-foot">
      <Icon name="info" class="foot-ic" />
      <span class="t-small ink-3 grow">You can change these anytime in Settings › Permissions.</span>
      <button type="button" class="btn btn-ghost" onclick={() => close()}>Cancel</button>
      <button type="button" class="btn btn-primary btn-lg" disabled={busy} onclick={install}>
        {total ? `Install with ${allowed} of ${total} allowed` : "Install"}
      </button>
    </div>
  {:else}
    <div class="wait" aria-busy="true">Checking signature and permissions…</div>
  {/if}
</dialog>

<style>
  .modal { margin: auto; width: 760px; max-width: calc(100vw - 48px); max-height: calc(100vh - 48px); overflow: auto; border: 0; padding: 0; border-radius: 20px; background: var(--bg-elev); color: var(--ink); box-shadow: var(--shadow-pop); }
  .modal::backdrop { background: rgba(6,6,12,0.62); backdrop-filter: blur(3px) saturate(0.9); }
  :global(:root[data-theme="light"]) .modal::backdrop { background: rgba(236,234,246,0.7); }
  .m-head { padding: 22px 24px 18px; display: flex; gap: 16px; align-items: center; background: linear-gradient(180deg, rgba(165,150,255,0.08), rgba(165,150,255,0)); }
  .head-thumb { width: 104px; height: 64px; border-radius: 12px; flex: none; box-shadow: 0 0 0 1px var(--line-2); }
  .accent { color: var(--accent); }
  .title { margin-top: 4px; font-size: 20px; }
  .sub { margin-top: 3px; }
  .linkish { color: var(--accent); font-size: inherit; padding: 0; }
  .linkish:hover { text-decoration: underline; }
  .close-x { align-self: flex-start; }
  .manifest { margin: 0 24px 12px; padding: 10px 12px; border-radius: 10px; background: var(--panel-2); font-size: 11px; color: var(--ink-2); white-space: pre-wrap; }
  .sig { display: flex; align-items: center; gap: 10px; margin: 0 24px; padding: 10px 12px; border-radius: 12px; background: var(--ok-soft); color: var(--ink-2); font-size: 12px; box-shadow: inset 0 0 0 1px rgba(111,227,180,0.22); }
  .sig :global(svg) { width: 18px; height: 18px; color: var(--ok); flex: none; }
  .sig.unsigned { background: var(--warn-soft); box-shadow: inset 0 0 0 1px rgba(245,197,110,0.3); }
  .sig.unsigned :global(svg) { color: var(--warn); }
  .strong { color: var(--ink); font-weight: 600; }
  .fp { font-size: 10.5px; color: var(--ink-3); }
  .perms { padding: 6px 24px 0; }
  .perm { display: grid; grid-template-columns: 36px 1fr auto; gap: 14px; align-items: center; padding: 13px 0; border-bottom: 1px solid var(--line); }
  .perm:last-child { border-bottom: 0; }
  .perm .ic { width: 36px; height: 36px; border-radius: 10px; display: grid; place-items: center; background: var(--panel-2); box-shadow: inset 0 0 0 1px var(--line); color: var(--ink); }
  .perm .ic :global(svg) { width: 18px; height: 18px; }
  .pt { font-size: 14px; font-weight: 550; }
  .pt b { font-weight: 650; color: var(--accent); }
  .psub { font-size: 12px; color: var(--ink-3); margin-top: 2px; }
  .tech { font-family: var(--mono); font-size: 10.5px; color: var(--ink-3); margin-top: 5px; display: inline-block; padding: 1px 6px; border-radius: 5px; background: var(--panel-2); }
  .ad { display: inline-flex; padding: 3px; border-radius: 10px; background: var(--panel-2); box-shadow: inset 0 0 0 1px var(--line); }
  .ad button { height: 28px; padding: 0 12px; display: inline-flex; align-items: center; gap: 5px; border-radius: 7px; font-size: 12.5px; font-weight: 550; color: var(--ink-3); }
  .ad button:hover:not(.on) { color: var(--ink); }
  .ad :global(svg) { width: 13px; height: 13px; }
  .ad .allow.on { background: var(--ok-soft); color: var(--ok); box-shadow: inset 0 0 0 1px rgba(111,227,180,0.35); }
  .ad .deny.on { background: var(--bad-soft); color: var(--bad); box-shadow: inset 0 0 0 1px rgba(255,139,126,0.35); }
  .perm.denied .pt { color: var(--ink-2); }
  .perm.denied .ic { color: var(--ink-3); }
  .blocked { margin: 4px 24px 0; padding: 14px; border-radius: 14px; background: var(--panel); box-shadow: inset 0 0 0 1px var(--line); }
  .blocked :global(.lock) { width: 15px; height: 15px; color: var(--ink-2); }
  .bl-title { font-size: 13px; }
  .bl-chips { list-style: none; display: flex; flex-wrap: wrap; gap: 6px; margin-top: 10px; }
  .bl-chips li { display: inline-flex; align-items: center; gap: 6px; height: 26px; padding: 0 10px; border-radius: 99px; font-size: 12px; color: var(--ink-2); background: var(--panel-2); }
  .bl-chips :global(svg) { width: 13px; height: 13px; color: var(--bad); }
  .m-foot { display: flex; align-items: center; gap: 10px; padding: 16px 24px 20px; margin-top: 18px; border-top: 1px solid var(--line); }
  .m-foot :global(.foot-ic) { width: 15px; height: 15px; color: var(--ink-3); }
  .wait { padding: 48px; text-align: center; color: var(--ink-3); }
</style>
