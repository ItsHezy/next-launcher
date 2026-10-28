<script lang="ts">
  // Virtualised 3:4 grid: only visible rows (+ overscan) are in the DOM.
  import type { Game } from '../../lib/state/library.svelte';
  import GameTile from './GameTile.svelte';

  let {
    games,
    scroller,
    onplay,
    onmenu,
  }: {
    games: Game[];
    scroller: HTMLElement | undefined;
    onplay: (g: Game) => void;
    onmenu: (g: Game, x: number, y: number) => void;
  } = $props();

  const MIN_TILE = 168;
  const GAP = 20;
  const ROW_GAP = 28;
  const META = 54; // tile text area under the cover (gap + 44px row)
  const OVERSCAN = 2;

  let el = $state<HTMLDivElement>();
  let width = $state(0);
  let top = $state(0); // grid's offset inside the scroller
  let scrollTop = $state(0);
  let viewport = $state(800);

  const cols = $derived(Math.max(1, Math.floor((width + GAP) / (MIN_TILE + GAP))));
  const tileW = $derived(width > 0 ? (width - GAP * (cols - 1)) / cols : MIN_TILE);
  const rowH = $derived(Math.round((tileW * 4) / 3 + META + ROW_GAP));
  const rows = $derived(Math.ceil(games.length / cols));
  const height = $derived(Math.max(0, rows * rowH - ROW_GAP));
  const first = $derived(Math.max(0, Math.floor((scrollTop - top) / rowH) - OVERSCAN));
  const last = $derived(Math.min(rows, Math.ceil((scrollTop - top + viewport) / rowH) + OVERSCAN));
  const visibleRows = $derived(Array.from({ length: Math.max(0, last - first) }, (_, i) => first + i));

  function measure() {
    if (!el || !scroller) return;
    width = el.clientWidth;
    viewport = scroller.clientHeight;
    top = el.getBoundingClientRect().top - scroller.getBoundingClientRect().top + scroller.scrollTop;
    scrollTop = scroller.scrollTop;
  }

  $effect(() => {
    const s = scroller;
    if (!el || !s) return;
    measure();
    let raf = 0;
    const onScroll = () => {
      if (raf) return;
      raf = requestAnimationFrame(() => {
        raf = 0;
        scrollTop = s.scrollTop;
      });
    };
    const ro = new ResizeObserver(measure);
    ro.observe(el);
    ro.observe(s);
    s.addEventListener('scroll', onScroll, { passive: true });
    return () => {
      ro.disconnect();
      s.removeEventListener('scroll', onScroll);
      if (raf) cancelAnimationFrame(raf);
    };
  });

  // Content above the grid (e.g. the recent row) can change height.
  $effect(() => {
    void games.length;
    queueMicrotask(measure);
  });
</script>

<div class="grid" bind:this={el} style:height="{height}px" role="list" aria-label="All games ({games.length})">
  {#each visibleRows as r (r)}
    <div
      class="row"
      style:transform="translateY({r * rowH}px)"
      style:grid-template-columns="repeat({cols}, minmax(0, 1fr))"
      style:column-gap="{GAP}px"
    >
      {#each games.slice(r * cols, r * cols + cols) as g (g.appName)}
        <div role="listitem"><GameTile game={g} {onplay} {onmenu} /></div>
      {/each}
    </div>
  {/each}
</div>

<style>
  .grid {
    position: relative;
    contain: layout size;
  }
  .row {
    position: absolute;
    left: 0;
    right: 0;
    top: 0;
    display: grid;
    will-change: transform;
  }
</style>
