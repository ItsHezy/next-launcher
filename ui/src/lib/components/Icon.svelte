<script lang="ts">
  import { ICONS, type IconName } from '../icons';

  let { name, size = 20, stroke = 1.8 }: { name: IconName; size?: number; stroke?: number } = $props();
</script>

<svg
  width={size}
  height={size}
  viewBox="0 0 24 24"
  fill="none"
  stroke="currentColor"
  stroke-width={stroke}
  stroke-linecap="round"
  stroke-linejoin="round"
  aria-hidden="true"
  focusable="false"
>
  <!-- eslint-disable-next-line svelte/no-at-html-tags -- static, trusted icon markup from icons.ts -->
  {@html ICONS[name]}
</svg>
