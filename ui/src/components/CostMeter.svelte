<script lang="ts">
  import type { Cost } from "../lib/daemon/types";
  import { costLabel } from "../lib/format";
  let { cost, extra = "", label = undefined, variant = "glass" }: {
    cost: Cost; extra?: string; label?: string; variant?: "glass" | "panel" | "status";
  } = $props();
  const text = $derived(`${label ?? costLabel(cost)}${extra ? ` · ${extra}` : ""}`);
</script>

<span class="cost {cost}" class:panel={variant === "panel"} class:status={variant === "status"} title="Running cost: {text}"><span class="pips" aria-hidden="true"><i></i><i></i><i></i></span>{text}</span>

<style>
  .cost.panel { background: var(--panel-2); color: var(--ink); box-shadow: inset 0 0 0 1px var(--line); }
  .cost.status { background: var(--ok-soft); color: var(--ok); box-shadow: none; }
</style>
