<script lang="ts">
  import { boot } from '../boot';
  import type { Game } from '../state/library.svelte';

  type Kind = 'tall' | 'wide' | 'thumb';
  /** `label`: draw the title on the placeholder (off when the parent overlays its own title). */
  let { game, kind = 'tall', label = true }: { game: Game; kind?: Kind; label?: boolean } = $props();

  const has = $derived(kind === 'wide' ? game.hasWide : game.hasCover);
  const src = $derived(`${boot.imgBase}/${encodeURIComponent(game.appName)}/${kind === 'thumb' ? 'tall' : kind}`);
  const h = $derived(game.hue);
  const bg = $derived(
    `linear-gradient(${kind === 'wide' ? 120 : 160}deg, hsl(${h} 42% 38%), hsl(${(h + 48) % 360} 48% 17%))`,
  );
</script>

<div class="cover {kind}" style:background={has ? undefined : bg}>
  {#if has}
    <img {src} alt="" loading="lazy" decoding="async" draggable="false" />
  {:else if kind !== 'thumb' && label}
    <span class="ph" aria-hidden="true">{game.title}</span>
  {/if}
</div>

<style>
  .cover {
    position: relative;
    overflow: hidden;
    background: var(--surface);
    border-radius: inherit;
    contain: strict;
  }
  .tall {
    aspect-ratio: 3 / 4;
  }
  .wide {
    aspect-ratio: 16 / 9;
  }
  .thumb {
    aspect-ratio: 1;
  }
  img {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    object-fit: cover;
  }
  .ph {
    position: absolute;
    inset: auto 12px 12px 12px;
    font-weight: 700;
    font-size: 17px;
    line-height: 1.15;
    letter-spacing: -0.01em;
    color: rgba(255, 255, 255, 0.92);
    text-shadow: 0 1px 12px rgba(0, 0, 0, 0.35);
    display: -webkit-box;
    -webkit-line-clamp: 3;
    line-clamp: 3;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }
  .wide .ph {
    font-size: 22px;
  }
</style>
