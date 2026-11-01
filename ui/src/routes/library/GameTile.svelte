<script lang="ts">
  import Cover from '../../lib/components/Cover.svelte';
  import Icon from '../../lib/components/Icon.svelte';
  import { storeName } from '../../lib/format';
  import { bump } from '../../lib/motion';
  import { library, type Game } from '../../lib/state/library.svelte';

  let {
    game,
    onplay,
    onmenu,
  }: { game: Game; onplay: (g: Game) => void; onmenu: (g: Game, x: number, y: number) => void } = $props();

  const installed = $derived(game.state === 'installed' || game.state === 'updateAvailable');
  const playing = $derived(library.running.has(game.appName));
  const starting = $derived(library.starting.has(game.appName));
  const external = $derived(!installed && !!game.thirdParty);
  const unavailable = $derived(!installed && !game.installable && !game.thirdParty);
  const store = $derived(game.thirdParty ? storeName(game.thirdParty) : '');
  // One line under the title; it bumps when it changes (installed, started, updated…).
  const status = $derived(
    playing
      ? { text: 'Playing', tone: 'active' }
      : starting
        ? { text: 'Starting…', tone: 'active' }
        : game.state === 'installed'
          ? { text: 'Ready', tone: 'ready' }
          : game.state === 'updateAvailable'
            ? { text: 'Update available', tone: 'update' }
            : game.state === 'installing'
              ? { text: 'Installing', tone: 'active' }
              : external
                ? { text: `Opens in ${store}`, tone: 'muted' }
                : unavailable
                  ? { text: 'Not on Windows', tone: 'muted' }
                  : { text: 'Not installed', tone: 'muted' },
  );
  const action = $derived(
    external
      ? `Open in ${store}:`
      : unavailable
        ? 'Not installable on Windows:'
        : game.state === 'notInstalled'
          ? 'Install'
          : game.state === 'installing'
            ? 'Installing'
            : 'Play',
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
      <div class="st status-{status.tone}" use:bump={status.text}>
        {#if playing}<span class="live" aria-hidden="true"></span>{/if}{status.text}
      </div>
    </div>
    <button
      class="round"
      class:play={installed}
      aria-label="{playing ? 'Playing' : action} {game.title}"
      disabled={game.state === 'installing' || unavailable || playing || starting}
      onclick={() => onplay(game)}
    >
      <Icon
        name={installed ? 'play' : external ? 'external' : unavailable ? 'info' : 'download'}
        size={installed ? 18 : 20}
      />
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
    transition:
      transform var(--dur) var(--ease-spring),
      box-shadow var(--dur) var(--ease-out);
  }
  .tile:hover .art {
    transform: translateY(-4px);
    box-shadow: 0 14px 30px rgba(0, 0, 0, 0.36);
  }
  .tile:hover .art :global(img) {
    transform: scale(1.04);
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
    transform-origin: left center;
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
    transform: scale(1.08);
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
