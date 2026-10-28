<script lang="ts">
  import { num } from '../boot';
  import type { DownloadRow } from '../bindings';
  import { bytes, duration, speed } from '../format';

  let { dl }: { dl: DownloadRow } = $props();

  const done = $derived(num(dl.bytesDone));
  const total = $derived(Math.max(1, num(dl.bytesTotal)));
  const bps = $derived(num(dl.speedBps));
  const pct = $derived(Math.min(100, Math.floor((done / total) * 100)));
  const left = $derived(bps > 0 ? (total - done) / bps : NaN);
  const paused = $derived(dl.state === 'paused');
</script>

<section class="dl" aria-label="Active download">
  <div class="top">
    <span class="title">{dl.title}</span>
    <span class="pct">{pct}%</span>
  </div>
  <div
    class="bar"
    role="progressbar"
    aria-label="Download progress for {dl.title}"
    aria-valuemin="0"
    aria-valuemax="100"
    aria-valuenow={pct}
  >
    <span style:width="{pct}%"></span>
  </div>
  <div class="meta">
    {#if paused}
      <span>Paused</span>
    {:else}
      <span>{speed(bps)}</span>
      <span>{duration(left)} left</span>
    {/if}
  </div>
  <div class="sizes">{bytes(done)} of {bytes(total)}</div>
</section>

<style>
  .dl {
    margin: 0 12px;
    padding: 14px;
    border-radius: var(--radius-md);
    background: var(--surface-inset);
  }
  .top {
    display: flex;
    align-items: baseline;
    gap: 8px;
  }
  .title {
    flex: 1;
    min-width: 0;
    font-weight: 600;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .pct {
    font-weight: 600;
    font-variant-numeric: tabular-nums;
    color: var(--accent-text);
  }
  .bar {
    height: 6px;
    margin: 10px 0 8px;
    border-radius: 3px;
    background: rgba(255, 255, 255, 0.08);
    overflow: hidden;
  }
  .bar span {
    display: block;
    height: 100%;
    border-radius: inherit;
    background: var(--accent);
    transition: width var(--dur-slow) var(--ease-out);
  }
  .meta {
    display: flex;
    justify-content: space-between;
    color: var(--text-2);
    font-size: 12.5px;
    font-variant-numeric: tabular-nums;
  }
  .sizes {
    margin-top: 2px;
    color: var(--text-3);
    font-size: 12.5px;
    font-variant-numeric: tabular-nums;
  }
</style>
