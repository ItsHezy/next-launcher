// Store view state (Phase 6). Data comes from Rust, which caches it (store.md "Caching").

import {
  commands,
  type StoreFeatured_Serialize as Featured,
  type StoreOffer_Serialize as Offer,
  type UiError,
} from '../bindings';
import { num } from '../boot';

export type Chip = 'featured' | 'free' | 'onSale' | 'newReleases';
export type { Offer };

class StoreState {
  chip = $state<Chip>('featured');
  /** Search text from the top bar while the Store is open. */
  query = $state('');
  featured = $state.raw<Featured | null>(null);
  offers = $state.raw<Offer[]>([]);
  total = $state(0);
  loading = $state(false);
  error = $state.raw<{ message: string; challenge: boolean } | null>(null);
  /** The offer whose product page is open. */
  product = $state.raw<Offer | null>(null);
  /** Set by the first load (the Store tab's first visit). */
  started = false;

  #seq = 0;
  #timer: ReturnType<typeof setTimeout> | undefined;

  get searching() {
    return this.query.trim().length > 0;
  }

  setChip(chip: Chip) {
    this.chip = chip;
    this.query = '';
    this.product = null;
    void this.load();
  }

  /** Debounced: typing doesn't send a request per key. */
  setQuery(q: string, now = false) {
    this.query = q;
    this.product = null;
    clearTimeout(this.#timer);
    this.#timer = setTimeout(() => void this.load(), now ? 0 : 350);
  }

  open(offer: Offer) {
    this.product = offer;
  }

  async load(more = false) {
    this.started = true;
    const seq = ++this.#seq;
    this.loading = true;
    if (!more) this.error = null;
    const q = this.query.trim();
    if (!q && this.chip === 'featured') {
      const res = await commands.storeFeatured();
      if (seq !== this.#seq) return;
      if (res.status === 'ok') this.featured = res.data;
      else this.fail(res.error);
    } else {
      const start = more ? this.offers.length : 0;
      const res = q
        ? await commands.storeSearch(q, start)
        : await commands.storeBrowse(this.chip === 'featured' ? 'newReleases' : this.chip, start);
      if (seq !== this.#seq) return;
      if (res.status === 'ok') {
        this.offers = more ? [...this.offers, ...res.data.offers] : res.data.offers;
        this.total = num(res.data.total);
      } else this.fail(res.error);
    }
    this.loading = false;
  }

  fail(e: UiError) {
    this.error = { message: e.message, challenge: e.code === 'challenge' };
    if (!this.offers.length) this.total = 0;
  }
}

export const store = new StoreState();

/** "Free until Oct 30" / "Free from Oct 30" for giveaways. */
export function freeLabel(o: Offer): string | null {
  if (!o.freeUntil || !o.freeFrom) return null;
  const now = Date.now();
  const from = Date.parse(o.freeFrom);
  const until = Date.parse(o.freeUntil);
  return from <= now ? `Free until ${dayFmt.format(until)}` : `Free from ${dayFmt.format(from)}`;
}

const dayFmt = new Intl.DateTimeFormat(undefined, { month: 'short', day: 'numeric' });
