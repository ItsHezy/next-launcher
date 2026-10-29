<script lang="ts">
  import { dismiss, menuKeys } from '../../lib/actions';
  import AuthBanner from '../../lib/components/AuthBanner.svelte';
  import Segmented from '../../lib/components/Segmented.svelte';
  import { library, type Filter, type Game, type Sort } from '../../lib/state/library.svelte';
  import { settings } from '../../lib/state/settings.svelte';
  import { comingSoon } from '../../lib/state/ui.svelte';
  import GameGrid from './GameGrid.svelte';
  import RecentCard from './RecentCard.svelte';

  let scroller = $state<HTMLElement>();
  let menu = $state<{ game: Game; x: number; y: number } | null>(null);

  const sort = $derived(settings.get('general.defaultSort') as Sort);
  const games = $derived(library.list(sort, settings.get('general.showUninstalled')));
  const showRecent = $derived(library.filter === 'all' && !library.query && library.recentGames.length > 0);

  const filters = $derived<{ value: Filter; label: string; count: number }[]>([
    { value: 'all', label: 'All', count: library.counts.all },
    { value: 'installed', label: 'Installed', count: library.counts.installed },
    { value: 'updates', label: 'Updates', count: library.counts.updates },
  ]);

  function play(g: Game) {
    if (g.state === 'notInstalled') comingSoon('Installing games', 4);
    else comingSoon('Launching games', 5);
  }

  function openMenu(game: Game, x: number, y: number) {
    // Keyboard context-menu key reports 0,0: anchor to the focused element.
    if (x === 0 && y === 0 && document.activeElement instanceof HTMLElement) {
      const r = document.activeElement.getBoundingClientRect();
      x = r.left;
      y = r.bottom;
    }
    menu = { game, x: Math.min(x, window.innerWidth - 240), y: Math.min(y, window.innerHeight - 120) };
  }
</script>

<div class="page" bind:this={scroller} data-scroll="library">
  <AuthBanner />
  <header class="head">
    <div class="title">
      <h1>Library</h1>
      <span class="count">{library.counts.all} {library.counts.all === 1 ? 'game' : 'games'}</span>
    </div>
    <Segmented
      label="Filter games"
      size="sm"
      options={filters}
      value={library.filter}
      onchange={(v) => (library.filter = v)}
    />
  </header>

  {#if showRecent}
    <section aria-labelledby="recent-h" class="recent-section">
      <h2 id="recent-h">Recently played</h2>
      <div class="recent">
        {#each library.recentGames as g (g.appName)}
          <RecentCard game={g} onplay={play} />
        {/each}
      </div>
    </section>
  {/if}

  <section aria-labelledby="all-h">
    <h2 id="all-h">
      {#if library.query}Results for “{library.query}”{:else if library.filter === 'installed'}Installed{:else if library.filter === 'updates'}Updates available{:else}All games{/if}
      <span class="sub">{games.length}</span>
    </h2>
    {#if games.length === 0}
      <p class="empty">
        {library.query ? 'No games match your search.' : library.filter === 'updates' ? 'Everything is up to date.' : 'No games here yet.'}
      </p>
    {:else}
      <GameGrid {games} {scroller} onplay={play} onmenu={openMenu} />
    {/if}
  </section>
</div>

{#if menu}
  {@const g = menu.game}
  <div
    class="menu ctx"
    role="menu"
    aria-label="{g.title} options"
    style:left="{menu.x}px"
    style:top="{menu.y}px"
    use:dismiss={{ onclose: () => (menu = null) }}
    use:menuKeys
  >
    <button
      role="menuitem"
      onclick={() => {
        library.setPinned(g.appName, !g.pinned);
        menu = null;
      }}
    >
      {g.pinned ? 'Remove from Quick Launch' : 'Add to Quick Launch'}
    </button>
    <button role="menuitem" onclick={() => ((menu = null), comingSoon('Game management', 4))}>Manage…</button>
  </div>
{/if}

<style>
  .page {
    height: 100%;
    overflow-y: auto;
    padding: 28px 32px 48px;
    scrollbar-gutter: stable;
  }
  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    margin-bottom: 28px;
  }
  .title {
    display: flex;
    align-items: baseline;
    gap: 12px;
  }
  .count {
    color: var(--text-2);
    font-variant-numeric: tabular-nums;
  }
  h2 {
    display: flex;
    align-items: baseline;
    gap: 10px;
    margin-bottom: 16px;
  }
  .sub {
    font-size: 13px;
    font-weight: 500;
    color: var(--text-3);
    font-variant-numeric: tabular-nums;
  }
  .recent-section {
    margin-bottom: 36px;
  }
  .recent {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(260px, 1fr));
    grid-template-rows: auto;
    grid-auto-rows: 0;
    overflow: hidden;
    gap: 0 20px;
  }
  .empty {
    color: var(--text-2);
    padding: 24px 0;
  }
  .ctx {
    position: fixed;
    width: 240px;
    transform-origin: top left;
  }
  .ctx button {
    display: flex;
    align-items: center;
    width: 100%;
    padding: 0 12px;
    border-radius: var(--radius-sm);
    text-align: left;
  }
  .ctx button:hover,
  .ctx button:focus-visible {
    background: var(--hover);
    outline-offset: -2px;
  }
</style>
