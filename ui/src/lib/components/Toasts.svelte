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
    position: relative;
    overflow: hidden;
    animation: up var(--dur-slow) var(--ease-spring);
  }
  /* How long it stays: a line that runs out (ui.toast keeps it 3.2 s). */
  .toast::after {
    content: '';
    position: absolute;
    left: 22px;
    right: 22px;
    bottom: 0;
    height: 2px;
    border-radius: 1px;
    background: var(--accent-text);
    opacity: 0.55;
    transform-origin: left;
    animation: run-out 3200ms linear forwards;
  }
  @keyframes up {
    from {
      opacity: 0;
      transform: translateY(12px) scale(0.92);
    }
  }
  @keyframes run-out {
    to {
      transform: scaleX(0);
    }
  }
</style>
