<script lang="ts">
  import { commands } from '../../lib/bindings';
  import { num } from '../../lib/boot';
  import Icon from '../../lib/components/Icon.svelte';
  import Switch from '../../lib/components/Switch.svelte';
  import { account, CONNECTIONS, connectionName, languageName } from '../../lib/state/account.svelte';
  import { library } from '../../lib/state/library.svelte';
  import { auth } from '../../lib/state/auth.svelte';
  import { ACCENTS, settings } from '../../lib/state/settings.svelte';
  import { router } from '../../lib/state/router.svelte';
  import { comingSoon, ui } from '../../lib/state/ui.svelte';
  import { appVersion } from './about.svelte';
  import type { Action, Row } from './schema';

  let { row }: { row: Row } = $props();

  const vars = $derived.by<Record<string, string>>(() => {
    const d = account.details;
    const w = account.wallet;
    const unavailable = w && !w.available ? 'On Epic' : '—';
    const v: Record<string, string> = {
      displayName: d?.displayName ?? library.account?.displayName ?? '—',
      email: d?.email ?? library.account?.email ?? '—',
      country: d?.country ?? library.account?.country ?? '—',
      language: languageName(d?.preferredLanguage),
      tfa: d?.tfaEnabled == null ? '—' : d.tfaEnabled ? 'On' : 'Off',
      balance: w?.balance ?? (w?.available ? 'Not in your region' : unavailable),
      rewards: w?.rewards ?? (w?.available ? 'Not in your region' : unavailable),
      coupons: w?.coupons == null ? unavailable : `${num(w.coupons)} available`,
      hidden: account.hiddenCount === 0 ? 'None' : String(account.hiddenCount),
      version: appVersion.value,
    };
    for (const c of CONNECTIONS) {
      const link = d?.connections?.find((x) => x.kind === c.kind);
      v[`conn.${c.kind}`] = d?.connections == null ? '—' : link ? (link.name ?? 'Connected') : 'Not connected';
    }
    const others = (d?.connections ?? []).filter((x) => !CONNECTIONS.some((c) => c.kind === x.kind));
    v['conn.other'] = d?.connections == null ? '—' : others.length ? others.map((x) => connectionName(x.kind)).join(', ') : 'None';
    return v;
  });
  const fill = (s: string) => s.replace(/\{([\w.]+)\}/g, (_, k: string) => vars[k] ?? '');

  async function check(fn: () => Promise<{ status: 'ok'; data: { ok: boolean; message: string } } | { status: 'error'; error: { message: string } }>) {
    busy = true;
    const res = await fn();
    busy = false;
    ui.toast(res.status === 'ok' ? res.data.message : res.error.message);
  }

  let busy = $state(false);

  function run(a: Action) {
    if (a.type === 'import') ui.dialog = 'import';
    else if (a.type === 'goto') router.go('settings', a.section);
    else if (a.type === 'web') void auth.openEpicPage(a.page);
    else if (a.type === 'dialog') {
      if (a.dialog === 'licenses') {
        ui.dialog = { kind: 'licenses' };
        return;
      }
      const inApp =
        a.dialog === 'hidden' ||
        (a.dialog === 'redeem' ? account.caps?.redeem : account.caps?.updateAccount) === true;
      if (inApp) ui.dialog = { kind: 'account', what: a.dialog };
      else if (a.fallback) void auth.openEpicPage(a.fallback);
    } else if (a.type === 'cmd') {
      if (a.cmd === 'checkConnection') void check(commands.checkConnection);
      else if (a.cmd === 'checkFolder') void check(commands.checkInstallFolder);
      else if (a.cmd === 'checkDisk') void check(commands.checkDisk);
      else if (a.cmd === 'openLogs')
        void commands.openLogsFolder().then((r) => r.status === 'error' && ui.toast(r.error.message));
      else if (a.cmd === 'openLicense')
        void commands.openBundled('license').then((r) => r.status === 'error' && ui.toast(r.error.message));
    } else comingSoon(a.what, a.phase);
  }

  async function pickFolder(key: Extract<Row, { kind: 'folder' }>['key']) {
    const res = await commands.pickFolder('Choose where new games are installed');
    if (res.status === 'ok' && res.data) settings.set(key, res.data as never);
    else if (res.status === 'error') ui.toast(res.error.message);
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

{#if row.kind === 'info'}
  <div class="row">
    {@render text()}
    <span class="value">{fill(row.value)}</span>
  </div>
{:else if row.kind === 'value' || row.kind === 'web'}
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
      <button class="btn btn-quiet" onclick={() => pickFolder(row.key)}>
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
      <button class="btn btn-quiet" disabled={busy} onclick={() => run(row.action)}>{busy ? 'Checking…' : row.button}</button>
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
