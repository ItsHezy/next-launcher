<script lang="ts">
  import { menuKeys } from '../actions';
  import { auth } from '../state/auth.svelte';
  import { router, type SettingsSection } from '../state/router.svelte';
  import { comingSoon, ui } from '../state/ui.svelte';
  import type { IconName } from '../icons';
  import Avatar from './Avatar.svelte';
  import Icon from './Icon.svelte';

  let status = $state<'online' | 'away' | 'invisible'>('online');
  const name = $derived(auth.displayName || 'Epic account');

  function open(section: SettingsSection) {
    ui.close();
    router.go('settings', section);
  }

  interface Item {
    icon: IconName;
    label: string;
    sub?: string;
    run: () => void;
    danger?: boolean;
  }
  const groups: Item[][] = [
    [{ icon: 'trophy', label: 'Achievements', run: () => comingSoon('Achievements', 7) }],
    [
      { icon: 'user', label: 'Account', sub: 'Profile, security, connections', run: () => open('account') },
      { icon: 'gift', label: 'Gifts & coupons', run: () => open('wallet') },
      { icon: 'ticket', label: 'Redeem code', run: () => open('wallet') },
    ],
    [
      { icon: 'gear', label: 'Settings', run: () => open('general') },
      { icon: 'help', label: 'Help & support', run: () => open('support') },
      { icon: 'document', label: 'Legal & policies', run: () => open('legal') },
    ],
    [
      {
        icon: 'logout',
        label: 'Sign out',
        danger: true,
        run: () => {
          ui.close();
          void auth.signOut();
        },
      },
    ],
  ];
</script>

<div class="menu profile" role="menu" aria-label="Profile" use:menuKeys>
  <div class="head">
    <Avatar {name} size={44} />
    <div class="who">
      <div class="name">{name}</div>
      <label class="status">
        <span class="dot {status}" aria-hidden="true"></span>
        <span class="sr-only">Online status</span>
        <select bind:value={status} onchange={() => comingSoon('Online status', 7)}>
          <option value="online">Online</option>
          <option value="away">Away</option>
          <option value="invisible">Invisible</option>
        </select>
      </label>
    </div>
  </div>

  {#each groups as group, gi (gi)}
    <div class="sep" role="separator"></div>
    {#each group as item (item.label)}
      <button role="menuitem" class="item" class:danger={item.danger} onclick={item.run}>
        <span class="ic"><Icon name={item.icon} size={20} /></span>
        <span class="txt">
          <span class="label">{item.label}</span>
          {#if item.sub}<span class="sub">{item.sub}</span>{/if}
        </span>
      </button>
    {/each}
  {/each}
</div>

<style>
  .profile {
    right: 0;
    top: calc(100% + 8px);
    width: 320px;
  }
  .head {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 8px 8px 10px;
  }
  .name {
    font-weight: 600;
    font-size: 15px;
  }
  .status {
    position: relative;
    display: inline-flex;
    align-items: center;
    gap: 6px;
    color: var(--text-2);
  }
  .status select {
    appearance: none;
    background: transparent;
    border: 0;
    color: var(--text-2);
    min-height: var(--hit);
    padding: 0 18px 0 0;
    cursor: pointer;
    background-image: linear-gradient(45deg, transparent 50%, currentColor 50%),
      linear-gradient(135deg, currentColor 50%, transparent 50%);
    background-position:
      calc(100% - 7px) 55%,
      calc(100% - 3px) 55%;
    background-size: 4px 4px;
    background-repeat: no-repeat;
  }
  .status select:focus-visible {
    outline: 2px solid var(--accent-text);
    border-radius: 6px;
  }
  .status option {
    background: var(--surface);
    color: var(--text);
  }
  .dot {
    width: 9px;
    height: 9px;
    border-radius: 50%;
    background: var(--success);
  }
  .dot.away {
    background: var(--warning);
  }
  .dot.invisible {
    background: transparent;
    border: 2px solid var(--text-2);
  }
  .sep {
    height: 1px;
    margin: 6px 8px;
    background: var(--border);
  }
  .item {
    display: flex;
    align-items: center;
    gap: 12px;
    width: 100%;
    min-height: var(--hit);
    padding: 6px 10px;
    border-radius: var(--radius-sm);
    text-align: left;
    color: var(--text);
  }
  .item:hover,
  .item:focus-visible {
    background: var(--hover);
  }
  .item:focus-visible {
    outline-offset: -2px;
  }
  .ic {
    display: grid;
    place-items: center;
    color: var(--text-2);
  }
  .txt {
    display: flex;
    flex-direction: column;
  }
  .label {
    font-weight: 500;
  }
  .sub {
    font-size: 12.5px;
    color: var(--text-2);
  }
  .danger,
  .danger .ic {
    color: var(--danger);
  }
</style>
