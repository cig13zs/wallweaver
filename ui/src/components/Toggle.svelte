<script lang="ts">
  let { checked = $bindable(false), label, onchange = undefined, small = false }: {
    checked?: boolean; label: string; onchange?: (v: boolean) => void; small?: boolean;
  } = $props();
  function flip() {
    checked = !checked;
    onchange?.(checked);
  }
</script>

<button type="button" role="switch" aria-checked={checked} aria-label={label} class="toggle" class:on={checked} class:small onclick={flip}></button>

<style>
  .small { transform: scale(0.85); }
</style>
