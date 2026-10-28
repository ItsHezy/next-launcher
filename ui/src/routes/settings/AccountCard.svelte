<script lang="ts">
  import Avatar from '../../lib/components/Avatar.svelte';
  import { library } from '../../lib/state/library.svelte';

  const acct = $derived(library.account);
</script>

<div class="card">
  <Avatar name={acct?.displayName ?? 'Guest'} size={64} />
  <div class="who">
    <div class="name">{acct?.displayName ?? 'Not signed in'}</div>
    {#if acct?.email}<div class="email">{acct.email}</div>{/if}
    <div class="state" class:on={acct?.signedIn}>
      <span class="dot"></span>{acct?.signedIn ? 'Signed in (sample account)' : 'Signed out'}
    </div>
  </div>
</div>

<style>
  .card {
    display: flex;
    align-items: center;
    gap: 18px;
    padding: 20px;
    margin-bottom: 20px;
    border-radius: var(--radius-md);
    background: var(--surface-inset);
  }
  .name {
    font-size: 20px;
    font-weight: 700;
  }
  .email {
    color: var(--text-2);
  }
  .state {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-top: 4px;
    font-size: 12.5px;
    color: var(--text-3);
  }
  .state.on {
    color: var(--success);
  }
  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: currentColor;
  }
</style>
