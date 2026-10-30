<script lang="ts">
  import { commands } from '../bindings';
  import { ui } from '../state/ui.svelte';
  import Modal from './Modal.svelte';

  interface Entry {
    name: string;
    version: string;
    license: string;
    source: string;
  }

  let entries = $state.raw<Entry[]>([]);
  let query = $state('');

  // Loaded on demand (≈ 330 entries), never part of the startup bundle.
  $effect(() => {
    void import('../licenses.json').then((m) => (entries = m.default as Entry[]));
  });

  const shown = $derived.by(() => {
    const q = query.trim().toLowerCase();
    return q ? entries.filter((e) => e.name.toLowerCase().includes(q) || e.license.toLowerCase().includes(q)) : entries;
  });

  async function openNotices() {
    const res = await commands.openNotices();
    if (res.status === 'error') ui.toast(res.error.message);
  }
</script>

<Modal title="Open-source licenses" onclose={() => (ui.dialog = null)} width={620}>
  <p class="msg">
    Next Launcher is built with {entries.length} open-source components. Their full license texts are in the
    notices file that ships with the app.
  </p>
  <input type="search" placeholder="Search components or licenses" aria-label="Search components or licenses" bind:value={query} />
  <ul class="list" aria-label="Components">
    {#each shown as e (e.name + e.version)}
      <li>
        <span class="name">{e.name} <span class="ver">{e.version}</span></span>
        <span class="lic">{e.license}</span>
      </li>
    {/each}
  </ul>
  <div class="actions">
    <button class="btn btn-quiet" onclick={openNotices}>Open full notices</button>
    <button class="btn btn-primary" onclick={() => (ui.dialog = null)}>Done</button>
  </div>
</Modal>

<style>
  .msg {
    margin: 8px 0 12px;
    color: var(--text-2);
  }
  input {
    width: 100%;
    margin-bottom: 10px;
  }
  .list {
    list-style: none;
    margin: 0;
    padding: 0;
    max-height: 360px;
    overflow-y: auto;
    border-radius: var(--radius-md);
    background: var(--surface-inset);
  }
  .list li {
    display: flex;
    justify-content: space-between;
    gap: 12px;
    padding: 8px 14px;
    font-size: 13px;
  }
  .list li + li {
    border-top: 1px solid var(--border);
  }
  .ver {
    color: var(--text-3);
  }
  .lic {
    color: var(--text-2);
    text-align: right;
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 16px;
  }
</style>
