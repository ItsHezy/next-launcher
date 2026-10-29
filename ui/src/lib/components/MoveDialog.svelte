<script lang="ts">
  import { commands, events, type MovePreview } from '../bindings';
  import { num } from '../boot';
  import { bytes } from '../format';
  import { library } from '../state/library.svelte';
  import { ui } from '../state/ui.svelte';
  import Modal from './Modal.svelte';

  let { appName }: { appName: string } = $props();

  const title = $derived(library.byId.get(appName)?.title ?? 'this game');
  let preview = $state<MovePreview | null>(null);
  let folder = $state('');
  let busy = $state(false);
  let error = $state<string | null>(null);
  /** Set when the folder is shared with the official launcher: a second, explicit confirmation. */
  let external = $state<string | null>(null);
  let done = $state(0);
  let total = $state(0);

  const pct = $derived(total > 0 ? Math.min(100, Math.floor((done / total) * 100)) : 0);
  const unchanged = $derived(!preview || folder.trim().toLowerCase() === preview.currentPath.toLowerCase());

  async function load() {
    const res = await commands.prepareMove(appName);
    if (res.status === 'error') {
      error = res.error.message;
      return;
    }
    preview = res.data;
    folder = res.data.currentPath;
  }

  async function move() {
    busy = true;
    error = null;
    done = 0;
    const stop = await events.moveProgress.listen((e) => {
      if (e.payload.appName !== appName) return;
      done = num(e.payload.bytesDone);
      total = num(e.payload.bytesTotal);
    });
    const res = await commands.moveGame(appName, folder, { confirmExternal: external !== null });
    stop();
    busy = false;
    if (res.status === 'ok') {
      ui.dialog = null;
      ui.toast(
        res.data.leftBehind
          ? `${title} was moved. Some old files were in use; you can delete the old folder later.`
          : `${title} was moved to ${res.data.path}.`,
      );
      return;
    }
    if (res.error.code === 'confirmExternal' && external === null) {
      external = res.error.message;
      return;
    }
    error = res.error.message;
  }

  $effect(() => {
    void load();
  });
</script>

<Modal title="Move {title}" onclose={() => (ui.dialog = null)} {busy} width={520}>
  {#if !preview && !error}
    <p class="msg">Checking the game folder…</p>
  {:else if preview}
    <dl class="facts">
      <div><dt>Current folder</dt><dd class="path">{preview.currentPath}</dd></div>
      <div><dt>Size</dt><dd>{bytes(num(preview.bytes))}</dd></div>
    </dl>
    <label class="field">
      <span>New folder</span>
      <input type="text" bind:value={folder} spellcheck="false" disabled={busy} />
    </label>
    <p class="hint">
      On the same drive this is instant. To another drive, everything is copied first and the old folder is deleted
      only after the copy is complete.
    </p>
    {#if busy}
      <div
        class="bar"
        role="progressbar"
        aria-label="Moving {title}"
        aria-valuemin="0"
        aria-valuemax="100"
        aria-valuenow={pct}
      >
        <span style:width="{pct}%"></span>
      </div>
      <p class="hint">{total > 0 ? `${bytes(done)} of ${bytes(total)}` : 'Moving…'}</p>
    {/if}
  {/if}
  {#if external}<p class="msg warn">{external}</p>{/if}
  {#if error}<p class="msg err">{error}</p>{/if}
  <div class="actions">
    {#if busy}
      <button class="btn btn-quiet" onclick={() => commands.cancelMove(appName)}>Cancel move</button>
    {:else}
      <button class="btn btn-quiet" onclick={() => (ui.dialog = null)}>Close</button>
      {#if preview}
        <button class="btn btn-primary" disabled={unchanged || !folder.trim()} onclick={move}>
          {external ? 'Move it anyway' : 'Move'}
        </button>
      {/if}
    {/if}
  </div>
</Modal>

<style>
  .msg {
    color: var(--text-2);
    margin: 8px 0 12px;
  }
  .warn {
    color: var(--warning);
  }
  .err {
    color: var(--danger);
  }
  .facts {
    display: grid;
    grid-template-columns: 1fr auto;
    gap: 12px 24px;
    margin: 8px 0 18px;
  }
  .facts div {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  dt {
    font-size: 12.5px;
    color: var(--text-2);
  }
  dd {
    margin: 0;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
  }
  .path {
    font-weight: 500;
    font-size: 13px;
    overflow-wrap: anywhere;
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin-bottom: 8px;
  }
  .field span {
    font-size: 13px;
    color: var(--text-2);
  }
  .hint {
    font-size: 12.5px;
    color: var(--text-2);
    margin: 0 0 12px;
  }
  .bar {
    height: 6px;
    margin: 4px 0 8px;
    border-radius: 3px;
    background: rgba(255, 255, 255, 0.08);
    overflow: hidden;
  }
  .bar span {
    display: block;
    height: 100%;
    border-radius: inherit;
    background: var(--accent);
    transition: width var(--dur-slow) var(--ease-out);
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 16px;
  }
</style>
