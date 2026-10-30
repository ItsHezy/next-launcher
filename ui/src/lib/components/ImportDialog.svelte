<script lang="ts">
  import { commands, type ImportCandidate } from '../bindings';
  import { bytes } from '../format';
  import { library } from '../state/library.svelte';
  import { ui } from '../state/ui.svelte';
  import Icon from './Icon.svelte';

  let items = $state<ImportCandidate[] | null>(null);
  let error = $state<string | null>(null);
  let confirming = $state<string | null>(null);
  let busy = $state(false);
  let dialog = $state<HTMLDivElement>();

  async function load() {
    error = null;
    const res = await commands.eglImportScan();
    if (res.status === 'ok') items = res.data;
    else error = res.error.message;
  }

  async function adopt(c: ImportCandidate) {
    busy = true;
    const res = await commands.eglAdopt(c.appName);
    busy = false;
    confirming = null;
    if (res.status === 'error') {
      ui.toast(res.error.message);
      return;
    }
    ui.toast(
      c.needsVerify
        ? `${c.title} added. It will be checked before it first launches.`
        : `${c.title} added to your library as installed.`,
    );
    await library.reload();
    await load();
  }

  function close() {
    ui.dialog = null;
  }

  $effect(() => {
    void load();
    dialog?.focus();
  });
</script>

<svelte:window onkeydown={(e) => e.key === 'Escape' && !busy && close()} />

<div class="backdrop" role="presentation" onclick={(e) => e.target === e.currentTarget && !busy && close()}>
  <div class="dialog" role="dialog" aria-modal="true" aria-labelledby="imp-h" tabindex="-1" bind:this={dialog}>
    <header>
      <h2 id="imp-h">Use games from the Epic Games Launcher</h2>
      <button class="icon-btn" aria-label="Close" onclick={close}><Icon name="close" size={18} /></button>
    </header>
    <p class="lead">
      Games the Epic Games Launcher already installed can be used here without downloading them again.
      Next Launcher only reads these folders. Nothing in them, or in the Epic Games Launcher, changes.
      Complete games you own are added automatically after each library refresh (Settings → Downloads &amp;
      installs).
    </p>

    {#if error}
      <p class="msg">{error}</p>
    {:else if items === null}
      <p class="msg">Looking for installed games…</p>
    {:else if items.length === 0}
      <p class="msg">The Epic Games Launcher has no installed games on this PC.</p>
    {:else}
      <ul class="list">
        {#each items as c (c.appName)}
          <li class="row">
            <div class="txt">
              <div class="name">{c.title}</div>
              <div class="meta">{c.folder} · {bytes(c.size ?? 0)} · version {c.version}</div>
              {#if c.needsVerify && c.adoptable && c.owned && !c.alreadyAdded}
                <div class="note">An update was interrupted. The files will be checked before first launch.</div>
              {/if}
              {#if confirming === c.appName}
                <div class="confirm">
                  Next Launcher will use the files in <strong>{c.folder}</strong> as they are.
                  Nothing in that folder changes now, and the Epic Games Launcher keeps working with it.
                </div>
              {/if}
            </div>
            <div class="act">
              {#if c.alreadyAdded}
                <span class="tag ok"><Icon name="check" size={16} /> Added</span>
              {:else if !c.owned}
                <span class="tag">Not in your library</span>
              {:else if !c.adoptable}
                <span class="tag">Incomplete install</span>
              {:else if confirming === c.appName}
                <button class="btn btn-quiet" disabled={busy} onclick={() => (confirming = null)}>Cancel</button>
                <button class="btn btn-primary" disabled={busy} onclick={() => adopt(c)}>
                  {busy ? 'Adding…' : 'Use this install'}
                </button>
              {:else}
                <button class="btn btn-quiet" onclick={() => (confirming = c.appName)}>Use this install…</button>
              {/if}
            </div>
          </li>
        {/each}
      </ul>
    {/if}
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 150;
    display: grid;
    place-items: center;
    background: rgba(0, 0, 0, 0.5);
    animation: fade var(--dur) var(--ease-out);
  }
  .dialog {
    width: min(720px, calc(100vw - 48px));
    max-height: calc(100vh - 96px);
    overflow-y: auto;
    padding: 20px 24px 24px;
    border-radius: var(--radius-lg);
    background: var(--surface);
    border: 1px solid var(--border);
    box-shadow: 0 24px 80px rgba(0, 0, 0, 0.5);
    outline: none;
    animation: pop var(--dur) var(--ease-spring);
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  .lead,
  .msg {
    color: var(--text-2);
    margin: 8px 0 16px;
  }
  .list {
    list-style: none;
    margin: 0;
    padding: 0;
    border-radius: var(--radius-md);
    background: var(--surface-inset);
  }
  .row {
    display: flex;
    gap: 16px;
    align-items: center;
    padding: 14px 16px;
  }
  .row + .row {
    border-top: 1px solid var(--border);
  }
  .txt {
    flex: 1;
    min-width: 0;
  }
  .name {
    font-weight: 600;
  }
  .meta {
    font-size: 12.5px;
    color: var(--text-3);
    overflow-wrap: anywhere;
  }
  .note {
    font-size: 12.5px;
    color: var(--warning);
    margin-top: 4px;
  }
  .confirm {
    margin-top: 10px;
    padding: 10px 12px;
    border-radius: var(--radius-sm);
    background: var(--control);
    font-size: 13px;
  }
  .act {
    display: flex;
    gap: 8px;
    flex: none;
  }
  .tag {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font-size: 13px;
    color: var(--text-2);
  }
  .tag.ok {
    color: var(--success);
  }
  @keyframes fade {
    from {
      opacity: 0;
    }
  }
  @keyframes pop {
    from {
      opacity: 0;
      transform: scale(0.97);
    }
  }
</style>
