<script lang="ts">
  import { num } from '../../lib/boot';
  import { freeLabel, store, type Offer } from '../../lib/state/store.svelte';

  let { offer }: { offer: Offer } = $props();

  const price = $derived(offer.price);
  const giveaway = $derived(freeLabel(offer));
  const discount = $derived(num(price?.discount));
  const freeNow = $derived(!!offer.freeFrom && Date.parse(offer.freeFrom) <= Date.now());
  const priceText = $derived(freeNow || price?.free ? 'Free' : (price?.fmtCurrent ?? ''));
  let failed = $state(false);
</script>

<!-- The visible text (title, seller, badge, price) is the button's name: no aria-label. -->
<button class="card" onclick={() => store.open(offer)}>
  <span class="art">
    {#if offer.imageTall && !failed}
      <img src={offer.imageTall} alt="" loading="lazy" decoding="async" onerror={() => (failed = true)} />
    {:else}
      <span class="ph" aria-hidden="true">{offer.title}</span>
    {/if}
    {#if offer.owned}
      <span class="badge owned">In library</span>
    {:else if giveaway}
      <span class="badge">{giveaway}</span>
    {:else if discount > 0}
      <span class="badge">−{discount}%</span>
    {/if}
  </span>
  <span class="name" title={offer.title}>{offer.title}</span>
  {#if offer.seller}<span class="seller">{offer.seller}</span>{/if}
  <span class="price">
    {#if discount > 0 && price?.fmtOriginal}<s>{price.fmtOriginal}</s>{/if}
    <strong class:free={priceText === 'Free'}>{priceText}</strong>
  </span>
</button>

<style>
  .card {
    display: flex;
    flex-direction: column;
    align-items: stretch;
    gap: 4px;
    width: 100%;
    min-width: 0;
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
    background: var(--surface);
    transition: transform var(--dur) var(--ease-spring);
  }
  .card:hover .art {
    transform: translateY(-3px);
  }
  img {
    width: 100%;
    height: 100%;
    object-fit: cover;
    display: block;
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
  .badge.owned {
    background: rgba(0, 0, 0, 0.72);
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
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .price {
    display: flex;
    gap: 8px;
    align-items: baseline;
    font-variant-numeric: tabular-nums;
    min-height: 1.4em;
  }
  s {
    color: var(--text-3);
    font-size: 12.5px;
  }
  .free {
    color: var(--success);
  }
</style>
