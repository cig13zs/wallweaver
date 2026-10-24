<script lang="ts">
  import { ICONS, type IconName } from "./icons";
  let { name, size = undefined, class: cls = "" }: { name: IconName; size?: number; class?: string } = $props();
</script>

<svg class="ic {cls}" width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true" focusable="false">{@html ICONS[name]}</svg>
