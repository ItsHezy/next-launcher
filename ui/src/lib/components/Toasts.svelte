<script lang="ts">
  import { shift, slide } from '../motion';
  import { ui } from '../state/ui.svelte';
</script>

<div class="toasts" role="status" aria-live="polite">
  {#each ui.toasts as t (t.id)}
    <div class="toast" out:slide animate:shift>{t.text}</div>
  {/each}
</div>

<style>
  .toasts {
    position: fixed;
    left: 50%;
    bottom: 24px;
    transform: translateX(-50%);
    display: flex;
    flex-direction: column;
    gap: 8px;
    align-items: center;
    z-index: 100;
    pointer-events: none;
  }
  .toast {
    max-width: 520px;
    padding: 12px 18px;
    border-radius: 999px;
    background: #3a3c40;
    border: 1px solid var(--border);
    box-shadow: 0 12px 32px rgba(0, 0, 0, 0.4);
    animation: up var(--dur) var(--ease-spring);
  }
  @keyframes up {
    from {
      opacity: 0;
      transform: translateY(8px);
    }
  }
</style>
