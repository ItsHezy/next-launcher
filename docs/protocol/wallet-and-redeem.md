# Wallet, rewards, coupons, code redemption

Crate: `egs-api::wallet`.

## Wallet summary (balance, Epic Rewards, coupon count) — [OBS] Phase 7

`GET https://egs-platform-service.store.epicgames.com/api/v1/private/egs/account/wallet?locale=<lc>&store=EGS` (bearer)

```json
{
  "__typename": "Wallet",
  "coupons": { "count": 0 },
  "epicRewards": { "__typename": "EpicRewardsSupported", "balance": "€9.63", "rawBalance": 963 },
  "wallet": { "__typename": "EpicWalletSupported", "balance": "€0.00" }
}
```

- The launcher token has `payment:public:<id>:wallet` and `reward:public:<id>:reward-accounts` READ
  and gets 200 **[OBS]**. On the owner's account: `wallet.__typename = "EpicWalletUnsupported"`
  (not offered in the region), `epicRewards.__typename = "EpicRewardsSupported"`, coupons count present.
- `__typename` variants other than `...Supported` (e.g. Wallet unavailable in the user's country)
  ⇒ hide that row and show "Not available in your region".
- Balances are display strings already formatted by Epic in the account currency. We show them
  verbatim and never do currency math. `rawBalance` is minor units.
- Refreshed on app start (after first paint), when the wallet popover opens (if older than 5 min),
  and after a web checkout window closes.
- If the endpoint is not permitted: the top-bar button shows a wallet icon without a balance, and
  the popover offers the web links below.

## Redeem a code — [REF], permission [OBS]

`POST https://fulfillment-public-service-prod.ol.epicgames.com/fulfillment/api/public/accounts/<accountId>/codes/<CODE_WITHOUT_DASHES>` (bearer)

→ 200 `{offerId, accountId, identityId, details:[{entitlementId, entitlementName, itemId, namespace, country}]}`.

Errors to map:
- `errors.com.epicgames.coderedemption.multiple_redemptions_not_allowed` (19025) → "You already redeemed this code."
- `errors.com.epicgames.ecommerce.fulfillment.code_not_redeemable.coupon_code` (1002) → "This is a coupon. Use it at checkout."
- not found / invalid → "That code isn't valid."

Requires `fulfillment:public:code CREATE`, which the launcher token has **[OBS]**, so codes are
redeemed in-app (dialog; code normalised to letters/digits, sent once, never retried or logged).
Not exercised live (no spare code); error mapping is unit-tested. Without the permission the row
opens `accounts.epicgames.com/account/code-redemption`.

The code is typed by the user in our dialog and sent once on their click. Never logged.

## Web-only wallet features (Epic's page inside Settings, ADR 0016)

| Feature | URL | Conf. |
|---|---|---|
| Add funds | `https://store.epicgames.com/wallet` | [OBS] route (301 from `/en-US/wallet`); wallet unsupported in the owner's region |
| Redeem Epic gift card / code | `https://accounts.epicgames.com/account/code-redemption` | [OBS] Phase 7 |
| Redeem Fortnite V-Bucks card | `https://www.fortnite.com/vbuckscard` | [OBS] Phase 7 |
| Transaction history | `https://accounts.epicgames.com/account/transactions` | [OBS] (→ `/transactions/purchases`) |
| Payment methods | `https://accounts.epicgames.com/account/payments` | [OBS] Phase 7 ("Payment settings") |
| Refund requests | from a purchase on the transactions page (Epic's refund form) | [REF] |
| Gifts | `https://accounts.epicgames.com/account/transactions/gifts` | [OBS] Phase 7 |
| Coupons | no page (404); the count comes from the wallet API and coupons apply at checkout | [OBS] |
| Epic Rewards details | `https://accounts.epicgames.com/account/rewards` | [OBS] Phase 7 |

Every payment-related action happens on Epic's page and is completed by the user.
