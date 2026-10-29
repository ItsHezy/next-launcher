<script lang="ts">
  import Avatar from '../../lib/components/Avatar.svelte';
  import { auth } from '../../lib/state/auth.svelte';
  import { library } from '../../lib/state/library.svelte';

  const s = $derived(auth.status);
  const email = $derived(library.account?.email ?? null);
  const line = $derived(
    s.state === 'signedIn'
      ? 'Signed in'
      : s.state === 'restoring'
        ? 'Reconnecting…'
        : s.state === 'offline'
          ? "Offline (can't reach Epic)"
          : s.state === 'needsAction'
            ? 'Epic needs you to confirm something'
            : s.state === 'signingIn'
              ? 'Signing in…'
              : 'Signed out',
  );
</script>

<div class="card">
  {#if auth.hasAccount}
    <Avatar name={auth.displayName || 'Epic account'} size={64} />
    <div class="who">
      <div class="name">{auth.displayName || 'Epic account'}</div>
      {#if email}<div class="email">{email}</div>{/if}
      <div class="state" class:on={s.state === 'signedIn'} class:warn={s.state === 'offline' || s.state === 'needsAction'}>
        <span class="dot"></span>{line}
      </div>
    </div>
    <button class="btn btn-danger out" onclick={() => auth.signOut()}>Sign out</button>
  {:else}
    <div class="who">
      <div class="name">{line}</div>
      <div class="email">Sign in with your Epic Games account. You sign in on Epic's own page; Next Launcher never sees your password.</div>
    </div>
    <button class="btn btn-primary" disabled={s.state === 'signingIn'} onclick={() => auth.signIn()}>Sign in</button>
  {/if}
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
  .who {
    flex: 1;
    min-width: 0;
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
  .state.warn {
    color: var(--warning);
  }
  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: currentColor;
  }
  .out {
    flex: none;
  }
</style>
