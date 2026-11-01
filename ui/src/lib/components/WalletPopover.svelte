<script lang="ts">
  import type { EpicPage } from '../bindings';
  import { num } from '../boot';
  import { drop } from '../motion';
  import { account } from '../state/account.svelte';
  import { auth } from '../state/auth.svelte';
  import { router } from '../state/router.svelte';
  import { ui } from '../state/ui.svelte';
  import Icon from './Icon.svelte';

  let first: HTMLButtonElement;
  $effect(() => first?.focus());
  // Fresh numbers when opened (Rust answers from its 5-minute cache).
  $effect(() => {
    void account.loadWallet();
  });

  const w = $derived(account.wallet);
  const coupons = $derived(w?.coupons == null ? null : num(w.coupons));

  function web(page: EpicPage) {
    ui.close();
    auth.openEpicPage(page);
  }
</script>

<div class="menu wallet" out:drop|global role="dialog" aria-label="Wallet">
  <div class="balance">
    <span class="label">Wallet balance</span>
    <span class="amount">{w?.balance ?? '—'}</span>
    {#if w && w.available && !w.balance}
      <span class="note">Epic's wallet isn't offered in your region.</span>
    {:else if w && !w.available}
      <span class="note">Your balance is on Epic's wallet page.</span>
    {/if}
  </div>
  <div class="row">
    <Icon name="sparkle" size={18} />
    <span>Epic Rewards</span>
    <span class="val">{w?.rewards ?? '—'}</span>
  </div>
  {#if coupons !== null && coupons > 0}
    <p class="note coupons">{coupons} coupon{coupons === 1 ? '' : 's'}, applied at checkout</p>
  {/if}
  <div class="actions">
    <button class="btn btn-primary" bind:this={first} onclick={() => web('wallet')}>Add funds</button>
    <button class="btn btn-quiet" onclick={() => web('redeem')}>Redeem card</button>
  </div>
  <div class="sep"></div>
  <button class="link" onclick={() => web('transactions')}>
    Transaction history <Icon name="chevronRight" size={16} />
  </button>
  <button class="link" onclick={() => web('paymentMethods')}>
    Payment methods <Icon name="chevronRight" size={16} />
  </button>
  <button
    class="link"
    onclick={() => {
      ui.close();
      router.go('settings', 'wallet');
    }}
  >
    Wallet settings <Icon name="chevronRight" size={16} />
  </button>
</div>

<style>
  .wallet {
    right: 0;
    top: calc(100% + 8px);
    width: 300px;
    padding: 16px;
  }
  .balance {
    display: flex;
    flex-direction: column;
    gap: 2px;
    margin-bottom: 12px;
  }
  .label {
    color: var(--text-2);
    font-size: 13px;
  }
  .note {
    color: var(--text-2);
    font-size: 12.5px;
  }
  .coupons {
    margin: 10px 2px 0;
  }
  .amount {
    font-size: 28px;
    font-weight: 700;
    letter-spacing: -0.02em;
    font-variant-numeric: tabular-nums;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 12px;
    border-radius: var(--radius-sm);
    background: var(--surface-inset);
    color: var(--text-2);
  }
  .row span:first-of-type {
    color: var(--text);
  }
  .val {
    margin-left: auto;
    color: var(--success);
    font-weight: 600;
    font-variant-numeric: tabular-nums;
  }
  .actions {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 8px;
    margin-top: 14px;
  }
  .sep {
    height: 1px;
    background: var(--border);
    margin: 14px 0 6px;
  }
  .link {
    display: flex;
    align-items: center;
    justify-content: space-between;
    width: 100%;
    padding: 0 8px;
    border-radius: var(--radius-sm);
    color: var(--text);
    text-align: left;
  }
  .link :global(svg) {
    color: var(--text-2);
  }
  .link:hover {
    background: var(--hover);
  }
</style>
