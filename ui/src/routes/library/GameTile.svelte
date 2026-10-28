<script lang="ts">
  import Cover from '../../lib/components/Cover.svelte';
  import Icon from '../../lib/components/Icon.svelte';
  import type { Game } from '../../lib/state/library.svelte';

  let {
    game,
    onplay,
    onmenu,
  }: { game: Game; onplay: (g: Game) => void; onmenu: (g: Game, x: number, y: number) => void } = $props();

  const installed = $derived(game.state === 'installed' || game.state === 'updateAvailable');
  const action = $derived(
    game.state === 'notInstalled' ? 'Install' : game.state === 'installing' ? 'Installing' : 'Play',
  );
</script>

<div
  class="tile"
  role="group"
  aria-label={game.title}
  oncontextmenu={(e) => {
    e.preventDefault();
    onmenu(game, e.clientX, e.clientY);
  }}
>
  <div class="art"><Cover {game} /></div>
  <div class="meta">
    <div class="txt">
      <div class="name" title={game.title}>{game.title}</div>
      {#if game.state === 'installed'}
        <div class="st status-ready">Ready</div>
      {:else if game.state === 'updateAvailable'}
        <div class="st status-update">Update available</div>
      {:else if game.state === 'installing'}
        <div class="st status-active">Installing</div>
      {:else}
        <div class="st status-muted">Not installed</div>
      {/if}
    </div>
    <button
      class="round"
      class:play={installed}
      aria-label="{action} {game.title}"
      disabled={game.state === 'installing'}
      onclick={() => onplay(game)}
    >
      <Icon name={installed ? 'play' : 'download'} size={installed ? 18 : 20} />
    </button>
  </div>
</div>

<style>
  .tile {
    display: flex;
    flex-direction: column;
    gap: 10px;
    min-width: 0;
  }
  .art {
    border-radius: var(--radius-md);
    overflow: hidden;
    box-shadow: 0 6px 18px rgba(0, 0, 0, 0.18);
    transition: transform var(--dur) var(--ease-spring);
  }
  .tile:hover .art {
    transform: translateY(-3px);
  }
  .meta {
    display: flex;
    align-items: center;
    gap: 8px;
    min-height: var(--hit);
  }
  .txt {
    flex: 1;
    min-width: 0;
  }
  .name {
    font-weight: 600;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .st {
    font-size: 12.5px;
  }
  .round {
    flex: none;
    display: grid;
    place-items: center;
    width: var(--hit);
    height: var(--hit);
    border-radius: 50%;
    background: var(--control);
    color: var(--text);
    transition:
      background-color var(--dur-fast) var(--ease-out),
      transform var(--dur-fast) var(--ease-spring);
  }
  .round:hover:not(:disabled) {
    background: var(--selected-strong);
  }
  .round.play {
    background: var(--accent-fill);
    color: #fff;
  }
  .round.play:hover {
    background: var(--accent-fill-hover);
  }
  .round:active:not(:disabled) {
    transform: scale(0.92);
  }
  .round:disabled {
    color: var(--text-2);
  }
</style>
