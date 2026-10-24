<script lang="ts">
  import { onMount } from "svelte";
  import Titlebar from "../components/Titlebar.svelte";
  import Icon from "../lib/Icon.svelte";
  import Art from "../lib/art/Art.svelte";
  import type { IconName } from "../lib/icons";
  import { daemon } from "../lib/daemon";
  import type { Funding, LinkId } from "../lib/daemon/types";
  import { attempt } from "../lib/stores/toast";

  const NOTIFY_KEY = "ww.notifyPacks";
  const TIERS: { name: string; tagline: string; price: number; icon: IconName; perks: number; pick?: boolean }[] = [
    { name: "Thread", tagline: "Say thanks", price: 3, icon: "heart", perks: 2 },
    { name: "Weave", tagline: "Try things first", price: 5, icon: "sparkles", perks: 3, pick: true },
    { name: "Loom", tagline: "Help steer it", price: 10, icon: "flag", perks: 4 },
  ];
  const PERKS = ["Supporter badge in the app and gallery", "Your name in the credits", "Early builds, a week before release", "Vote on the roadmap"];

  let funding = $state<Funding | null>(null);
  let notify = $state(false);

  onMount(() => {
    try { notify = localStorage.getItem(NOTIFY_KEY) === "1"; } catch { /* storage unavailable */ }
    daemon().then((d) => d.funding()).then((f) => { funding = f; }).catch(() => {});
  });

  function open(id: LinkId) {
    attempt(async () => (await daemon()).openLink(id), "Couldn't open the link");
  }
  function toggleNotify() {
    notify = !notify;
    try { localStorage.setItem(NOTIFY_KEY, notify ? "1" : "0"); } catch { /* storage unavailable */ }
  }
  const pct = $derived(funding ? Math.min(100, (funding.raised / funding.target) * 100) : 0);
</script>

<Titlebar>
  <h1 class="t-h3 page-title">Support Wallweaver</h1>
  <span class="spacer" data-tauri-drag-region></span>
  <button type="button" class="btn btn-ghost btn-sm" onclick={() => open("funding")}><Icon name="info" />Where the money goes</button>
  <button type="button" class="btn btn-secondary btn-sm" onclick={() => open("source")}><Icon name="git" />Source code</button>
</Titlebar>

<section class="content">
  <div class="card hero">
    <div class="hero-l">
      <div class="row gap-8"><span class="badge badge-accent"><Icon name="code" />MIT OR Apache-2.0</span><span class="badge badge-neutral">Every feature in the free build</span></div>
      <h2 class="hero-h">Wallweaver is free and open source.<br /><span class="grad">No ads, ever.</span></h2>
      <p>Nothing here unlocks features. Supporters pay for code signing, test machines for Windows, macOS and Linux, and commissions for new wallpaper artists.</p>
      <div class="row gap-8 hero-btns">
        <button type="button" class="btn btn-primary btn-lg" onclick={() => open("sponsor")}><Icon name="heart" />Become a supporter</button>
        <button type="button" class="btn btn-lg tip" onclick={() => open("tip")}><Icon name="coffee" />One-off tip</button>
        <button type="button" class="btn btn-ghost btn-lg" onclick={() => open("source")}><Icon name="star" />Star the repo</button>
      </div>
    </div>
    <div class="hero-r">
      <Art spec={{ kind: "mesh", seed: 6 }} />
      {#if funding}
        <div class="goal">
          <div class="row"><span class="t-eyebrow goal-eb">{funding.month} goal</span><span class="spacer"></span><span class="t-micro dim">{funding.supporters} supporters</span></div>
          <div class="row goal-num"><span class="tnum big">${funding.raised}</span><span class="t-small dim">of ${funding.target} a month</span></div>
          <div class="bar" role="progressbar" aria-label="Monthly goal" aria-valuemin={0} aria-valuemax={funding.target} aria-valuenow={funding.raised}><b style:width="{pct}%"></b></div>
          <div class="t-micro dim2">Covers: {funding.covers.join(" · ")}</div>
        </div>
      {/if}
    </div>
  </div>

  <div class="sec-h"><h2 class="t-h2">Monthly support</h2><span class="t-small ink-3">Same tiers on the tip page and the sponsor page. Cancel anytime.</span></div>
  <div class="tiers">
    {#each TIERS as t (t.name)}
      <div class="card tier" class:pick={t.pick}>
        <div class="top">
          <span class="ic"><Icon name={t.icon} /></span>
          <div><div class="t-h3">{t.name}{#if t.pick}<span class="badge badge-accent most">Most picked</span>{/if}</div><div class="t-micro ink-3">{t.tagline}</div></div>
          <div class="price"><b>${t.price}</b><span>/ month</span></div>
        </div>
        <ul class="perks">
          {#each PERKS as p, i (p)}
            <li class="perk" class:off={i >= t.perks}><span class="m"><Icon name={i < t.perks ? "check" : "minus"} /></span>{i === 3 && t.perks === 4 ? `${p} every month` : p}<span class="sr-only">{i < t.perks ? "(included)" : "(not included)"}</span></li>
          {/each}
        </ul>
        <div class="btns">
          <button type="button" class="btn btn-sm {t.pick ? 'btn-primary' : 'btn-secondary'}" onclick={() => open("tip")}><Icon name="coffee" />Tip page</button>
          <button type="button" class="btn btn-secondary btn-sm" onclick={() => open("sponsor")}><Icon name="git" />Sponsor page</button>
        </div>
      </div>
    {/each}
  </div>

  <div class="bottom">
    <div class="card offer">
      <div class="offer-h">
        <span class="ic"><Icon name="store" /></span>
        <div class="grow"><h3 class="t-h3">Supporter build in the app store</h3><div class="tag">Windows · one-time purchase</div></div>
        <div class="price"><b class="b22">~$2.99</b><span>once</span></div>
      </div>
      <p>It's the same app with the same features. Buying it through the store is a one-off way to fund development, and the store keeps it updated for you. The free download never falls behind.</p>
      <div class="row gap-8"><button type="button" class="btn btn-secondary" onclick={() => open("store")}><Icon name="external" />View in the store</button><button type="button" class="btn btn-ghost" onclick={() => open("source")}><Icon name="download" />Free build</button></div>
    </div>
    <div class="card offer">
      <div class="offer-h">
        <span class="ic"><Icon name="palette" /></span>
        <div class="grow"><h3 class="t-h3">Premium packs<span class="badge badge-neutral soon">Coming soon</span></h3><div class="tag">Commissioned from independent artists</div></div>
        <button type="button" class="btn btn-secondary btn-sm" aria-pressed={notify} onclick={toggleNotify}><Icon name={notify ? "check" : "bell"} />{notify ? "We'll tell you" : "Notify me"}</button>
      </div>
      <div class="packs">
        <div class="thumb"><Art spec={{ kind: "sakura", seed: 21 }} /></div>
        <div class="thumb"><Art spec={{ kind: "synthwave", seed: 7 }} /></div>
        <div class="thumb"><Art spec={{ kind: "lowpoly", seed: 12 }} /></div>
      </div>
      <div>
        <div class="row t-small split-l"><span class="strong">70% goes to the artist who made the pack</span><span class="spacer"></span><span class="ink-3">30% to the project</span></div>
        <div class="split" aria-hidden="true"><span class="a"></span><span class="p"></span></div>
      </div>
    </div>
  </div>

  <div class="promise">
    <span class="lead"><Icon name="shieldcheck" /></span>
    <div class="grow t-small"><span class="strong">Security and performance are never paywalled.</span> <span class="ink-2">The sandbox, signed packages, pause rules and power presets are in every build, free or paid.</span></div>
    <button type="button" class="t-small policy" onclick={() => open("funding")}>Read the funding policy</button>
  </div>
</section>

<style>
  .page-title { font-size: 15px; }
  .spacer { align-self: stretch; }
  .strong { font-weight: 600; color: var(--ink); }
  .content { display: flex; flex-direction: column; gap: 16px; padding-top: 20px; }
  .hero { display: grid; grid-template-columns: 1fr 400px; overflow: hidden; position: relative; flex: none; }
  .hero::before { content: ""; position: absolute; inset: 0; pointer-events: none; background: radial-gradient(600px 260px at 0% 0%, rgba(255,183,168,0.10), transparent 70%); }
  .hero-l { padding: 24px 26px 22px; display: flex; flex-direction: column; position: relative; }
  .hero-h { font-size: 32px; line-height: 1.12; font-weight: 650; letter-spacing: -0.025em; margin-top: 10px; }
  .grad { background: var(--loom); -webkit-background-clip: text; background-clip: text; color: transparent; }
  :global(:root[data-theme="light"]) .grad { background: linear-gradient(120deg, #d9573f 0%, #6e5af0 50%, #11866f 100%); -webkit-background-clip: text; background-clip: text; }
  .hero-l p { color: var(--ink-2); font-size: 13.5px; margin-top: 10px; max-width: 660px; }
  .hero-btns { margin-top: auto; padding-top: 18px; }
  .hero-r { position: relative; margin: 10px 10px 10px 0; border-radius: 12px; overflow: hidden; min-height: 240px; background: #f3ecfb; }
  .hero-r :global(canvas) { position: absolute; inset: 0; }
  .goal { position: absolute; left: 14px; right: 14px; bottom: 14px; z-index: 2; padding: 14px; border-radius: 12px; color: #fff; background: rgba(12,10,20,0.62); backdrop-filter: blur(16px); box-shadow: inset 0 0 0 1px rgba(255,255,255,0.14); }
  .goal-eb { color: rgba(255,255,255,0.7); }
  .dim { opacity: .75; }
  .dim2 { opacity: .8; }
  .goal-num { margin-top: 6px; align-items: baseline; gap: 6px; }
  .big { font-size: 24px; font-weight: 650; letter-spacing: -0.02em; }
  .bar { height: 8px; border-radius: 99px; background: rgba(255,255,255,0.16); overflow: hidden; margin: 10px 0 8px; }
  .bar b { display: block; height: 100%; border-radius: 99px; background: var(--loom); }
  .tip { background: linear-gradient(135deg, rgba(255,183,168,0.16), rgba(198,168,255,0.14)); box-shadow: inset 0 0 0 1px rgba(255,183,168,0.35); color: var(--ink); }
  .tip:hover { background: linear-gradient(135deg, rgba(255,183,168,0.24), rgba(198,168,255,0.2)); }
  :global(:root[data-theme="light"]) .tip { background: linear-gradient(135deg, rgba(232,110,88,0.12), rgba(110,90,240,0.10)); box-shadow: inset 0 0 0 1px rgba(217,87,63,0.35); }
  .sec-h { display: flex; align-items: baseline; gap: 12px; }
  .tiers { display: grid; grid-template-columns: repeat(3, 1fr); gap: 14px; flex: none; }
  .tier { padding: 16px 16px 14px; display: flex; flex-direction: column; gap: 12px; position: relative; }
  .tier.pick { border-color: var(--accent-line); background: linear-gradient(180deg, var(--accent-soft), var(--panel) 70%); box-shadow: 0 0 0 1px var(--accent-line); }
  .top { display: flex; align-items: center; gap: 10px; }
  .tier .ic { width: 34px; height: 34px; border-radius: 10px; display: grid; place-items: center; background: var(--panel-2); box-shadow: inset 0 0 0 1px var(--line); color: var(--ink-2); }
  .tier .ic :global(svg) { width: 17px; height: 17px; }
  .tier.pick .ic { background: var(--accent); color: #120f22; box-shadow: none; }
  .most { height: 18px; margin-left: 8px; }
  .price { display: flex; align-items: baseline; gap: 4px; margin-left: auto; }
  .price b { font-size: 26px; font-weight: 650; letter-spacing: -0.02em; font-variant-numeric: tabular-nums; }
  .price .b22 { font-size: 22px; }
  .price span { font-size: 12px; color: var(--ink-3); }
  .perks { list-style: none; display: flex; flex-direction: column; gap: 7px; font-size: 12.5px; }
  .perk { display: flex; align-items: center; gap: 9px; color: var(--ink); }
  .perk .m { width: 18px; height: 18px; border-radius: 50%; display: grid; place-items: center; flex: none; background: var(--ok-soft); color: var(--ok); }
  .perk .m :global(svg) { width: 11px; height: 11px; stroke-width: 2.4; }
  .perk.off { color: var(--ink-3); }
  .perk.off .m { background: var(--panel-2); color: var(--ink-3); }
  .btns { display: grid; grid-template-columns: 1fr 1fr; gap: 8px; margin-top: 2px; }
  .bottom { display: grid; grid-template-columns: 1fr 1fr; gap: 14px; flex: 1; min-height: 0; }
  .offer { padding: 16px; display: grid; grid-template-columns: 1fr; gap: 12px; align-content: start; }
  .offer-h { display: flex; align-items: center; gap: 12px; }
  .offer-h .ic { width: 40px; height: 40px; border-radius: 11px; display: grid; place-items: center; background: var(--panel-2); box-shadow: inset 0 0 0 1px var(--line); color: var(--ink); flex: none; }
  .offer-h .ic :global(svg) { width: 19px; height: 19px; }
  .soon { height: 18px; margin-left: 8px; }
  .offer p { font-size: 12.5px; color: var(--ink-2); }
  .tag { font-size: 11px; color: var(--ink-3); }
  .packs { display: grid; grid-template-columns: repeat(3, 1fr); gap: 8px; height: 60px; }
  .packs .thumb { border-radius: 9px; box-shadow: 0 0 0 1px var(--line); }
  .split-l { margin-bottom: 6px; }
  .split { display: flex; height: 8px; border-radius: 99px; overflow: hidden; gap: 2px; }
  .split .a { width: 70%; background: var(--ok); }
  .split .p { flex: 1; background: var(--panel-3); }
  .promise { display: flex; align-items: center; gap: 12px; padding: 12px 16px; border-radius: var(--r-lg); background: var(--ok-soft); box-shadow: inset 0 0 0 1px rgba(111,227,180,0.25); flex: none; }
  :global(:root[data-theme="light"]) .promise { box-shadow: inset 0 0 0 1px rgba(19,138,94,0.25); }
  .lead { color: var(--ok); display: inline-flex; }
  .lead :global(svg) { width: 20px; height: 20px; }
  .policy { color: var(--accent); font-weight: 550; white-space: nowrap; padding: 0; }
  .policy:hover { text-decoration: underline; }
</style>
