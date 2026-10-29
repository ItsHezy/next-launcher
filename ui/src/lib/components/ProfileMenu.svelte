<script lang="ts">
  import { menuKeys } from '../actions';
  import { account } from '../state/account.svelte';
  import { auth } from '../state/auth.svelte';
  import { router, type SettingsSection } from '../state/router.svelte';
  import { comingSoon, ui } from '../state/ui.svelte';
  import type { IconName } from '../icons';
  import Avatar from './Avatar.svelte';
  import Icon from './Icon.svelte';

  const name = $derived(auth.displayName || 'Epic account');

  function open(section: SettingsSection) {
    ui.close();
    router.go('settings', section);
  }
  function redeem() {
    ui.close();
    if (account.caps?.redeem) ui.dialog = { kind: 'account', what: 'redeem' };
    else void auth.openEpicPage('redeem');
  }

  interface Item {
    icon: IconName;
    label: string;
    sub?: string;
    run: () => void;
    danger?: boolean;
  }
  const groups: Item[][] = [
    [
      {
        icon: 'trophy',
        label: 'Achievements',
        sub: 'Per game; a list is coming',
        run: () => {
          ui.close();
          comingSoon('Achievements', 9);
        },
      },
    ],
    [
      { icon: 'user', label: 'Account', sub: 'Profile, security, connections', run: () => open('account') },
      { icon: 'gift', label: 'Gifts & coupons', run: () => open('wallet') },
      { icon: 'ticket', label: 'Redeem code', run: redeem },
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
      <!-- Next Launcher doesn't connect to Epic's chat/presence service (ADR 0011), so
           friends see this account as offline while it's in use. Say so plainly. -->
      <span class="status" title="Online status needs Epic's chat connection, which Next Launcher doesn't use yet">
        <span class="dot invisible" aria-hidden="true"></span>
        <span>Shown offline to friends</span>
      </span>
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
    display: inline-flex;
    align-items: center;
    gap: 6px;
    margin-top: 2px;
    font-size: 13px;
    color: var(--text-2);
  }
  .dot {
    width: 9px;
    height: 9px;
    border-radius: 50%;
    background: var(--success);
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
