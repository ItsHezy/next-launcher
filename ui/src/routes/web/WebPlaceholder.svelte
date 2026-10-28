<script lang="ts">
  import Icon from '../../lib/components/Icon.svelte';
  import type { IconName } from '../../lib/icons';
  import { comingSoon } from '../../lib/state/ui.svelte';

  let { title, body, icon }: { title: string; body: string; icon: IconName } = $props();
</script>

<div class="page">
  <div class="card panel">
    <span class="ic"><Icon name={icon} size={32} /></span>
    <h1>{title}</h1>
    <p>{body}</p>
    <button class="btn btn-primary" onclick={() => comingSoon(`${title} (signed-in web view)`, 7)}>
      Open {title} <Icon name="external" size={16} />
    </button>
  </div>
</div>

<style>
  .page {
    height: 100%;
    display: grid;
    place-items: center;
    padding: 32px;
  }
  .card {
    max-width: 460px;
    padding: 36px;
    text-align: center;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 14px;
  }
  .ic {
    display: grid;
    place-items: center;
    width: 64px;
    height: 64px;
    border-radius: 18px;
    background: var(--control);
    color: var(--accent-text);
  }
  p {
    color: var(--text-2);
  }
</style>
