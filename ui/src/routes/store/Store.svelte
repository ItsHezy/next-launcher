<script lang="ts">
  import Icon from '../../lib/components/Icon.svelte';
  import WebPane from '../../lib/components/WebPane.svelte';
  import { router } from '../../lib/state/router.svelte';
  import { store, type Chip, type Offer } from '../../lib/state/store.svelte';
  import ProductView from './ProductView.svelte';
  import StoreCard from './StoreCard.svelte';

  const chips: { value: Chip; label: string }[] = [
    { value: 'featured', label: 'Featured' },
    { value: 'free', label: 'Free games' },
    { value: 'onSale', label: 'On sale' },
    { value: 'newReleases', label: 'New releases' },
  ];

  let scroller = $state<HTMLDivElement>();
  /** Epic's store page shown here (checkout, wishlist, cart...), if any. */
  const web = $derived(router.route === 'store' ? router.web : null);

  const sections = $derived.by(() => {
    const f = store.featured;
    if (!f) return [];
    const freeNow = f.free.filter((o) => !o.freeFrom || Date.parse(o.freeFrom) <= Date.now());
    const freeNext = f.free.filter((o) => o.freeFrom && Date.parse(o.freeFrom) > Date.now());
    return [
      { id: 'free', title: 'Free now', chip: 'free' as Chip, offers: [...freeNow, ...freeNext].slice(0, 6) },
      { id: 'sale', title: 'On sale', chip: 'onSale' as Chip, offers: f.onSale.slice(0, 6) },
      { id: 'new', title: 'New releases', chip: 'newReleases' as Chip, offers: f.newReleases.slice(0, 6) },
    ].filter((s) => s.offers.length > 0);
  });
  const heading = $derived(
    store.searching ? `Results for “${store.query.trim()}”` : chips.find((c) => c.value === store.chip)?.label,
  );
  const more = $derived(!store.searching && store.chip === 'free' ? false : store.offers.length < store.total);

  // First visit loads Featured; later visits keep what's there (Rust caches for 10 min).
  $effect(() => {
    if (!store.started) void store.load();
  });
  // Opening a product or switching chips starts at the top.
  $effect(() => {
    void store.product;
    void store.chip;
    scroller?.scrollTo({ top: 0 });
  });

  function openEpic() {
    if (store.searching) store.openEpic({ kind: 'search', query: store.query.trim() }, "Search on Epic's store");
    else store.openEpic({ kind: 'home' }, "Epic's store");
  }

  function onkey(e: KeyboardEvent) {
    if (e.key !== 'Escape' || !web || e.target instanceof HTMLInputElement) return;
    router.web = null;
  }
</script>

<svelte:window onkeydown={onkey} />

{#snippet grid(offers: Offer[], label: string)}
  <ul class="grid" aria-label={label}>
    {#each offers as o (o.namespace + o.id)}
      <li><StoreCard offer={o} /></li>
    {/each}
  </ul>
{/snippet}

{#if web}
  <div class="web panel">
    <div class="bar">
      <button class="up" onclick={() => (router.web = null)}>
        <Icon name="chevronLeft" size={20} />
        <span>{store.product?.title ?? 'Store'}</span>
      </button>
      <h2>{web.title}</h2>
    </div>
    <WebPane target={web.target} />
  </div>
{/if}

<!-- Kept while Epic's page shows, so going back returns to the same place. -->
<div class="page" hidden={!!web} bind:this={scroller}>
  {#if store.product}
    <ProductView offer={store.product} />
  {:else}
    <header class="head">
      <h1>Store</h1>
    </header>

    <div class="chips" role="toolbar" aria-label="Store categories">
      {#each chips as c (c.value)}
        <button
          class="chip"
          aria-pressed={!store.searching && store.chip === c.value}
          onclick={() => store.setChip(c.value)}>{c.label}</button
        >
      {/each}
    </div>

    {#if store.error}
      <div class="notice" role="alert">
        <p>{store.error.message}</p>
        {#if store.error.challenge}
          <button class="btn btn-primary" onclick={openEpic}>Open Epic's store</button>
        {:else}
          <button class="btn btn-quiet" onclick={() => store.load()}>Try again</button>
        {/if}
      </div>
    {:else if !store.searching && store.chip === 'featured'}
      {#if store.featured}
        {#each sections as s (s.id)}
          <section class="section" aria-labelledby="sec-{s.id}">
            <div class="sec-head">
              <h2 id="sec-{s.id}">{s.title}</h2>
              <button class="link" onclick={() => store.setChip(s.chip)}>See all <Icon name="chevronRight" size={16} /></button>
            </div>
            {@render grid(s.offers, s.title)}
          </section>
        {/each}
      {:else}
        <p class="empty">Loading the store…</p>
      {/if}
    {:else}
      <h2 class="sub">{heading}</h2>
      {#if store.offers.length === 0}
        <p class="empty">{store.loading ? 'Loading…' : store.searching ? 'No games match your search.' : 'Nothing here right now.'}</p>
      {:else}
        {@render grid(store.offers, heading ?? 'Games')}
        {#if more}
          <div class="more">
            <button class="btn btn-quiet" disabled={store.loading} onclick={() => store.load(true)}>
              {store.loading ? 'Loading…' : 'Show more'}
            </button>
          </div>
        {/if}
      {/if}
    {/if}

    <p class="foot"><Icon name="info" size={16} /> Checkout is Epic's own page, signed in, where you confirm the purchase.</p>
  {/if}
</div>

<style>
  .web {
    height: 100%;
    display: flex;
    flex-direction: column;
  }
  .bar {
    display: flex;
    align-items: center;
    gap: 10px;
    min-height: 64px;
    padding: 8px 20px 8px 14px;
  }
  .up {
    display: inline-flex;
    align-items: center;
    gap: 2px;
    min-height: var(--hit);
    max-width: 40%;
    padding: 0 12px 0 6px;
    border-radius: var(--radius-sm);
    color: var(--accent-text);
    font-weight: 500;
    flex: none;
  }
  .up span {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .up:hover {
    background: var(--hover);
  }
  .bar h2 {
    font-size: 22px;
    font-weight: 700;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
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
  .chips {
    display: flex;
    gap: 8px;
    margin-bottom: 24px;
    flex-wrap: wrap;
  }
  .chip {
    min-height: var(--hit);
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
  .section {
    margin-bottom: 36px;
  }
  .sec-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 14px;
  }
  h2 {
    margin: 0;
    font-size: 19px;
  }
  .sub {
    margin-bottom: 16px;
  }
  .link {
    display: inline-flex;
    align-items: center;
    gap: 2px;
    min-height: var(--hit);
    padding: 0 8px;
    color: var(--text-2);
    font-weight: 500;
    border-radius: var(--radius-sm);
  }
  .link:hover {
    color: var(--text);
  }
  .grid {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(168px, 1fr));
    gap: 28px 20px;
  }
  .grid li {
    min-width: 0;
  }
  .more {
    display: flex;
    justify-content: center;
    margin-top: 28px;
  }
  .empty {
    color: var(--text-2);
  }
  .notice {
    display: flex;
    align-items: center;
    gap: 16px;
    padding: 16px 18px;
    border-radius: var(--radius-md);
    background: var(--surface-inset);
  }
  .notice p {
    flex: 1;
    margin: 0;
    color: var(--text-2);
  }
  .foot {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-top: 36px;
    color: var(--text-2);
  }
</style>
