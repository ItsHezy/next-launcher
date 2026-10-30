<script lang="ts">
  // Epic's own page, drawn by a native child view over this box (ADR 0016). Rust can't see
  // our layout, so the box reports where it is; the view steps aside while a menu or dialog
  // is open, because it always draws above the app's own content.
  import { commands, type EpicPage, type PaneBounds } from '../../lib/bindings';
  import { settings } from '../../lib/state/settings.svelte';
  import { ui } from '../../lib/state/ui.svelte';

  let { page }: { page: EpicPage } = $props();

  let box = $state<HTMLElement>();
  let failed = $state<string | null>(null);
  const covered = $derived(ui.popover !== null || ui.dialog !== null);

  function bounds(): PaneBounds | null {
    if (!box) return null;
    const r = box.getBoundingClientRect();
    return { x: r.left, y: r.top, width: r.width, height: r.height };
  }

  // A new page: navigate the view (creating it the first time). Rust keeps the view hidden
  // until the page has loaded, so the spinner below shows meanwhile.
  $effect(() => {
    const p = page;
    const b = bounds();
    if (!b) return;
    failed = null;
    void commands.webPaneOpen(p, b, settings.get('general.accent')).then((res) => {
      if (res.status === 'error') failed = res.error.message;
      else if (covered) void commands.webPaneVisible(false);
    });
  });

  // Follow the box when the window or the layout changes.
  $effect(() => {
    if (!box) return;
    let frame = 0;
    const follow = () => {
      cancelAnimationFrame(frame);
      frame = requestAnimationFrame(() => {
        const b = bounds();
        if (b) void commands.webPaneBounds(b);
      });
    };
    const ro = new ResizeObserver(follow);
    ro.observe(box);
    window.addEventListener('resize', follow);
    return () => {
      cancelAnimationFrame(frame);
      ro.disconnect();
      window.removeEventListener('resize', follow);
    };
  });

  $effect(() => {
    void commands.webPaneVisible(!covered);
  });

  // Leaving the page closes the view.
  $effect(() => () => void commands.webPaneClose());
</script>

<div class="pane" bind:this={box}>
  {#if failed}
    <p class="note">{failed}</p>
  {:else}
    <span class="spinner" role="status" aria-label="Loading"></span>
  {/if}
</div>

<style>
  .pane {
    flex: 1;
    min-height: 0;
    /* The native view is square-cornered: keep it clear of the panel's rounded corners. */
    margin: 0 12px 12px;
    display: grid;
    place-items: center;
  }
  .note {
    color: var(--text-2);
  }
  .spinner {
    width: 28px;
    height: 28px;
    border-radius: 50%;
    border: 3px solid var(--selected);
    border-top-color: var(--accent-text);
    animation: spin 0.8s linear infinite;
  }
  @keyframes spin {
    to {
      transform: rotate(1turn);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .spinner {
      animation-duration: 2.4s;
    }
  }
</style>
