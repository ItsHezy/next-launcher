<script lang="ts">
  import { auth } from '../state/auth.svelte';
  import Icon from './Icon.svelte';

  const s = $derived(auth.status);
</script>

{#if s.state === 'signedOut'}
  <div class="banner" role="status">
    <Icon name="user" size={20} />
    <div class="txt">
      <strong>{s.reason ?? 'Sign in to Epic Games'}</strong>
      <span>Your library, downloads and cloud saves sync once you're signed in. You sign in on Epic's own page.</span>
    </div>
    <button class="btn btn-primary" onclick={() => auth.signIn()}>Sign in</button>
  </div>
{:else if s.state === 'signingIn'}
  <div class="banner" role="status">
    <Icon name="external" size={20} />
    <div class="txt">
      <strong>Finish signing in in the Epic Games window</strong>
      <span>Two-factor codes and security checks happen there, on Epic's page.</span>
    </div>
  </div>
{:else if s.state === 'offline'}
  <div class="banner warn" role="status">
    <Icon name="globe" size={20} />
    <div class="txt">
      <strong>Couldn't reach Epic's servers</strong>
      <span>Showing your saved library. Retrying in {s.retryInSecs} s.</span>
    </div>
  </div>
{:else if s.state === 'needsAction'}
  <div class="banner warn" role="status">
    <Icon name="shield" size={20} />
    <div class="txt">
      <strong>Epic needs you to confirm something</strong>
      <span>Epic asked you to complete a step on its website before your account can be used here.</span>
    </div>
    <button class="btn btn-primary" onclick={() => auth.openCorrectiveAction()}>Open Epic</button>
  </div>
{/if}

<style>
  .banner {
    display: flex;
    align-items: center;
    gap: 14px;
    margin-bottom: 24px;
    padding: 14px 16px 14px 18px;
    border-radius: var(--radius-md);
    background: var(--surface);
    color: var(--text-2);
  }
  .banner.warn :global(svg) {
    color: var(--warning);
  }
  .txt {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  strong {
    color: var(--text);
    font-weight: 600;
  }
  span {
    font-size: 13px;
  }
</style>
