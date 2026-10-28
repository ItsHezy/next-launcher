<script lang="ts">
  import { num } from '../boot';
  import { library } from '../state/library.svelte';
  import { router } from '../state/router.svelte';
  import { comingSoon } from '../state/ui.svelte';
  import Cover from './Cover.svelte';
  import DownloadCard from './DownloadCard.svelte';
  import Icon from './Icon.svelte';

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
        <li>
          <button
            class="ql-item"
            onclick={() =>
              g.state === 'notInstalled' ? comingSoon('Installing', 4) : comingSoon('Launching games', 5)}
          >
            <span class="thumb"><Cover game={g} kind="thumb" /></span>
            <span class="txt">
              <span class="name">{g.title}</span>
              {#if g.state === 'installed'}
                <span class="st status-ready">Ready</span>
              {:else if g.state === 'updateAvailable'}
                <span class="st status-update">Update</span>
              {:else if g.state === 'installing' && library.download?.appName === g.appName}
                <span class="st status-active">Installing {dlPct}%</span>
              {:else}
                <span class="st muted">Not installed</span>
              {/if}
            </span>
          </button>
        </li>
      {/each}
    </ul>
  {/if}

  <div class="flex"></div>

  {#if library.download}
    <DownloadCard dl={library.download} />
  {/if}

  <button class="settings-link" onclick={() => router.go('settings', 'general')}>
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
