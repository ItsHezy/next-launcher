<script lang="ts">
  import { num } from '../../lib/boot';
  import Cover from '../../lib/components/Cover.svelte';
  import Icon from '../../lib/components/Icon.svelte';
  import { ago, playtime } from '../../lib/format';
  import { library, type Game } from '../../lib/state/library.svelte';

  let { game, onplay }: { game: Game; onplay: (g: Game) => void } = $props();
</script>

<article class="card" aria-label={game.title}>
  <Cover {game} kind="wide" label={false} />
  <div class="shade"></div>
  <div class="info">
    <div class="txt">
      <h3>{game.title}</h3>
      <p>
        {game.lastPlayed ? `Played ${ago(num(game.lastPlayed))}` : ''} · {playtime(num(game.playtimeSecs))}
      </p>
    </div>
    {#if library.running.has(game.appName)}
      <button class="btn btn-primary play" disabled aria-label="{game.title} is running">Playing</button>
    {:else}
      <button
        class="btn btn-primary play"
        disabled={library.starting.has(game.appName)}
        onclick={() => onplay(game)}
        aria-label="Play {game.title}"
      >
        <Icon name="play" size={16} /> Play
      </button>
    {/if}
  </div>
</article>

<style>
  .card {
    position: relative;
    border-radius: var(--radius-lg);
    overflow: hidden;
    background: var(--surface);
    isolation: isolate;
  }
  .shade {
    position: absolute;
    inset: 0;
    background: linear-gradient(180deg, transparent 35%, rgba(0, 0, 0, 0.78) 100%);
    pointer-events: none;
  }
  .info {
    position: absolute;
    left: 0;
    right: 0;
    bottom: 0;
    display: flex;
    align-items: flex-end;
    gap: 12px;
    padding: 16px;
  }
  .txt {
    flex: 1;
    min-width: 0;
  }
  h3 {
    margin: 0;
    font-size: 17px;
    font-weight: 700;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  p {
    font-size: 12.5px;
    color: rgba(255, 255, 255, 0.86);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .play {
    flex: none;
    padding: 0 18px 0 14px;
  }
</style>
