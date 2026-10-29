<script lang="ts">
  import { commands, type InstallPreview_Serialize } from '../bindings';
  import { num } from '../boot';
  import { bytes } from '../format';
  import { library } from '../state/library.svelte';
  import { ui } from '../state/ui.svelte';
  import Modal from './Modal.svelte';
  import Switch from './Switch.svelte';

  let { appName }: { appName: string } = $props();

  let preview = $state<InstallPreview_Serialize | null>(null);
  let error = $state<string | null>(null);
  let folder = $state('');
  let chosen = $state<Record<string, boolean>>({});
  let busy = $state(false);

  const title = $derived(preview?.title ?? library.byId.get(appName)?.title ?? 'Install');
  const extra = $derived(
    (preview?.components ?? []).filter((c) => !c.required && chosen[c.id]).reduce((s, c) => s + num(c.size), 0),
  );
  const total = $derived(num(preview?.installSize) + extra);
  const free = $derived(preview?.freeBytes == null ? null : num(preview.freeBytes));
  const tooBig = $derived(free !== null && total + 1024 ** 3 > free);

  async function load() {
    const res = await commands.prepareInstall(appName);
    if (res.status === 'error') {
      error = res.error.message;
      return;
    }
    preview = res.data;
    folder = res.data.defaultFolder;
  }

  async function install() {
    busy = true;
    const ids = Object.entries(chosen)
      .filter(([, on]) => on)
      .map(([id]) => id);
    const res = await commands.startInstall(appName, folder, ids);
    busy = false;
    if (res.status === 'error') {
      error = res.error.message;
      return;
    }
    ui.dialog = null;
    ui.toast(`Installing ${title}.`);
  }

  $effect(() => {
    void load();
  });
</script>

<Modal title="Install {title}" onclose={() => (ui.dialog = null)} {busy}>
  {#if error}
    <p class="msg err">{error}</p>
    <div class="actions"><button class="btn btn-quiet" onclick={() => (ui.dialog = null)}>Close</button></div>
  {:else if !preview}
    <p class="msg">Getting the file list from Epic…</p>
  {:else}
    <dl class="facts">
      <div><dt>Install size</dt><dd>{bytes(total)}</dd></div>
      <div><dt>Download</dt><dd>{bytes(num(preview.downloadSize))}</dd></div>
      {#if free !== null}<div><dt>Free on drive</dt><dd class:warn={tooBig}>{bytes(free)}</dd></div>{/if}
      <div><dt>Version</dt><dd class="ver">{preview.buildVersion}</dd></div>
    </dl>

    <label class="field">
      <span>Install folder</span>
      <input type="text" bind:value={folder} spellcheck="false" />
    </label>

    {#if preview.components.length > 0}
      <h3>Components</h3>
      <ul class="comps">
        {#each preview.components as c (c.id)}
          <li>
            <div class="txt">
              <span class="name">{c.title}</span>
              <span class="desc">{c.required ? 'Required' : c.description || 'Optional'} · {bytes(num(c.size))}</span>
            </div>
            <Switch
              checked={c.required || !!chosen[c.id]}
              disabled={c.required}
              label="Install {c.title}"
              onchange={(v) => (chosen[c.id] = v)}
            />
          </li>
        {/each}
      </ul>
    {/if}

    {#if tooBig}<p class="msg err">Not enough free space on that drive.</p>{/if}
    <div class="actions">
      <button class="btn btn-quiet" disabled={busy} onclick={() => (ui.dialog = null)}>Cancel</button>
      <button class="btn btn-primary" disabled={busy || tooBig || !folder.trim()} onclick={install}>
        {busy ? 'Starting…' : 'Install'}
      </button>
    </div>
  {/if}
</Modal>

<style>
  .msg {
    color: var(--text-2);
    margin: 8px 0 16px;
  }
  .err {
    color: var(--danger);
  }
  .facts {
    display: grid;
    grid-template-columns: repeat(2, 1fr);
    gap: 12px 24px;
    margin: 8px 0 18px;
  }
  .facts div {
    display: flex;
    flex-direction: column;
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
  .ver {
    font-weight: 500;
    font-size: 13px;
    overflow-wrap: anywhere;
  }
  .warn {
    color: var(--warning);
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin-bottom: 16px;
  }
  .field span {
    font-size: 13px;
    color: var(--text-2);
  }
  h3 {
    margin: 8px 0;
    font-size: 13px;
    color: var(--text-2);
  }
  .comps {
    list-style: none;
    margin: 0 0 12px;
    padding: 0;
    border-radius: var(--radius-md);
    background: var(--surface-inset);
    max-height: 260px;
    overflow-y: auto;
  }
  .comps li {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 4px 14px;
  }
  .comps li + li {
    border-top: 1px solid var(--border);
  }
  .txt {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }
  .name {
    font-weight: 500;
  }
  .desc {
    font-size: 12.5px;
    color: var(--text-3);
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 12px;
  }
</style>
