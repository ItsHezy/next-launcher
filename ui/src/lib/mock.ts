// UI-only placeholder for the wallet (Phase 7). Library data lives in SQLite; the store
// is live since Phase 6.

/** Real balances arrive in Phase 7; until then nothing pretends to be your wallet. */
export const wallet: { balance: string | null; rewards: string | null; coupons: number | null } = {
  balance: null,
  rewards: null,
  coupons: null,
};
