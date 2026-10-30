<script lang="ts">
  import { commands } from '../bindings';
  import { library } from '../state/library.svelte';
  import { ui } from '../state/ui.svelte';
  import Modal from './Modal.svelte';

  let { version, notes }: { version: string; notes: string | null } = $props();

  let busy = $state(false);
  let error = $state<string | null>(null);
  const playing = $derived(library.running.size > 0);

  async function install() {
    busy = true;
    error = null;
    // On success the installer takes over and Next Launcher restarts; this only returns on failure.
    const res = await commands.installAppUpdate();
    busy = false;
    if (res.status === 'error') error = res.error.message;
  }
</script>

<Modal title="Update Next Launcher" onclose={() => (ui.dialog = null)} {busy} width={500}>
  <p class="msg">
    Version {version} is ready. Next Launcher closes, installs it and opens again. Your games stay as they are,
    and downloads pick up where they left off.
  </p>
  {#if notes}<p class="notes">{notes}</p>{/if}
  {#if playing}<p class="msg">Close your game first.</p>{/if}
  {#if error}<p class="msg err">{error}</p>{/if}
  <div class="actions">
    <button class="btn btn-quiet" disabled={busy} onclick={() => (ui.dialog = null)}>Later</button>
    <button class="btn btn-primary" disabled={busy || playing} onclick={install}>
      {busy ? 'Downloading…' : 'Install and restart'}
    </button>
  </div>
</Modal>

<style>
  .msg {
    margin: 8px 0 12px;
    color: var(--text-2);
  }
  .err {
    color: var(--danger);
  }
  .notes {
    max-height: 200px;
    overflow-y: auto;
    margin: 0 0 12px;
    padding: 10px 14px;
    border-radius: var(--radius-md);
    background: var(--surface-inset);
    white-space: pre-wrap;
    font-size: 13px;
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 16px;
  }
</style>
