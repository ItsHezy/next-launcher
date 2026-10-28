<script lang="ts">
  import Switch from '../../lib/components/Switch.svelte';
  import { isInstalled, library } from '../../lib/state/library.svelte';
  import { settings } from '../../lib/state/settings.svelte';

  // Phase 8 limits this to games whose catalog entry supports cloud saves.
  const games = $derived(library.games.filter(isInstalled).slice(0, 12));
  const enabled = $derived(Boolean(settings.get('cloud.enabled')));
</script>

<h2 class="h">Games</h2>
<ul class="list" aria-label="Cloud saves per game">
  {#each games as g (g.appName)}
    {@const on = enabled && Boolean(settings.getRaw(`game.${g.appName}.cloud`, true))}
    <li class="row">
      <span class="name">{g.title}</span>
      <Switch checked={on} label="Cloud saves for {g.title}" disabled={!enabled} onchange={(v) => settings.setRaw(`game.${g.appName}.cloud`, v)} />
    </li>
  {/each}
</ul>

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
    justify-content: space-between;
    min-height: 56px;
    padding: 0 16px;
  }
  .row + .row {
    border-top: 1px solid var(--border);
  }
  .name {
    font-weight: 500;
  }
</style>
