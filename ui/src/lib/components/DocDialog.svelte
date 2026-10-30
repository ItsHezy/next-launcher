<script lang="ts">
  // A document that ships with the app, shown in the app (no editor window).
  import { commands, type BundledDoc } from '../bindings';
  import { ui } from '../state/ui.svelte';
  import Modal from './Modal.svelte';

  let { doc, title }: { doc: BundledDoc; title: string } = $props();

  let text = $state<string | null>(null);
  let failed = $state<string | null>(null);

  $effect(() => {
    void commands.readBundled(doc).then((res) => {
      if (res.status === 'ok') text = res.data;
      else failed = res.error.message;
    });
  });
</script>

<Modal {title} onclose={() => (ui.dialog = null)} width={720}>
  {#if failed}
    <p class="msg">{failed}</p>
  {:else}
    <pre class="doc" aria-label={title}>{text ?? 'Loading…'}</pre>
  {/if}
  <div class="actions">
    <button class="btn btn-primary" onclick={() => (ui.dialog = null)}>Done</button>
  </div>
</Modal>

<style>
  .msg {
    margin: 8px 0 0;
    color: var(--text-2);
  }
  .doc {
    margin: 8px 0 0;
    max-height: min(60vh, 560px);
    overflow: auto;
    padding: 14px 16px;
    border-radius: var(--radius-md);
    background: var(--surface-inset);
    font-size: 12.5px;
    line-height: 1.5;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    margin-top: 16px;
  }
</style>
