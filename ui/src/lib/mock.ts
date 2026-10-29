// UI-only sample data for Phase 1 (wallet, store). Library mock data lives in SQLite.
// All names are invented; no store data or artwork is used.

/** Real balances arrive in Phase 7; until then nothing pretends to be your wallet. */
export const wallet: { balance: string | null; rewards: string | null; coupons: number | null } = {
  balance: null,
  rewards: null,
  coupons: null,
};

export interface StoreItem {
  id: string;
  title: string;
  seller: string;
  price: string;
  original?: string;
  discount?: number;
  free?: boolean;
  hue: number;
  tags: Array<'featured' | 'free' | 'sale' | 'new'>;
}

const NAMES = [
  'Ember Protocol', 'Tidebreaker', 'Glasshouse', 'Paper Moons', 'Rook & Raven', 'Static Bloom',
  'Saltmarsh', 'Kingdom of Ash', 'Low Orbit', 'Hearthbound', 'Night Shift Racer', 'Woven Worlds',
  'Signal Lost', 'Granite Peak', 'Copperline', 'The Long Quiet', 'Velvet Circuit', 'Moonlit Ferry',
  'Iron Lantern', 'Outpost Seven', 'Fable Engine', 'Driftwood', 'Cinder Court', 'Polar Relay',
];
const SELLERS = ['Northwind Interactive', 'Paper Lantern Games', 'Ninefold', 'Kestrel Works', 'Overcast'];

export const storeItems: StoreItem[] = NAMES.map((title, i) => {
  const tags: StoreItem['tags'] = [];
  if (i % 3 === 0) tags.push('featured');
  if (i % 8 === 2) tags.push('free');
  if (i % 4 === 1) tags.push('sale');
  if (i % 5 === 3) tags.push('new');
  const base = [9.99, 14.99, 19.99, 24.99, 29.99, 39.99, 59.99][i % 7] ?? 19.99;
  const free = tags.includes('free');
  const discount = tags.includes('sale') ? [20, 35, 50, 75][i % 4] : undefined;
  const price = free ? 'Free' : discount ? `$${(base * (1 - discount / 100)).toFixed(2)}` : `$${base}`;
  return {
    id: `store-${i}`,
    title,
    seller: SELLERS[i % SELLERS.length] ?? 'Indie',
    price,
    original: free || discount ? `$${base}` : undefined,
    discount,
    free,
    hue: (i * 47) % 360,
    tags,
  };
});
