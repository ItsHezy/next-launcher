<script lang="ts">
  import { commands, type HiddenGame } from '../bindings';
  import { account, LANGUAGES } from '../state/account.svelte';
  import { auth } from '../state/auth.svelte';
  import { ui } from '../state/ui.svelte';
  import Modal from './Modal.svelte';

  let { what }: { what: 'displayName' | 'language' | 'redeem' | 'hidden' } = $props();

  const titles = {
    displayName: 'Change display name',
    language: 'Communication language',
    redeem: 'Redeem a code',
    hidden: 'Hidden games',
  } as const;

  let busy = $state(false);
  let error = $state<string | null>(null);
  let done = $state<string | null>(null);
  let name = $state(account.details?.displayName ?? '');
  let lang = $state(account.details?.preferredLanguage ?? 'en');
  let code = $state('');
  let hidden = $state.raw<HiddenGame[] | null>(null);

  const nameOk = $derived(name.trim().length >= 3 && name.trim().length <= 16);
  const changed = $derived(
    what === 'displayName'
      ? name.trim() !== (account.details?.displayName ?? '')
      : lang !== (account.details?.preferredLanguage ?? ''),
  );

  $effect(() => {
    if (what === 'hidden') void loadHidden();
  });

  async function loadHidden() {
    const res = await commands.hiddenGames();
    if (res.status === 'ok') hidden = res.data;
    else error = res.error.message;
  }

  async function unhide(appName: string) {
    const res = await commands.setHidden(appName, false);
    if (res.status === 'error') error = res.error.message;
    await loadHidden();
    void account.loadHidden();
  }

  async function save() {
    busy = true;
    error = null;
    const res = await commands.updateAccount(
      what === 'displayName'
        ? { displayName: name.trim(), preferredLanguage: null }
        : { displayName: null, preferredLanguage: lang },
    );
    busy = false;
    if (res.status === 'ok') {
      account.details = res.data;
      ui.dialog = null;
      ui.toast(what === 'displayName' ? `Your display name is now ${res.data.displayName}.` : 'Communication language saved.');
    } else error = res.error.message;
  }

  async function redeem() {
    busy = true;
    error = null;
    done = null;
    const res = await commands.redeemCode(code);
    busy = false;
    if (res.status === 'ok') {
      code = '';
      done = res.data.items.length
        ? `Added to your account: ${res.data.items.join(', ')}.`
        : 'Code redeemed. It may take a minute to show up in your library.';
    } else error = res.error.message;
  }
</script>

<Modal title={titles[what]} onclose={() => (ui.dialog = null)} {busy} width={460}>
  {#if what === 'displayName'}
    {#if account.details && !account.details.canChangeName}
      <p class="msg">Epic doesn't allow another display name change yet.</p>
      <div class="actions">
        <button class="btn btn-quiet" onclick={() => (ui.dialog = null)}>Close</button>
        <button class="btn btn-primary" onclick={() => (ui.dialog = null, auth.openEpicPage('accountPersonal'))}>
          Open Epic's page
        </button>
      </div>
    {:else}
      <label class="field">
        <span>Display name</span>
        <input type="text" bind:value={name} maxlength="16" spellcheck="false" autocomplete="off" />
      </label>
      <p class="hint">3 to 16 characters. Epic limits how often you can change it, and your friends see the new name.</p>
      {#if error}<p class="msg err">{error}</p>{/if}
      <div class="actions">
        <button class="btn btn-quiet" disabled={busy} onclick={() => (ui.dialog = null)}>Cancel</button>
        <button class="btn btn-primary" disabled={busy || !nameOk || !changed} onclick={save}>
          {busy ? 'Saving…' : 'Change name'}
        </button>
      </div>
    {/if}
  {:else if what === 'language'}
    <label class="field">
      <span>Language for emails from Epic</span>
      <select bind:value={lang}>
        {#each LANGUAGES as l (l.value)}<option value={l.value}>{l.label}</option>{/each}
      </select>
    </label>
    {#if error}<p class="msg err">{error}</p>{/if}
    <div class="actions">
      <button class="btn btn-quiet" disabled={busy} onclick={() => (ui.dialog = null)}>Cancel</button>
      <button class="btn btn-primary" disabled={busy || !changed} onclick={save}>{busy ? 'Saving…' : 'Save'}</button>
    </div>
  {:else if what === 'redeem'}
    <label class="field">
      <span>Code</span>
      <input
        type="text"
        bind:value={code}
        placeholder="XXXXX-XXXXX-XXXXX"
        spellcheck="false"
        autocomplete="off"
        onkeydown={(e) => e.key === 'Enter' && code.trim() && !busy && redeem()}
      />
    </label>
    <p class="hint">
      Game keys and other codes. Epic gift cards and Fortnite V-Bucks cards are redeemed on Epic's pages
      (Settings → Wallet &amp; rewards).
    </p>
    {#if error}<p class="msg err">{error}</p>{/if}
    {#if done}<p class="msg ok">{done}</p>{/if}
    <div class="actions">
      <button class="btn btn-quiet" disabled={busy} onclick={() => (ui.dialog = null)}>{done ? 'Done' : 'Cancel'}</button>
      <button class="btn btn-primary" disabled={busy || code.trim().length < 4} onclick={redeem}>
        {busy ? 'Redeeming…' : 'Redeem'}
      </button>
    </div>
  {:else}
    {#if hidden === null}
      <p class="msg">Loading…</p>
    {:else if hidden.length === 0}
      <p class="msg">No hidden games. Hide one from its menu in the library.</p>
    {:else}
      <ul class="list">
        {#each hidden as g (g.appName)}
          <li>
            <span class="name">{g.title}</span>
            <button class="btn btn-quiet" onclick={() => unhide(g.appName)}>Show in library</button>
          </li>
        {/each}
      </ul>
    {/if}
    {#if error}<p class="msg err">{error}</p>{/if}
    <div class="actions">
      <button class="btn btn-quiet" onclick={() => (ui.dialog = null)}>Close</button>
    </div>
  {/if}
</Modal>

<style>
  .field {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin: 8px 0;
  }
  .field span {
    font-size: 13px;
    color: var(--text-2);
  }
  .hint {
    margin: 0 0 8px;
    font-size: 12.5px;
    color: var(--text-2);
  }
  .msg {
    margin: 8px 0 12px;
    color: var(--text-2);
  }
  .err {
    color: var(--danger);
  }
  .ok {
    color: var(--success);
  }
  .list {
    list-style: none;
    margin: 4px 0 0;
    padding: 0;
    border-radius: var(--radius-md);
    background: var(--surface-inset);
    max-height: 320px;
    overflow-y: auto;
  }
  .list li {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 6px 8px 6px 16px;
  }
  .list li + li {
    border-top: 1px solid var(--border);
  }
  .name {
    flex: 1;
    min-width: 0;
    font-weight: 500;
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 16px;
  }
</style>
