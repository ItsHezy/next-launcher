<script lang="ts">
  import Icon from '../../lib/components/Icon.svelte';
  import Switch from '../../lib/components/Switch.svelte';
  import { library } from '../../lib/state/library.svelte';
  import { ACCENTS, settings } from '../../lib/state/settings.svelte';
  import { router } from '../../lib/state/router.svelte';
  import { comingSoon } from '../../lib/state/ui.svelte';
  import { wallet } from '../../lib/mock';
  import type { Action, Row } from './schema';

  let { row }: { row: Row } = $props();

  const vars = $derived<Record<string, string>>({
    displayName: library.account?.displayName ?? '—',
    email: library.account?.email ?? '—',
    country: library.account?.country ?? '—',
    balance: wallet.balance,
    rewards: wallet.rewards,
    coupons: `${wallet.coupons} available`,
  });
  const fill = (s: string) => s.replace(/\{(\w+)\}/g, (_, k: string) => vars[k] ?? '');

  function run(a: Action) {
    if (a.type === 'goto') router.go('settings', a.section);
    else if (a.type === 'web') comingSoon(`${a.what} (opens Epic's page, signed in)`, a.phase);
    else comingSoon(a.what, a.phase);
  }

  const id = `row-${Math.random().toString(36).slice(2, 9)}`;
</script>

{#snippet text()}
  <span class="text">
    <span class="label" id="{id}-l">{row.label}</span>
    {#if row.desc}<span class="desc" id="{id}-d">{row.desc}</span>{/if}
    {#if row.kind === 'toggle' && row.disabled}<span class="desc">{row.disabled}</span>{/if}
  </span>
{/snippet}

{#if row.kind === 'value' || row.kind === 'web'}
  <button class="row clickable" onclick={() => run(row.action)}>
    {@render text()}
    {#if row.kind === 'value'}<span class="value">{fill(row.value)}</span>{/if}
    <span class="trail"><Icon name={row.kind === 'web' ? 'external' : 'chevronRight'} size={18} /></span>
  </button>
{:else}
  <div class="row">
    {@render text()}
    {#if row.kind === 'toggle'}
      {@const on = !row.disabled && Boolean(settings.get(row.key))}
      <Switch checked={on} labelledby="{id}-l" disabled={!!row.disabled} onchange={(v) => settings.set(row.key, v as never)} />
    {:else if row.kind === 'select'}
      <select
        aria-labelledby="{id}-l"
        value={String(settings.get(row.key))}
        onchange={(e) => {
          const raw = e.currentTarget.value;
          const opt = row.options.find((o) => String(o.value) === raw);
          if (opt) settings.set(row.key, opt.value as never);
        }}
      >
        {#each row.options as o (o.value)}
          <option value={String(o.value)}>{o.label}</option>
        {/each}
      </select>
    {:else if row.kind === 'accent'}
      <div class="swatches" role="radiogroup" aria-labelledby="{id}-l">
        {#each ACCENTS as a (a.value)}
          <button
            class="swatch"
            role="radio"
            aria-checked={settings.get('general.accent') === a.value}
            aria-label={a.label}
            style:background={a.value}
            onclick={() => settings.set('general.accent', a.value)}
          >
            {#if settings.get('general.accent') === a.value}<Icon name="check" size={16} stroke={2.4} />{/if}
          </button>
        {/each}
      </div>
    {:else if row.kind === 'folder'}
      <span class="value path">{settings.get(row.key)}</span>
      <button class="btn btn-quiet" onclick={() => comingSoon('Choosing a folder', 4)}>
        <Icon name="folder" size={18} /> Change…
      </button>
    {:else if row.kind === 'text'}
      <input
        type="text"
        aria-labelledby="{id}-l"
        placeholder={row.placeholder}
        disabled={row.enabledBy ? !settings.get(row.enabledBy) : false}
        value={String(settings.get(row.key))}
        onchange={(e) => settings.set(row.key, e.currentTarget.value as never)}
      />
    {:else if row.kind === 'button'}
      <button class="btn btn-quiet" onclick={() => run(row.action)}>{row.button}</button>
    {:else if row.kind === 'danger'}
      <button class="btn btn-danger" onclick={() => run(row.action)}>{row.button}</button>
    {/if}
  </div>
{/if}

<style>
  .row {
    display: flex;
    align-items: center;
    gap: 16px;
    width: 100%;
    min-height: 56px;
    padding: 8px 16px;
    text-align: left;
  }
  .clickable {
    border-radius: 0;
    transition: background-color var(--dur-fast) var(--ease-out);
  }
  .clickable:hover {
    background: var(--hover);
  }
  .clickable:focus-visible {
    outline-offset: -2px;
  }
  .text {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .label {
    font-weight: 500;
  }
  .desc {
    font-size: 12.5px;
    color: var(--text-3);
  }
  .value {
    color: var(--text-2);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    max-width: 50%;
  }
  .path {
    font-variant-numeric: tabular-nums;
  }
  .trail {
    display: grid;
    color: var(--text-2);
  }
  select {
    min-height: var(--hit);
    padding: 0 36px 0 14px;
    border-radius: var(--radius-sm);
    border: 1px solid var(--border);
    background-color: var(--control);
    appearance: none;
    cursor: pointer;
    background-image: linear-gradient(45deg, transparent 50%, var(--text-2) 50%),
      linear-gradient(135deg, var(--text-2) 50%, transparent 50%);
    background-position:
      calc(100% - 18px) 52%,
      calc(100% - 13px) 52%;
    background-size: 5px 5px;
    background-repeat: no-repeat;
  }
  select:focus-visible {
    outline: 2px solid var(--accent-text);
    outline-offset: 2px;
  }
  option {
    background: var(--surface);
  }
  input {
    width: 260px;
  }
  input:disabled {
    opacity: 0.5;
  }
  .swatches {
    display: flex;
    gap: 6px;
  }
  .swatch {
    display: grid;
    place-items: center;
    width: var(--hit);
    height: var(--hit);
    border-radius: 50%;
    color: #fff;
    box-shadow: inset 0 0 0 2px rgba(255, 255, 255, 0.12);
    transition: transform var(--dur-fast) var(--ease-spring);
  }
  .swatch[aria-checked='true'] {
    box-shadow:
      0 0 0 2px var(--surface-inset),
      0 0 0 4px rgba(255, 255, 255, 0.8);
  }
  .swatch:active {
    transform: scale(0.92);
  }
</style>
