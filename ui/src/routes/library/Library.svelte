<script lang="ts">
  import { dismiss, menuKeys } from '../../lib/actions';
  import AuthBanner from '../../lib/components/AuthBanner.svelte';
  import Icon from '../../lib/components/Icon.svelte';
  import * as actions from '../../lib/gameActions';
  import { ago } from '../../lib/format';
  import { auth } from '../../lib/state/auth.svelte';
  import Segmented from '../../lib/components/Segmented.svelte';
  import { library, type Filter, type Game, type Sort } from '../../lib/state/library.svelte';
  import { settings } from '../../lib/state/settings.svelte';
  import { comingSoon, ui } from '../../lib/state/ui.svelte';
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
    if (g.state === 'notInstalled' && g.thirdParty) comingSoon(`Opening ${g.thirdParty}`, 5);
    else if (g.state === 'notInstalled') actions.install(g.appName);
    else comingSoon('Launching games', 5);
  }

  async function refresh() {
    const err = await library.refresh();
    if (err) ui.toast(err);
  }

  const syncLine = $derived(
    library.sync.syncing
      ? 'Syncing…'
      : library.sync.error
        ? library.sync.error
        : library.sync.lastSync
          ? `Synced ${ago(library.sync.lastSync)}`
          : '',
  );

  function openMenu(game: Game, x: number, y: number) {
    // Keyboard context-menu key reports 0,0: anchor to the focused element.
    // Clamp so the tallest menu (5 items) stays on screen.
    if (x === 0 && y === 0 && document.activeElement instanceof HTMLElement) {
      const r = document.activeElement.getBoundingClientRect();
      x = r.left;
      y = r.bottom;
    }
    menu = { game, x: Math.min(x, window.innerWidth - 240), y: Math.min(y, window.innerHeight - 240) };
  }
</script>

<div class="page" bind:this={scroller} data-scroll="library">
  <AuthBanner />
  <header class="head">
    <div class="title">
      <h1>Library</h1>
      <span class="count">{library.counts.all} {library.counts.all === 1 ? 'game' : 'games'}</span>
    </div>
    <div class="tools">
      {#if auth.signedIn}
        <span class="sync" class:err={!!library.sync.error && !library.sync.syncing} aria-live="polite">{syncLine}</span>
        <button
          class="icon-btn"
          class:spin={library.sync.syncing}
          aria-label="Refresh library"
          disabled={library.sync.syncing}
          onclick={refresh}
        >
          <Icon name="update" size={18} />
        </button>
      {/if}
    <Segmented
      label="Filter games"
      size="sm"
      options={filters}
      value={library.filter}
      onchange={(v) => (library.filter = v)}
    />
    </div>
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
        {library.query
          ? 'No games match your search.'
          : library.filter === 'updates'
            ? 'Everything is up to date.'
            : library.sync.syncing
              ? 'Getting your library from Epic…'
              : auth.hasAccount
                ? 'No games here yet.'
                : 'Sign in to see your games.'}
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
    {#if g.state === 'updateAvailable'}
      <button role="menuitem" onclick={() => ((menu = null), actions.update(g.appName, g.title))}>Update now</button>
    {/if}
    {#if g.state === 'installed' || g.state === 'updateAvailable'}
      <button role="menuitem" onclick={() => ((menu = null), actions.verifyAndRepair(g.appName, g.title))}>Verify files</button>
      <button role="menuitem" onclick={() => ((menu = null), actions.move(g.appName))}>Move…</button>
      <button role="menuitem" class="danger" onclick={() => ((menu = null), actions.uninstall(g.appName))}>Uninstall…</button>
    {:else if g.state === 'notInstalled' && g.installable}
      <button role="menuitem" onclick={() => ((menu = null), actions.install(g.appName))}>Install…</button>
    {/if}
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
  .tools {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .sync {
    font-size: 13px;
    color: var(--text-3);
    max-width: 320px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .sync.err {
    color: var(--warning);
  }
  .spin :global(svg) {
    animation: spin 0.9s linear infinite;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
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
  .ctx .danger {
    color: var(--danger);
  }
  .ctx button:hover,
  .ctx button:focus-visible {
    background: var(--hover);
    outline-offset: -2px;
  }
</style>
