<script lang="ts">
  import { play } from '../gameActions';
  import { library } from '../state/library.svelte';
  import { ui } from '../state/ui.svelte';
  import Modal from './Modal.svelte';

  let { appName, code, message }: { appName: string; code: string; message: string } = $props();

  const game = $derived(library.byId.get(appName));
  const title = $derived(code === 'prereq' ? 'One more thing first' : 'Play offline?');

  function go(options: { offline: boolean; prereq?: string | null }) {
    const g = game;
    ui.dialog = null;
    if (g) void play(g, options);
  }
</script>

<Modal {title} onclose={() => (ui.dialog = null)} width={460}>
  <p class="msg">{message}</p>
  <div class="actions">
    <button class="btn btn-quiet" onclick={() => (ui.dialog = null)}>Cancel</button>
    {#if code === 'prereq'}
      <button class="btn btn-quiet" onclick={() => go({ offline: false, prereq: 'skip' })}>Play without it</button>
      <button class="btn btn-primary" onclick={() => go({ offline: false, prereq: 'run' })}>Install and play</button>
    {:else}
      <button class="btn btn-primary" onclick={() => go({ offline: true })}>Play offline</button>
    {/if}
  </div>
</Modal>

<style>
  .msg {
    color: var(--text-2);
    margin: 8px 0 12px;
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 16px;
  }
</style>
