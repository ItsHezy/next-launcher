# Wallet, rewards, coupons, code redemption

Crate: `egs-api::wallet`.

## Wallet summary (balance, Epic Rewards, coupon count) — [REF], permission [GUESS]

`GET https://egs-platform-service.store.epicgames.com/api/v1/private/egs/account/wallet?locale=<lc>&store=EGS` (bearer)

```json
{
  "__typename": "Wallet",
  "coupons": { "count": 0 },
  "epicRewards": { "__typename": "EpicRewardsSupported", "balance": "€9.63", "rawBalance": 963 },
  "wallet": { "__typename": "EpicWalletSupported", "balance": "€0.00" }
}
```

- Needs a reward-accounts read permission (community docs suggest `reward:public:{accountId}:reward-accounts READ`).
  Phase 7 checks whether the launcher token gets 200. `{"errorMessage":"Unauthorized"}` means it doesn't.
- `__typename` variants other than `...Supported` (e.g. Wallet unavailable in the user's country)
  ⇒ hide that row and show "Not available in your region".
- Balances are display strings already formatted by Epic in the account currency. We show them
  verbatim and never do currency math. `rawBalance` is minor units.
- Refreshed on app start (after first paint), when the wallet popover opens (if older than 5 min),
  and after a web checkout window closes.
- If the endpoint is not permitted: the top-bar button shows a wallet icon without a balance, and
  the popover offers the web links below.

## Redeem a code — [REF], permission [GUESS]

`POST https://fulfillment-public-service-prod.ol.epicgames.com/fulfillment/api/public/accounts/<accountId>/codes/<CODE_WITHOUT_DASHES>` (bearer)

→ 200 `{offerId, accountId, identityId, details:[{entitlementId, entitlementName, itemId, namespace, country}]}`.

Errors to map:
- `errors.com.epicgames.coderedemption.multiple_redemptions_not_allowed` (19025) → "You already redeemed this code."
- `errors.com.epicgames.ecommerce.fulfillment.code_not_redeemable.coupon_code` (1002) → "This is a coupon. Use it at checkout."
- not found / invalid → "That code isn't valid."

Requires `fulfillment:public:code CREATE`. If the launcher token lacks it, the "Redeem code" row
opens `https://store.epicgames.com/redeem` in the signed-in web window (route exists, 403 to curl
because the site blocks scripted requests [OBS]).

The code is typed by the user in our dialog and sent once on their click. Never logged.

## Web-only wallet features (signed-in web window)

| Feature | URL | Conf. |
|---|---|---|
| Add funds | `https://store.epicgames.com/wallet` | [OBS] route (301 from `/en-US/wallet`) |
| Redeem Epic gift card | `https://store.epicgames.com/redeem` (same page as codes) | [OBS] route |
| Redeem Fortnite V-Bucks card | `https://www.fortnite.com/vbuckscard` | [GUESS] |
| Transaction history | `https://accounts.epicgames.com/account/transactions` | [OBS] route (→ `/transactions/purchases`) |
| Payment methods | `https://accounts.epicgames.com/account/payment-management` | [GUESS] |
| Refund requests | from a transaction row on the transactions page (Epic's refund form) | [GUESS] |
| Gifts | `https://accounts.epicgames.com/account/gifts` (or a store gifts page) | [GUESS] |
| Coupons | `https://accounts.epicgames.com/account/coupons` | [GUESS] |
| Epic Rewards details | `https://accounts.epicgames.com/account/rewards` | [GUESS] |

Every payment-related action happens on Epic's page and is completed by the user.
