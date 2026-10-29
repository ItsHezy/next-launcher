<script lang="ts">
  import { commands } from '../bindings';
  import { library } from '../state/library.svelte';
  import { ui } from '../state/ui.svelte';
  import Modal from './Modal.svelte';

  let { appName }: { appName: string } = $props();

  const title = $derived(library.byId.get(appName)?.title ?? 'this game');
  let busy = $state(false);
  let error = $state<string | null>(null);
  /** Set when the folder is shared with the official launcher: a second, explicit confirmation. */
  let external = $state<string | null>(null);

  async function uninstall() {
    busy = true;
    error = null;
    const res = await commands.uninstallGame(appName, { confirmExternal: external !== null });
    busy = false;
    if (res.status === 'ok') {
      ui.dialog = null;
      ui.toast(`${title} was uninstalled.`);
      return;
    }
    if (res.error.code === 'confirmExternal' && external === null) {
      external = res.error.message;
      return;
    }
    error = res.error.message;
  }
</script>

<Modal title="Uninstall {title}?" onclose={() => (ui.dialog = null)} {busy} width={480}>
  <p class="msg">
    This deletes the game's files from your PC. Saves stored in the cloud stay in your Epic account; files the game
    created outside its install folder are not touched.
  </p>
  {#if external}
    <p class="msg warn">{external}</p>
  {/if}
  {#if error}<p class="msg err">{error}</p>{/if}
  <div class="actions">
    <button class="btn btn-quiet" disabled={busy} onclick={() => (ui.dialog = null)}>Keep it</button>
    <button class="btn btn-danger" disabled={busy} onclick={uninstall}>
      {busy ? 'Uninstalling…' : external ? 'Delete the files' : 'Uninstall'}
    </button>
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
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 16px;
  }
</style>
