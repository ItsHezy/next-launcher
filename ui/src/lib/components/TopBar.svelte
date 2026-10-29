<script lang="ts">
  import { dismiss } from '../actions';
  import { commands, type StoreTarget } from '../bindings';
  import { wallet } from '../mock';
  import { auth } from '../state/auth.svelte';
  import { library } from '../state/library.svelte';
  import { router, type Route } from '../state/router.svelte';
  import { store } from '../state/store.svelte';
  import { ui } from '../state/ui.svelte';
  import Avatar from './Avatar.svelte';
  import Icon from './Icon.svelte';
  import ProfileMenu from './ProfileMenu.svelte';
  import Segmented from './Segmented.svelte';
  import WalletPopover from './WalletPopover.svelte';
  import WindowControls from './WindowControls.svelte';

  const tabs: { value: Exclude<Route, 'settings'>; label: string }[] = [
    { value: 'library', label: 'Library' },
    { value: 'store', label: 'Store' },
    { value: 'unreal', label: 'Unreal Engine' },
    { value: 'fab', label: 'Fab' },
  ];

  let searching = $state(false);
  let searchInput = $state<HTMLInputElement>();
  let walletBtn = $state<HTMLButtonElement>();
  let profileBtn = $state<HTMLButtonElement>();

  const name = $derived(auth.displayName || 'Epic account');

  // Search follows the section: the Store searches Epic's store, everything else the library.
  const onStore = $derived(router.route === 'store');
  const query = $derived(onStore ? store.query : library.query);

  function openSearch() {
    searching = true;
    if (router.route !== 'library' && router.route !== 'store') router.go('library');
    queueMicrotask(() => searchInput?.focus());
  }
  function setQuery(q: string, now = false) {
    if (onStore) store.setQuery(q, now);
    else library.query = q;
  }
  function closeSearch() {
    if (store.query) store.setQuery('', true);
    library.query = '';
    searching = false;
  }
  async function openEpic(target: StoreTarget) {
    const res = await commands.storeOpen(target);
    if (res.status === 'error') ui.toast(res.error.message);
  }
</script>

<header class="topbar panel" data-tauri-drag-region>
  <nav aria-label="Sections">
    <Segmented
      label="Sections"
      options={tabs}
      value={router.route === 'settings' ? 'library' : router.route}
      onchange={(v) => router.go(v)}
    />
  </nav>

  <div class="spacer" data-tauri-drag-region></div>

  <div class="actions">
    {#if searching}
      <div class="search">
        <Icon name="search" size={18} />
        <input
          bind:this={searchInput}
          value={query}
          oninput={(e) => setQuery(e.currentTarget.value)}
          type="search"
          placeholder={onStore ? 'Search the store' : 'Search your library'}
          aria-label={onStore ? 'Search the store' : 'Search your library'}
          onkeydown={(e) => {
            if (e.key === 'Escape') closeSearch();
            else if (e.key === 'Enter') setQuery(e.currentTarget.value, true);
          }}
          onblur={() => !query && (searching = false)}
        />
      </div>
    {:else}
      <button class="icon-btn" aria-label="Search" onclick={openSearch}><Icon name="search" /></button>
    {/if}
    <button class="icon-btn" aria-label="Wishlist (opens Epic's page)" onclick={() => openEpic({ kind: 'wishlist' })}>
      <Icon name="heart" />
    </button>
    <button class="icon-btn" aria-label="Cart (opens Epic's page)" onclick={() => openEpic({ kind: 'cart' })}><Icon name="cart" /></button>

    {#if auth.hasAccount}
    <div class="anchor">
      <button
        bind:this={walletBtn}
        class="wallet-btn"
        aria-haspopup="dialog"
        aria-expanded={ui.popover === 'wallet'}
        aria-label={wallet.balance ? `Wallet, balance ${wallet.balance}` : 'Wallet'}
        onclick={() => ui.toggle('wallet')}
      >
        <Icon name="wallet" size={18} />
        <span>{wallet.balance ?? 'Wallet'}</span>
      </button>
      {#if ui.popover === 'wallet'}
        <div use:dismiss={{ onclose: () => ui.close(), trigger: () => walletBtn }}>
          <WalletPopover />
        </div>
      {/if}
    </div>
    {/if}

    <span class="divider" aria-hidden="true"></span>

    <button
      class="icon-btn"
      aria-label="Settings"
      aria-pressed={router.route === 'settings'}
      onclick={() => router.go('settings', 'general')}
    >
      <Icon name="gear" />
    </button>

    {#if auth.hasAccount}
      <div class="anchor">
        <button
          bind:this={profileBtn}
          class="avatar-btn"
          aria-haspopup="menu"
          aria-expanded={ui.popover === 'profile'}
          aria-label="Profile menu for {name}"
          onclick={() => ui.toggle('profile')}
        >
          <Avatar {name} size={34} />
          {#if auth.status.state === 'offline' || auth.status.state === 'needsAction'}
            <span class="badge" aria-hidden="true"></span>
          {/if}
        </button>
        {#if ui.popover === 'profile'}
          <div use:dismiss={{ onclose: () => ui.close(), trigger: () => profileBtn }}>
            <ProfileMenu />
          </div>
        {/if}
      </div>
    {:else}
      <button class="btn btn-primary sign-in" disabled={auth.status.state === 'signingIn'} onclick={() => auth.signIn()}>
        {auth.status.state === 'signingIn' ? 'Signing in…' : 'Sign in'}
      </button>
    {/if}

    <span class="divider" aria-hidden="true"></span>
    <WindowControls />
  </div>
</header>

<style>
  .topbar {
    display: flex;
    align-items: center;
    gap: 12px;
    min-height: 64px;
    padding: 8px 8px 8px 10px;
    border-radius: var(--radius-lg);
  }
  .spacer {
    flex: 1;
    align-self: stretch;
  }
  .actions {
    display: flex;
    align-items: center;
    gap: 4px;
  }
  .anchor {
    position: relative;
  }
  .search {
    display: flex;
    align-items: center;
    gap: 8px;
    padding-left: 12px;
    background: var(--control);
    border-radius: 999px;
    color: var(--text-2);
    animation: grow var(--dur) var(--ease-spring);
  }
  .search input {
    width: 220px;
    border: 0;
    background: transparent;
    padding-left: 0;
  }
  @keyframes grow {
    from {
      opacity: 0;
      transform: scaleX(0.9);
    }
  }
  .wallet-btn {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    padding: 0 14px;
    margin: 0 4px;
    border-radius: 999px;
    background: var(--control);
    color: var(--text);
    font-weight: 600;
    font-variant-numeric: tabular-nums;
  }
  .wallet-btn :global(svg) {
    color: var(--text-2);
  }
  .wallet-btn:hover,
  .wallet-btn[aria-expanded='true'] {
    background: var(--selected);
  }
  .divider {
    width: 1px;
    height: 24px;
    margin: 0 6px;
    background: var(--border);
  }
  .avatar-btn {
    position: relative;
    display: grid;
    place-items: center;
    width: var(--hit);
    border-radius: 50%;
  }
  .badge {
    position: absolute;
    right: 4px;
    top: 6px;
    width: 10px;
    height: 10px;
    border-radius: 50%;
    background: var(--warning);
    box-shadow: 0 0 0 2px var(--surface);
  }
  .sign-in {
    margin-left: 4px;
    padding: 0 20px;
  }
  .sign-in:disabled {
    opacity: 0.7;
  }
</style>
