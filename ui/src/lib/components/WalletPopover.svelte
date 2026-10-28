<script lang="ts">
  import { wallet } from '../mock';
  import { router } from '../state/router.svelte';
  import { comingSoon, ui } from '../state/ui.svelte';
  import Icon from './Icon.svelte';

  let first: HTMLButtonElement;
  $effect(() => first?.focus());

  function web(what: string) {
    comingSoon(`${what} (signed-in Epic page)`, 7);
  }
</script>

<div class="menu wallet" role="dialog" aria-label="Wallet">
  <div class="balance">
    <span class="label">Wallet balance</span>
    <span class="amount">{wallet.balance}</span>
  </div>
  <div class="row">
    <Icon name="sparkle" size={18} />
    <span>Epic Rewards</span>
    <span class="val">{wallet.rewards}</span>
  </div>
  <div class="actions">
    <button class="btn btn-primary" bind:this={first} onclick={() => web('Add funds')}>Add funds</button>
    <button class="btn btn-quiet" onclick={() => web('Redeem card')}>Redeem card</button>
  </div>
  <div class="sep"></div>
  <button class="link" onclick={() => web('Transaction history')}>
    Transaction history <Icon name="external" size={16} />
  </button>
  <button class="link" onclick={() => web('Payment methods')}>
    Payment methods <Icon name="external" size={16} />
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
