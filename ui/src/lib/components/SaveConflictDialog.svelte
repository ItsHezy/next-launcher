<script lang="ts">
  import { commands } from '../bindings';
  import { play } from '../gameActions';
  import { library } from '../state/library.svelte';
  import { ui } from '../state/ui.svelte';
  import Modal from './Modal.svelte';

  let {
    appName,
    title,
    localTime,
    cloudTime,
    launch,
  }: { appName: string; title: string; localTime: string; cloudTime: string; launch: boolean } = $props();

  let busy = $state(false);
  let error = $state<string | null>(null);

  const fmt = (iso: string) =>
    iso ? new Intl.DateTimeFormat(undefined, { dateStyle: 'medium', timeStyle: 'short' }).format(Date.parse(iso)) : '—';

  async function choose(choice: 'download' | 'upload' | 'skip') {
    if (launch) {
      const g = library.byId.get(appName);
      ui.dialog = null;
      if (g) void play(g, { offline: false, saves: choice });
      return;
    }
    if (choice === 'skip') {
      ui.dialog = null;
      return;
    }
    busy = true;
    error = null;
    const res = await commands.cloudSaveSync(appName, choice);
    busy = false;
    if (res.status === 'ok') {
      ui.dialog = null;
      ui.toast(choice === 'download' ? `${title}: cloud save restored. Your previous files were backed up.` : `${title}: this PC's save is now in the cloud.`);
    } else error = res.error.message;
  }
</script>

<Modal title="Which save do you want to keep?" onclose={() => (ui.dialog = null)} {busy} width={500}>
  <p class="msg">{title} has saves that changed both on this PC and in the cloud since they last matched.</p>
  <dl class="times">
    <div><dt>This PC</dt><dd>{fmt(localTime)}</dd></div>
    <div><dt>Cloud</dt><dd>{fmt(cloudTime)}</dd></div>
  </dl>
  <p class="hint">
    Using the cloud save first copies this PC's files to a backup folder (the last 3 are kept).
    Keeping this PC's save uploads it as the newest cloud save; older cloud saves stay on Epic's side.
  </p>
  {#if error}<p class="msg err">{error}</p>{/if}
  <div class="actions">
    <button class="btn btn-quiet" disabled={busy} onclick={() => choose('skip')}>
      {launch ? 'Play without syncing' : 'Decide later'}
    </button>
    <button class="btn btn-quiet" disabled={busy} onclick={() => choose('upload')}>Keep this PC's save</button>
    <button class="btn btn-primary" disabled={busy} onclick={() => choose('download')}>Use cloud save</button>
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
  .times {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 12px;
    margin: 0 0 12px;
    padding: 12px 14px;
    border-radius: var(--radius-md);
    background: var(--surface-inset);
  }
  .times div {
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
  }
  .hint {
    margin: 0;
    font-size: 12.5px;
    color: var(--text-2);
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    flex-wrap: wrap;
    gap: 8px;
    margin-top: 16px;
  }
</style>
