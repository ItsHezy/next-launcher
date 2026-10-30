<script lang="ts">
  import { untrack } from 'svelte';
  import { commands, type StoreProduct_Serialize as Product } from '../../lib/bindings';
  import { num } from '../../lib/boot';
  import Icon from '../../lib/components/Icon.svelte';
  import Markdown from '../../lib/components/Markdown.svelte';
  import { library } from '../../lib/state/library.svelte';
  import { router } from '../../lib/state/router.svelte';
  import { freeLabel, store, type Offer } from '../../lib/state/store.svelte';

  let { offer }: { offer: Offer } = $props();

  let product = $state.raw<Product | null>(null);
  let error = $state<string | null>(null);
  let failed = $state(false);

  // Details refine what the grid already showed (price, ownership).
  const view = $derived(product?.offer ?? offer);
  const giveaway = $derived(freeLabel(offer));
  const freeNow = $derived(!!offer.freeFrom && Date.parse(offer.freeFrom) <= Date.now());
  const price = $derived(view.price);
  const free = $derived(freeNow || !!price?.free);
  const discount = $derived(num(price?.discount));
  const released = $derived(
    product?.releaseDate
      ? new Date(product.releaseDate).toLocaleDateString(undefined, { year: 'numeric', month: 'long', day: 'numeric' })
      : null,
  );

  $effect(() => {
    const o = offer;
    product = null;
    error = null;
    failed = false;
    void commands.storeProduct(o.namespace, o.id).then((res) => {
      if (store.product !== o) return;
      if (res.status === 'ok') product = res.data;
      else error = res.error.message;
    });
  });

  // A game got on Epic's checkout shows as owned once the library has it (it refreshes when
  // Epic's page closes).
  $effect(() => {
    void library.games;
    const o = offer;
    untrack(() => {
      if (!product || product.offer.owned) return;
      void commands.storeProduct(o.namespace, o.id).then((res) => {
        if (store.product === o && res.status === 'ok') product = res.data;
      });
    });
  });
</script>

<div class="product">
  <button class="back btn btn-quiet" onclick={() => (store.product = null)}>
    <Icon name="chevronLeft" size={18} /> Store
  </button>

  <div class="layout">
    <div class="main">
      <h1>{view.title}</h1>
      <p class="by">
        {[product?.developer || view.seller, product?.publisher && product.publisher !== product.developer ? product.publisher : null]
          .filter(Boolean)
          .join(' · ')}
      </p>

      <div class="media">
        {#if view.imageWide && !failed}
          <img src={view.imageWide} alt="" decoding="async" onerror={() => (failed = true)} />
        {:else}
          <span class="ph" aria-hidden="true">{view.title}</span>
        {/if}
      </div>

      {#if product}
        {#if product.genres.length || product.features.length}
          <ul class="tags" aria-label="Genres and features">
            {#each [...product.genres, ...product.features] as t (t)}<li>{t}</li>{/each}
          </ul>
        {/if}
        {#if product.description}<p class="lead">{product.description}</p>{/if}
        {#if product.longDescription}<Markdown source={product.longDescription} />{/if}
      {:else if error}
        <p class="msg err">{error}</p>
      {:else}
        <p class="msg">Loading details…</p>
      {/if}
    </div>

    <aside class="buy" aria-label="Price and purchase">
      {#if view.owned}
        <p class="owned"><Icon name="check" size={18} /> In your library</p>
        <button class="btn btn-primary wide" onclick={() => router.go('library')}>Go to Library</button>
      {:else}
        <div class="price">
          {#if discount > 0 && price?.fmtOriginal}<span class="pct">−{discount}%</span><s>{price.fmtOriginal}</s>{/if}
          <strong class:free>{free ? 'Free' : (price?.fmtCurrent ?? '')}</strong>
        </div>
        {#if giveaway}<p class="note">{giveaway}</p>{/if}
        <button
          class="btn btn-primary wide"
          onclick={() => store.openEpic({ kind: 'checkout', namespace: view.namespace, offerId: view.id }, 'Checkout')}
        >
          {free ? 'Get' : 'Buy now'}
        </button>
        <p class="note">Shows Epic's own checkout, signed in. Nothing is bought until you confirm there.</p>
      {/if}
      {#if view.slug}
        <button class="btn btn-quiet wide" onclick={() => store.openEpic({ kind: 'product', slug: view.slug ?? '' }, "Epic's store page")}>
          View on Epic's store
        </button>
      {/if}
      {#if released}<p class="note">Released {released}</p>{/if}
    </aside>
  </div>
</div>

<style>
  .back {
    margin: -6px 0 12px -10px;
  }
  .layout {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 280px;
    gap: 32px;
    align-items: start;
  }
  h1 {
    margin: 0;
    font-size: 28px;
    line-height: 1.15;
  }
  .by {
    margin: 6px 0 18px;
    color: var(--text-2);
  }
  .media {
    position: relative;
    aspect-ratio: 16 / 9;
    border-radius: var(--radius-lg);
    overflow: hidden;
    background: var(--surface);
    max-width: 760px;
  }
  .media img {
    width: 100%;
    height: 100%;
    object-fit: cover;
    display: block;
  }
  .ph {
    position: absolute;
    inset: auto 20px 20px;
    font-size: 24px;
    font-weight: 700;
  }
  .tags {
    list-style: none;
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    margin: 18px 0 6px;
    padding: 0;
  }
  .tags li {
    padding: 4px 12px;
    border-radius: 999px;
    background: var(--control);
    color: var(--text-2);
    font-size: 13px;
  }
  .lead {
    margin: 14px 0;
    max-width: 760px;
    color: var(--text);
    line-height: 1.5;
  }
  .msg {
    color: var(--text-2);
    margin: 16px 0;
  }
  .err {
    color: var(--danger);
  }
  .buy {
    position: sticky;
    top: 0;
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 18px;
    border-radius: var(--radius-lg);
    background: var(--surface);
  }
  .price {
    display: flex;
    align-items: baseline;
    gap: 10px;
    font-variant-numeric: tabular-nums;
  }
  .price strong {
    font-size: 20px;
  }
  .pct {
    padding: 2px 8px;
    border-radius: 8px;
    background: var(--accent-fill);
    color: #fff;
    font-weight: 700;
    font-size: 13px;
  }
  s {
    color: var(--text-2);
  }
  .free {
    color: var(--success);
  }
  .owned {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 0;
    font-weight: 600;
    color: var(--success);
  }
  .wide {
    width: 100%;
    justify-content: center;
  }
  .note {
    margin: 0;
    font-size: 12.5px;
    color: var(--text-2);
  }
  @media (max-width: 1100px) {
    .layout {
      grid-template-columns: minmax(0, 1fr);
    }
    .buy {
      position: static;
      max-width: 420px;
    }
  }
</style>
