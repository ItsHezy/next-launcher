<script lang="ts">
  import { num } from '../../lib/boot';
  import Cover from '../../lib/components/Cover.svelte';
  import Icon from '../../lib/components/Icon.svelte';
  import * as actions from '../../lib/gameActions';
  import Switch from '../../lib/components/Switch.svelte';
  import { bytes } from '../../lib/format';
  import { isInstalled, library } from '../../lib/state/library.svelte';
  import { settings } from '../../lib/state/settings.svelte';

  const installed = $derived(library.games.filter(isInstalled));
  let open = $state<string | null>(null);

  function flag(app: string, what: 'autoUpdate' | 'cloud', fallback: boolean): boolean {
    return Boolean(settings.getRaw(`game.${app}.${what}`, fallback));
  }
</script>

<p class="lead">{installed.length} installed games</p>

<ul class="list">
  {#each installed as g (g.appName)}
    {@const expanded = open === g.appName}
    <li class="game">
      <button
        class="head"
        aria-expanded={expanded}
        aria-controls="mg-{g.appName}"
        onclick={() => (open = expanded ? null : g.appName)}
      >
        <span class="thumb"><Cover game={g} kind="thumb" /></span>
        <span class="txt">
          <span class="name">{g.title}</span>
          <span class="meta">{bytes(num(g.installSize))}{g.state === 'updateAvailable' ? ' · Update available' : ''}</span>
        </span>
        <span class="chev" class:up={expanded}><Icon name="chevronDown" size={18} /></span>
      </button>
      {#if expanded}
        <div class="body" id="mg-{g.appName}">
          <div class="actions">
            <button class="btn btn-quiet" onclick={() => actions.move(g.appName)}><Icon name="folder" size={18} /> Move…</button>
            {#if g.state === 'updateAvailable'}
              <button class="btn btn-primary" onclick={() => actions.update(g.appName, g.title)}><Icon name="update" size={18} /> Update</button>
            {/if}
            <button class="btn btn-quiet" onclick={() => actions.verifyAndRepair(g.appName, g.title)}><Icon name="shield" size={18} /> Verify & repair</button>
            <button class="btn btn-danger" onclick={() => actions.uninstall(g.appName)}>Uninstall…</button>
          </div>
          <label class="field">
            <span>Launch arguments</span>
            <input
              type="text"
              placeholder="e.g. -windowed"
              value={String(settings.getRaw(`game.${g.appName}.args`, ''))}
              onchange={(e) => settings.setRaw(`game.${g.appName}.args`, e.currentTarget.value)}
            />
          </label>
          {#each [{ k: 'cloud', label: 'Cloud saves', d: true }, { k: 'autoUpdate', label: 'Auto-update', d: true }] as const as f (f.k)}
            {@const on = flag(g.appName, f.k, f.d)}
            <div class="toggle-row">
              <span>{f.label}</span>
              <Switch checked={on} label="{f.label} for {g.title}" onchange={(v) => settings.setRaw(`game.${g.appName}.${f.k}`, v)} />
            </div>
          {/each}
        </div>
      {/if}
    </li>
  {/each}
</ul>

<style>
  .lead {
    color: var(--text-2);
    margin-bottom: 12px;
  }
  .list {
    list-style: none;
    margin: 0;
    padding: 0;
    border-radius: var(--radius-md);
    background: var(--surface-inset);
    overflow: hidden;
  }
  .game + .game {
    border-top: 1px solid var(--border);
  }
  .head {
    display: flex;
    align-items: center;
    gap: 14px;
    width: 100%;
    min-height: 60px;
    padding: 8px 16px;
    text-align: left;
  }
  .head:hover {
    background: var(--hover);
  }
  .head:focus-visible {
    outline-offset: -2px;
  }
  .thumb {
    width: 40px;
    height: 40px;
    border-radius: 10px;
    overflow: hidden;
    flex: none;
  }
  .txt {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }
  .name {
    font-weight: 600;
  }
  .meta {
    font-size: 12.5px;
    color: var(--text-3);
  }
  .chev {
    display: grid;
    color: var(--text-2);
    transition: transform var(--dur) var(--ease-spring);
  }
  .chev.up {
    transform: rotate(180deg);
  }
  .body {
    padding: 4px 16px 16px 70px;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .actions {
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
  }
  .field {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .field input {
    flex: 1;
    max-width: 360px;
  }
  .toggle-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    max-width: 480px;
  }
</style>
