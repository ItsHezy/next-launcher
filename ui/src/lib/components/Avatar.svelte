<script lang="ts">
  import { initials } from '../format';

  let { name, size = 36 }: { name: string; size?: number } = $props();
</script>

<span class="av" style:width="{size}px" style:height="{size}px" style:font-size="{Math.round(size * 0.4)}px" aria-hidden="true">
  {initials(name)}
</span>

<style>
  .av {
    display: inline-grid;
    place-items: center;
    flex: none;
    border-radius: 50%;
    background: var(--accent-fill);
    color: #fff;
    font-weight: 600;
    letter-spacing: 0.02em;
  }
</style>
