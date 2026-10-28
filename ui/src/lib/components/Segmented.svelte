<script lang="ts" generics="T extends string">
  interface Option {
    value: T;
    label: string;
    count?: number;
  }
  let {
    options,
    value,
    onchange,
    label,
    size = 'md',
  }: { options: Option[]; value: T; onchange: (v: T) => void; label: string; size?: 'md' | 'sm' } = $props();

  let el: HTMLDivElement;

  function key(e: KeyboardEvent) {
    const i = options.findIndex((o) => o.value === value);
    let n = -1;
    if (e.key === 'ArrowRight') n = (i + 1) % options.length;
    else if (e.key === 'ArrowLeft') n = (i - 1 + options.length) % options.length;
    else if (e.key === 'Home') n = 0;
    else if (e.key === 'End') n = options.length - 1;
    if (n < 0) return;
    e.preventDefault();
    const next = options[n];
    if (!next) return;
    onchange(next.value);
    el.querySelectorAll<HTMLButtonElement>('button')[n]?.focus();
  }
</script>

<div class="seg {size}" role="tablist" aria-label={label} bind:this={el} onkeydown={key} tabindex="-1">
  {#each options as o (o.value)}
    <button
      role="tab"
      aria-selected={o.value === value}
      tabindex={o.value === value ? 0 : -1}
      onclick={() => onchange(o.value)}
    >
      {o.label}
      {#if o.count !== undefined}<span class="count">{o.count}</span>{/if}
    </button>
  {/each}
</div>

<style>
  .seg {
    display: inline-flex;
    gap: 2px;
    padding: 3px;
    background: var(--control);
    border-radius: 999px;
  }
  button {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    min-height: var(--hit);
    padding: 0 16px;
    border-radius: 999px;
    color: var(--text-2);
    font-weight: 500;
    white-space: nowrap;
    transition:
      background-color var(--dur-fast) var(--ease-out),
      color var(--dur-fast) var(--ease-out);
  }
  .sm button {
    padding: 0 14px;
  }
  button:hover {
    color: var(--text);
  }
  button[aria-selected='true'] {
    background: var(--selected-strong);
    color: var(--text);
    font-weight: 600;
  }
  .count {
    font-size: 12px;
    font-variant-numeric: tabular-nums;
    color: inherit;
    opacity: 0.85;
  }
</style>
