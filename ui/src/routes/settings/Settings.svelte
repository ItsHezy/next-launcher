<script lang="ts">
  import Icon from '../../lib/components/Icon.svelte';
  import WindowControls from '../../lib/components/WindowControls.svelte';
  import { account } from '../../lib/state/account.svelte';
  import { auth } from '../../lib/state/auth.svelte';
  import { router } from '../../lib/state/router.svelte';
  import AboutDiagnostics from './AboutDiagnostics.svelte';
  import AccountCard from './AccountCard.svelte';
  import CloudGames from './CloudGames.svelte';
  import ManageGames from './ManageGames.svelte';
  import SettingRow from './SettingRow.svelte';
  import { ALL_SECTIONS, SECTIONS, type Row, type Section } from './schema';

  let query = $state('');
  let content = $state<HTMLElement>();

  const current = $derived(ALL_SECTIONS.find((s) => s.id === router.section) ?? ALL_SECTIONS[0]!);

  // Live values for the section on screen (Rust caches them for a few minutes).
  $effect(() => {
    const id = current.id;
    if (id === 'account' && auth.signedIn) void account.loadDetails();
    else if (id === 'wallet' && auth.signedIn) void account.loadWallet();
    else if (id === 'general') void account.loadHidden();
  });

  const results = $derived.by(() => {
    const q = query.trim().toLowerCase();
    if (!q) return [];
    const out: { section: Section; rows: Row[] }[] = [];
    for (const s of ALL_SECTIONS) {
      const sectionHit = s.label.toLowerCase().includes(q);
      const rows = s.groups
        .flatMap((g) => g.rows)
        .filter((r) => sectionHit || r.label.toLowerCase().includes(q) || (r.desc ?? '').toLowerCase().includes(q));
      if (rows.length || (sectionHit && s.custom)) out.push({ section: s, rows });
    }
    return out;
  });

  function select(id: Section['id']) {
    query = '';
    router.go('settings', id);
    content?.scrollTo({ top: 0 });
  }

  function onkey(e: KeyboardEvent) {
    if (e.key === 'Escape' && !query && router.route === 'settings') router.go('library');
  }
</script>

<svelte:window onkeydown={onkey} />

<div class="settings">
  <nav class="side panel" aria-label="Settings sections">
    <div class="drag" data-tauri-drag-region></div>
    <button class="back" onclick={() => router.go('library')}>
      <Icon name="chevronLeft" size={20} /> Library
    </button>
    <h1 class="title">Settings</h1>
    <div class="search">
      <Icon name="search" size={18} />
      <input type="search" placeholder="Search settings" aria-label="Search settings" bind:value={query} />
    </div>

    <div class="sections">
      {#each SECTIONS as group, gi (gi)}
        <ul class="group">
          {#each group as s (s.id)}
            <li>
              <button
                class="sec"
                aria-current={!query && router.section === s.id ? 'page' : undefined}
                onclick={() => select(s.id)}
              >
                <span class="tile" style:background={s.tile}><Icon name={s.icon} size={17} stroke={2} /></span>
                <span>{s.label}</span>
              </button>
            </li>
          {/each}
        </ul>
      {/each}
    </div>
  </nav>

  <main class="main panel">
    <div class="bar" data-tauri-drag-region>
      <h2 class="h" data-tauri-drag-region>{query ? 'Search results' : current.label}</h2>
      <WindowControls />
    </div>

    <div class="content" bind:this={content}>
      <div class="inner">
        {#if query}
          {#if results.length === 0}
            <p class="empty">No settings match “{query}”.</p>
          {/if}
          {#each results as r (r.section.id)}
            <button class="result-head" onclick={() => select(r.section.id)}>
              <span class="tile sm" style:background={r.section.tile}><Icon name={r.section.icon} size={14} stroke={2} /></span>
              {r.section.label}
              <Icon name="chevronRight" size={16} />
            </button>
            {#if r.rows.length}
              <div class="card">
                {#each r.rows as row, i (i)}<SettingRow {row} />{/each}
              </div>
            {/if}
          {/each}
        {:else}
          {#if current.custom === 'account'}<AccountCard />{/if}
          {#if current.custom === 'games'}<ManageGames />{/if}
          {#each current.groups as g, gi (`${current.id}-${gi}`)}
            {#if g.title}<h3 class="gt">{g.title}</h3>{/if}
            <div class="card">
              {#each g.rows as row, i (i)}<SettingRow {row} />{/each}
            </div>
            {#if g.footer}<p class="foot">{g.footer}</p>{/if}
          {/each}
          {#if current.custom === 'cloudGames'}<CloudGames />{/if}
          {#if current.custom === 'about'}<AboutDiagnostics />{/if}
        {/if}
      </div>
    </div>
  </main>
</div>

<style>
  .settings {
    display: flex;
    gap: var(--gap);
    height: 100%;
    padding: var(--gap);
  }
  .side {
    width: 300px;
    flex: none;
    display: flex;
    flex-direction: column;
    padding: 0 12px 12px;
    min-height: 0;
  }
  .drag {
    height: 14px;
    flex: none;
  }
  .back {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    align-self: flex-start;
    padding: 0 12px 0 6px;
    border-radius: var(--radius-sm);
    color: var(--accent-text);
    font-weight: 500;
  }
  .back:hover {
    background: var(--hover);
  }
  .title {
    padding: 6px 10px 12px;
  }
  .search {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 0 4px 12px;
    padding-left: 12px;
    border-radius: var(--radius-sm);
    background: var(--control);
    color: var(--text-2);
  }
  .search input {
    flex: 1;
    border: 0;
    background: transparent;
    padding-left: 0;
  }
  .sections {
    overflow-y: auto;
    min-height: 0;
    display: flex;
    flex-direction: column;
    gap: 14px;
    padding: 2px;
  }
  .group {
    list-style: none;
    margin: 0;
    padding: 4px;
    border-radius: var(--radius-md);
    background: var(--surface-inset);
  }
  .sec {
    display: flex;
    align-items: center;
    gap: 12px;
    width: 100%;
    padding: 0 10px;
    border-radius: var(--radius-sm);
    text-align: left;
    font-weight: 500;
  }
  .sec:hover {
    background: var(--hover);
  }
  .sec[aria-current='page'] {
    background: var(--selected-strong);
  }
  .tile {
    display: grid;
    place-items: center;
    width: 30px;
    height: 30px;
    border-radius: 9px;
    color: #fff;
    flex: none;
  }
  .tile.sm {
    width: 24px;
    height: 24px;
    border-radius: 7px;
  }
  .main {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }
  .bar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    min-height: 64px;
    padding: 8px 8px 8px 32px;
  }
  .h {
    font-size: 22px;
    font-weight: 700;
  }
  .content {
    flex: 1;
    overflow-y: auto;
    scrollbar-gutter: stable;
  }
  .inner {
    max-width: 760px;
    padding: 4px 32px 40px;
  }
  .gt {
    margin: 22px 4px 8px;
    font-size: 13px;
    font-weight: 600;
    color: var(--text-2);
  }
  .card {
    border-radius: var(--radius-md);
    background: var(--surface-inset);
    overflow: hidden;
  }
  .card > :global(* + *) {
    border-top: 1px solid var(--border);
  }
  .foot {
    margin: 10px 4px 0;
    font-size: 12.5px;
    color: var(--text-2);
  }
  .result-head {
    display: flex;
    align-items: center;
    gap: 10px;
    margin: 18px 0 8px;
    padding: 0 6px;
    border-radius: var(--radius-sm);
    font-weight: 600;
    color: var(--text-2);
  }
  .result-head:hover {
    color: var(--text);
    background: var(--hover);
  }
  .empty {
    color: var(--text-2);
    padding: 20px 4px;
  }
</style>
