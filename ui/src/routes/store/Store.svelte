<script lang="ts">
  import Icon from '../../lib/components/Icon.svelte';
  import { storeItems, type StoreItem } from '../../lib/mock';
  import { comingSoon } from '../../lib/state/ui.svelte';

  type Chip = StoreItem['tags'][number];
  const chips: { value: Chip; label: string }[] = [
    { value: 'featured', label: 'Featured' },
    { value: 'free', label: 'Free games' },
    { value: 'sale', label: 'On sale' },
    { value: 'new', label: 'New releases' },
  ];
  let chip = $state<Chip>('featured');
  const items = $derived(storeItems.filter((i) => i.tags.includes(chip)));
</script>

<div class="page">
  <header class="head">
    <h1>Store</h1>
    <span class="note">Sample data. The live store arrives in Phase 6.</span>
  </header>

  <div class="chips" role="toolbar" aria-label="Store categories">
    {#each chips as c (c.value)}
      <button class="chip" aria-pressed={chip === c.value} onclick={() => (chip = c.value)}>{c.label}</button>
    {/each}
  </div>

  <ul class="grid" aria-label="{chips.find((c) => c.value === chip)?.label} games">
    {#each items as it (it.id)}
      <li>
        <button class="card" onclick={() => comingSoon('Product pages', 6)} aria-label="{it.title}, {it.price}">
          <span
            class="art"
            style:background="linear-gradient(160deg, hsl({it.hue} 45% 40%), hsl({(it.hue + 50) % 360} 50% 18%))"
          >
            <span class="ph" aria-hidden="true">{it.title}</span>
            {#if it.discount}<span class="badge">−{it.discount}%</span>{/if}
          </span>
          <span class="name">{it.title}</span>
          <span class="seller">{it.seller}</span>
          <span class="price">
            {#if it.original}<s>{it.original}</s>{/if}
            <strong class:free={it.free}>{it.free ? 'Free now' : it.price}</strong>
          </span>
        </button>
      </li>
    {/each}
  </ul>

  <p class="foot"><Icon name="info" size={16} /> Checkout will always open Epic's own page, signed in, where you confirm the purchase.</p>
</div>

<style>
  .page {
    height: 100%;
    overflow-y: auto;
    padding: 28px 32px 48px;
    scrollbar-gutter: stable;
  }
  .head {
    display: flex;
    align-items: baseline;
    gap: 16px;
    margin-bottom: 20px;
  }
  .note {
    color: var(--text-2);
  }
  .chips {
    display: flex;
    gap: 8px;
    margin-bottom: 24px;
    flex-wrap: wrap;
  }
  .chip {
    padding: 0 18px;
    border-radius: 999px;
    background: var(--control);
    color: var(--text-2);
    font-weight: 500;
    transition: background-color var(--dur-fast) var(--ease-out);
  }
  .chip:hover {
    color: var(--text);
  }
  .chip[aria-pressed='true'] {
    background: var(--selected-strong);
    color: var(--text);
    font-weight: 600;
  }
  .grid {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(168px, 1fr));
    gap: 28px 20px;
  }
  .card {
    display: flex;
    flex-direction: column;
    align-items: stretch;
    gap: 4px;
    width: 100%;
    text-align: left;
    border-radius: var(--radius-md);
  }
  .art {
    position: relative;
    display: block;
    aspect-ratio: 3 / 4;
    border-radius: var(--radius-md);
    overflow: hidden;
    margin-bottom: 8px;
    transition: transform var(--dur) var(--ease-spring);
  }
  .card:hover .art {
    transform: translateY(-3px);
  }
  .ph {
    position: absolute;
    inset: auto 12px 12px;
    font-weight: 700;
    font-size: 17px;
    line-height: 1.15;
    color: rgba(255, 255, 255, 0.92);
  }
  .badge {
    position: absolute;
    top: 10px;
    left: 10px;
    padding: 3px 8px;
    border-radius: 8px;
    background: var(--accent-fill);
    color: #fff;
    font-size: 12.5px;
    font-weight: 700;
  }
  .name {
    font-weight: 600;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .seller {
    font-size: 12.5px;
    color: var(--text-3);
  }
  .price {
    display: flex;
    gap: 8px;
    align-items: baseline;
    font-variant-numeric: tabular-nums;
  }
  s {
    color: var(--text-3);
    font-size: 12.5px;
  }
  .free {
    color: var(--success);
  }
  .foot {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-top: 36px;
    color: var(--text-2);
  }
</style>
