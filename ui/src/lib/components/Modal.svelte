<script lang="ts">
  import type { Snippet } from 'svelte';
  import { fade, pop } from '../motion';
  import Icon from './Icon.svelte';

  let {
    title,
    onclose,
    busy = false,
    width = 560,
    children,
  }: { title: string; onclose: () => void; busy?: boolean; width?: number; children: Snippet } = $props();

  let box = $state<HTMLDivElement>();
  const id = `dlg-${Math.random().toString(36).slice(2, 8)}`;
  // Focus moves into the dialog and returns to where it was when the dialog closes.
  $effect(() => {
    const before = document.activeElement as HTMLElement | null;
    box?.focus();
    return () => before?.focus?.();
  });

  /** Escape closes; Tab stays inside the dialog (aria-modal). */
  function onkey(e: KeyboardEvent) {
    if (e.key === 'Escape' && !busy) onclose();
    if (e.key !== 'Tab' || !box) return;
    const items = [...box.querySelectorAll<HTMLElement>('button, [href], input, select, textarea, [tabindex]:not([tabindex="-1"])')].filter(
      (el) => !el.hasAttribute('disabled') && el.offsetParent !== null,
    );
    if (items.length === 0) return;
    const first = items[0]!;
    const last = items[items.length - 1]!;
    const active = document.activeElement;
    if (e.shiftKey && (active === first || active === box)) {
      e.preventDefault();
      last.focus();
    } else if (!e.shiftKey && active === last) {
      e.preventDefault();
      first.focus();
    }
  }
</script>

<svelte:window onkeydown={onkey} />

<div class="backdrop" role="presentation" out:fade|global onclick={(e) => e.target === e.currentTarget && !busy && onclose()}>
  <div
    out:pop|global
    class="dialog"
    role="dialog"
    aria-modal="true"
    aria-labelledby={id}
    tabindex="-1"
    bind:this={box}
    style:width="min({width}px, calc(100vw - 48px))"
  >
    <header>
      <h2 {id}>{title}</h2>
      <button class="icon-btn" aria-label="Close" disabled={busy} onclick={onclose}><Icon name="close" size={18} /></button>
    </header>
    {@render children()}
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 150;
    display: grid;
    place-items: center;
    background: rgba(0, 0, 0, 0.5);
    animation: fade var(--dur) var(--ease-out);
  }
  .dialog {
    max-height: calc(100vh - 96px);
    overflow-y: auto;
    padding: 20px 24px 24px;
    border-radius: var(--radius-lg);
    background: var(--surface);
    border: 1px solid var(--border);
    box-shadow: 0 24px 80px rgba(0, 0, 0, 0.5);
    outline: none;
    animation: pop var(--dur) var(--ease-spring);
  }
  .dialog > :global(*) {
    animation: item-in var(--dur-slow) var(--ease-out) backwards;
  }
  .dialog > :global(:nth-child(2)) {
    animation-delay: 40ms;
  }
  .dialog > :global(:nth-child(3)) {
    animation-delay: 80ms;
  }
  .dialog > :global(:nth-child(n + 4)) {
    animation-delay: 110ms;
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    margin-bottom: 8px;
  }
  @keyframes fade {
    from {
      opacity: 0;
    }
  }
  @keyframes pop {
    from {
      opacity: 0;
      transform: scale(0.97);
    }
  }
</style>
