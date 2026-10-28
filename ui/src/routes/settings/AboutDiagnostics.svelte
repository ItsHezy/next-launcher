<script lang="ts">
  import { commands, type Mark } from '../../lib/bindings';
  import { num } from '../../lib/boot';
  import { router } from '../../lib/state/router.svelte';

  let marks = $state<Mark[]>([]);
  $effect(() => {
    commands.startupTimeline().then((m) => (marks = m));
  });
</script>

<h2 class="h">Performance</h2>
<div class="box">
  <h3>Startup timeline <span>(ms since launch)</span></h3>
  <table>
    <tbody>
      {#each marks as m (m.name)}
        <tr><td class="ms">{num(m.ms).toFixed(1)}</td><td>{m.name}</td></tr>
      {/each}
    </tbody>
  </table>
  {#if router.timings.length}
    <h3>Recent tab switches</h3>
    <table>
      <tbody>
        {#each router.timings.slice(-8) as t, i (i)}
          <tr><td class="ms">{t.ms.toFixed(1)}</td><td>{t.route}{t.firstVisit ? ' (first visit)' : ''}</td></tr>
        {/each}
      </tbody>
    </table>
  {/if}
</div>

<style>
  .h {
    font-size: 13px;
    color: var(--text-2);
    margin: 20px 4px 8px;
  }
  .box {
    padding: 16px;
    border-radius: var(--radius-md);
    background: var(--surface-inset);
    user-select: text;
    -webkit-user-select: text;
  }
  h3 {
    margin: 0 0 6px;
    font-size: 13px;
    font-weight: 600;
  }
  h3 span {
    color: var(--text-3);
    font-weight: 400;
  }
  table {
    border-collapse: collapse;
    margin-bottom: 14px;
    font-size: 12.5px;
    font-variant-numeric: tabular-nums;
  }
  td {
    padding: 2px 12px 2px 0;
    color: var(--text-2);
  }
  .ms {
    text-align: right;
    color: var(--text);
    min-width: 64px;
  }
</style>
