<script lang="ts">
  import { num } from '../boot';
  import { commands, type DownloadRow } from '../bindings';
  import { bytes, duration, speed } from '../format';
  import { library } from '../state/library.svelte';
  import { ui } from '../state/ui.svelte';
  import Icon from './Icon.svelte';

  let { dl }: { dl: DownloadRow } = $props();

  // Live numbers from progress events when they belong to this job, else the snapshot row.
  const live = $derived(library.progress && num(library.progress.id) === num(dl.id) ? library.progress : null);
  const done = $derived(num(live?.bytesDone ?? dl.bytesDone));
  const total = $derived(Math.max(1, num(live?.bytesTotal ?? dl.bytesTotal)));
  const bps = $derived(num(live?.speedBps ?? dl.speedBps));
  const pct = $derived(Math.min(100, Math.floor((done / total) * 100)));
  const left = $derived(bps > 0 ? (total - done) / bps : NaN);
  const paused = $derived(dl.state === 'paused');
  const failed = $derived(dl.state === 'failed');
  const phase = $derived(live?.phase ?? (dl.state === 'queued' ? 'queued' : 'downloading'));

  async function act(fn: () => Promise<{ status: string; error?: { message: string } }>) {
    const res = await fn();
    if (res.status === 'error' && res.error) ui.toast(res.error.message);
  }
</script>

<section class="dl" aria-label="Active download">
  <div class="top">
    <span class="title">{dl.title}</span>
    <span class="pct">{pct}%</span>
  </div>
  <div
    class="bar"
    class:paused={paused || failed}
    role="progressbar"
    aria-label="Download progress for {dl.title}"
    aria-valuemin="0"
    aria-valuemax="100"
    aria-valuenow={pct}
  >
    <span style:width="{pct}%"><i class="sheen" class:on={!paused && !failed && phase !== 'queued'}></i></span>
  </div>
  <div class="meta">
    {#if failed}
      <span class="err">Stopped</span>
    {:else if paused}
      <span>Paused</span>
    {:else if phase === 'preparing'}
      <span>Preparing…</span>
    {:else if phase === 'verifying'}
      <span>Checking files…</span>
    {:else if phase === 'finishing'}
      <span>Finishing…</span>
    {:else if phase === 'queued'}
      <span>Queued</span>
    {:else}
      <span>{speed(bps)}</span>
      <span>{duration(left)} left</span>
    {/if}
  </div>
  <div class="foot">
    <span class="sizes">{bytes(done)} of {bytes(total)}</span>
    <span class="ctl">
      {#if paused || failed}
        <button class="icon-btn" aria-label="Resume download" onclick={() => act(() => commands.resumeDownload(num(dl.id)))}>
          <Icon name="play" size={16} />
        </button>
      {:else}
        <button class="icon-btn" aria-label="Pause download" onclick={() => act(() => commands.pauseDownload(num(dl.id)))}>
          <Icon name="pause" size={18} />
        </button>
      {/if}
      <button class="icon-btn" aria-label="Cancel download" onclick={() => act(() => commands.cancelDownload(num(dl.id)))}>
        <Icon name="close" size={16} />
      </button>
    </span>
  </div>
</section>

<style>
  .dl {
    margin: 0 12px;
    padding: 14px 14px 6px;
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
    position: relative;
    display: block;
    height: 100%;
    overflow: hidden;
    border-radius: inherit;
    background: var(--accent);
    transition: width var(--dur-slow) var(--ease-out);
  }
  /* Working: a light runs along the bar. */
  .sheen.on {
    position: absolute;
    inset: 0;
    background: linear-gradient(90deg, transparent, rgba(255, 255, 255, 0.35), transparent);
    animation: sheen 1.6s var(--ease-out) infinite;
  }
  .bar.paused span {
    background: var(--text-3);
  }
  .meta {
    display: flex;
    justify-content: space-between;
    color: var(--text-2);
    font-size: 12.5px;
    font-variant-numeric: tabular-nums;
  }
  .err {
    color: var(--danger);
  }
  .foot {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  .sizes {
    color: var(--text-3);
    font-size: 12.5px;
    font-variant-numeric: tabular-nums;
  }
  .ctl {
    display: flex;
    margin-right: -8px;
  }
</style>
