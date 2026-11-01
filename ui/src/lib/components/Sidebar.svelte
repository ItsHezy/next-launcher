<script lang="ts">
  import { num } from '../boot';
  import { library, type Game } from '../state/library.svelte';
  import * as actions from '../gameActions';
  import { bump, shift, slide } from '../motion';
  import { router } from '../state/router.svelte';
  import Cover from './Cover.svelte';
  import DownloadCard from './DownloadCard.svelte';
  import Icon from './Icon.svelte';

  /** The line under a Quick Launch game; it bumps when its kind changes. */
  function status(g: Game) {
    if (library.running.has(g.appName)) return { kind: 'playing', text: 'Playing', cls: 'status-active' };
    if (library.starting.has(g.appName)) return { kind: 'starting', text: 'Starting…', cls: 'status-active' };
    if (g.state === 'installed') return { kind: 'ready', text: 'Ready', cls: 'status-ready' };
    if (g.state === 'updateAvailable') return { kind: 'update', text: 'Update', cls: 'status-update' };
    if (g.state === 'installing' && library.download?.appName === g.appName)
      return { kind: 'installing', text: `Installing ${dlPct}%`, cls: 'status-active' };
    return { kind: 'none', text: 'Not installed', cls: 'muted' };
  }

  const dlPct = $derived(
    library.download
      ? Math.floor((num(library.download.bytesDone) / Math.max(1, num(library.download.bytesTotal))) * 100)
      : 0,
  );
</script>

<aside class="sidebar panel" aria-label="Quick Launch and downloads">
  <div class="drag" data-tauri-drag-region></div>
  <h2 class="heading" id="ql-heading">Quick Launch</h2>

  {#if library.pinned.length === 0}
    <p class="empty">Pin games from your library to start them from here.</p>
  {:else}
    <ul class="ql" aria-labelledby="ql-heading">
      {#each library.pinned as g (g.appName)}
        <li in:slide|global={{ x: -12, y: 0 }} out:slide={{ x: -12, y: 0 }} animate:shift>
          <button
            class="ql-item"
            onclick={() => void actions.play(g)}
          >
            <span class="thumb"><Cover game={g} kind="thumb" /></span>
            <span class="txt">
              <span class="name">{g.title}</span>
              <span class="st {status(g).cls}" use:bump={status(g).kind}>
                {#if status(g).kind === 'playing'}<span class="live" aria-hidden="true"></span>{/if}{status(g).text}
              </span>
            </span>
          </button>
        </li>
      {/each}
    </ul>
  {/if}

  <div class="flex"></div>

  {#if library.download}
    <div transition:slide>
      <DownloadCard dl={library.download} />
    </div>
  {/if}

  <button class="settings-link turn" onclick={() => router.go('settings', 'general')}>
    <Icon name="gear" size={20} />
    <span>Settings</span>
  </button>
</aside>

<style>
  .sidebar {
    display: flex;
    flex-direction: column;
    width: var(--sidebar-w);
    flex: none;
    padding-bottom: 12px;
    min-height: 0;
  }
  .drag {
    height: 14px;
    flex: none;
  }
  .heading {
    padding: 4px 22px 10px;
    font-size: 13px;
    font-weight: 600;
    letter-spacing: 0.02em;
    color: var(--text-2);
  }
  .empty {
    padding: 0 22px;
    color: var(--text-2);
    font-size: 13px;
  }
  .ql {
    list-style: none;
    margin: 0;
    padding: 0 10px;
    overflow-y: auto;
    min-height: 0;
  }
  .ql-item {
    display: flex;
    align-items: center;
    gap: 12px;
    width: 100%;
    min-height: 56px;
    padding: 6px 10px;
    border-radius: var(--radius-sm);
    text-align: left;
    transition: background-color var(--dur-fast) var(--ease-out);
  }
  .ql-item:hover {
    background: var(--hover);
  }
  .thumb {
    width: 40px;
    height: 40px;
    flex: none;
    border-radius: 10px;
    overflow: hidden;
    transition: transform var(--dur) var(--ease-spring);
  }
  .ql-item:hover .thumb {
    transform: scale(1.08);
  }
  .ql-item:active .thumb {
    transform: scale(0.94);
  }
  .txt {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .name {
    font-weight: 500;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .st {
    font-size: 12.5px;
    transform-origin: left center;
  }
  .muted {
    color: var(--text-2);
  }
  .flex {
    flex: 1;
    min-height: 12px;
  }
  .settings-link {
    display: flex;
    align-items: center;
    gap: 12px;
    margin: 12px 10px 0;
    padding: 0 12px;
    border-radius: var(--radius-sm);
    color: var(--text-2);
    font-weight: 500;
  }
  .settings-link:hover {
    background: var(--hover);
    color: var(--text);
  }
</style>
