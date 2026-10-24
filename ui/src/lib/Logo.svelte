<script lang="ts">
  let { size = 28 }: { size?: number } = $props();
  const id = `ww-logo-${Math.random().toString(36).slice(2, 8)}`;
</script>

<!-- Two interlaced strands forming a W, on the loom gradient -->
<svg width={size} height={size} viewBox="0 0 32 32" role="img" aria-label="Wallweaver">
  <defs>
    <linearGradient {id} x1="0" y1="0" x2="1" y2="1">
      <stop offset="0" stop-color="#ffb7a8" /><stop offset=".5" stop-color="#c6a8ff" /><stop offset="1" stop-color="#8fe3d3" />
    </linearGradient>
  </defs>
  <rect width="32" height="32" rx="9" fill="url(#{id})" />
  <path d="M6.5 9.5 11.5 23 16 13" fill="none" stroke="#17132b" stroke-width="2.6" stroke-linecap="round" stroke-linejoin="round" />
  <path d="M16 13l4.5 10 5-13.5" fill="none" stroke="#fff" stroke-width="2.6" stroke-linecap="round" stroke-linejoin="round" />
  <path d="M14.2 17.2 16 13" fill="none" stroke="#17132b" stroke-width="2.6" stroke-linecap="round" />
</svg>
