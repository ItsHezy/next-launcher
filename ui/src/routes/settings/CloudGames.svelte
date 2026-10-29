<script lang="ts">
  import { commands, type SaveStatus } from '../../lib/bindings';
  import { num } from '../../lib/boot';
  import Switch from '../../lib/components/Switch.svelte';
  import { ago } from '../../lib/format';
  import { auth } from '../../lib/state/auth.svelte';
  import { settings } from '../../lib/state/settings.svelte';
  import { ui } from '../../lib/state/ui.svelte';

  const enabled = $derived(Boolean(settings.get('cloud.enabled')));
  let games = $state.raw<SaveStatus[] | null>(null);
  let status = $state<Record<string, SaveStatus>>({});
  let busy = $state<Record<string, boolean>>({});

  const LABEL: Record<string, string> = {
    nothing: 'No saves yet',
    inSync: 'In sync',
    upload: 'This PC is newer',
    download: 'Cloud is newer',
    conflict: 'Changed in both places',
  };

  async function load() {
    const res = await commands.cloudSaveGames();
    if (res.status !== 'ok') return;
    games = res.data;
    if (!auth.signedIn) return;
    await Promise.all(res.data.map((g) => refresh(g.appName)));
  }

  async function refresh(appName: string) {
    const res = await commands.cloudSaveStatus(appName);
    if (res.status === 'ok') status = { ...status, [appName]: res.data };
  }

  async function sync(g: SaveStatus, action: 'auto' | 'upload' | 'download') {
    busy = { ...busy, [g.appName]: true };
    const res = await commands.cloudSaveSync(g.appName, action);
    busy = { ...busy, [g.appName]: false };
    if (res.status === 'error') {
      ui.toast(res.error.message);
      return;
    }
    status = { ...status, [g.appName]: res.data };
    if (res.data.decision === 'conflict') {
      ui.dialog = {
        kind: 'saves',
        appName: g.appName,
        title: g.title,
        localTime: iso(res.data.localTime),
        cloudTime: iso(res.data.cloudTime),
        launch: false,
      };
    } else ui.toast(`${g.title}: saves are in sync.`);
  }

  const iso = (t: number | null) => (t == null ? '' : new Date(num(t) * 1000).toISOString());

  function line(s: SaveStatus | undefined): string {
    if (!s) return auth.signedIn ? 'Checking…' : 'Sign in to sync';
    const parts = [LABEL[s.decision] ?? s.decision];
    if (s.lastSync != null) parts.push(`synced ${ago(num(s.lastSync))}`);
    else if (s.cloudTime == null && num(s.localFiles) > 0) parts.push('not in the cloud yet');
    return parts.join(' · ');
  }

  $effect(() => {
    void load();
  });
</script>

<h2 class="h">Games with cloud saves</h2>
{#if games === null}
  <p class="empty">Loading…</p>
{:else if games.length === 0}
  <p class="empty">None of your installed games use Epic's cloud saves.</p>
{:else}
  <ul class="list" aria-label="Cloud saves per game">
    {#each games as g (g.appName)}
      {@const on = enabled && Boolean(settings.getRaw(`game.${g.appName}.cloud`, true))}
      {@const s = status[g.appName]}
      <li class="row">
        <span class="txt">
          <span class="name">{g.title}</span>
          <span class="meta" class:warn={s?.decision === 'conflict'}>{line(s)}</span>
        </span>
        {#if auth.signedIn && on}
          <button class="btn btn-quiet" disabled={busy[g.appName]} onclick={() => sync(g, 'auto')}>
            {busy[g.appName] ? 'Syncing…' : 'Sync now'}
          </button>
        {/if}
        <Switch
          checked={on}
          label="Cloud saves for {g.title}"
          disabled={!enabled}
          onchange={(v) => settings.setRaw(`game.${g.appName}.cloud`, v)}
        />
      </li>
    {/each}
  </ul>
  <p class="foot">
    Before a cloud save replaces files on this PC, the old files are copied to a backup folder (the last 3 are kept).
  </p>
{/if}

<style>
  .h {
    font-size: 13px;
    color: var(--text-2);
    margin: 20px 4px 8px;
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
    align-items: center;
    gap: 12px;
    min-height: 60px;
    padding: 6px 16px;
  }
  .row + .row {
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
  .meta {
    font-size: 12.5px;
    color: var(--text-3);
  }
  .meta.warn {
    color: var(--warning);
  }
  .empty,
  .foot {
    color: var(--text-2);
    font-size: 13px;
    margin: 8px 4px;
  }
</style>
