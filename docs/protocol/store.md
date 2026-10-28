# Store: free games, browse/search, product pages, achievements, checkout

Crate: `egs-api::store`. All store reads are unauthenticated unless stated, so the store works
even when token refresh is pending. Responses go through `store_cache` (query hash → JSON, TTL).

## Free games — [OBS] probe 2026-10-28

`GET https://store-site-backend-static-ipv4.ak.epicgames.com/freeGamesPromotions?locale=en-US&country=<CC>&allowCountries=<CC>`

`data.Catalog.searchStore.elements[]` (13 on the probe day). Each element has: `title, id, namespace,
description, effectiveDate, offerType, expiryDate, viewableDate, status, isCodeRedemptionOnly,
keyImages[], seller{id,name}, productSlug, urlSlug, url, items[], customAttributes[], categories[],
tags[], catalogNs{mappings[{pageSlug,pageType}]}, offerMappings[], price{totalPrice{discountPrice,
originalPrice, currencyCode, fmtPrice{originalPrice, discountPrice, intermediatePrice}}}, promotions`.

- **Free now**: `promotions.promotionalOffers[].promotionalOffers[]` with
  `discountSetting.discountPercentage == 0`, now between `startDate` and `endDate`.
- **Coming next**: same shape under `promotions.upcomingPromotionalOffers`.
- Elements with no promotions are other giveaway-adjacent items. Ignore them.
- Cache until the earliest `endDate`/`startDate` boundary, max 6 h.
- "Claim" opens the product page in the embedded signed-in store (a $0 checkout is still a purchase flow).

## Search / browse — [OBS] basic query works; arguments [REF]/[GUESS]

`POST https://store.epicgames.com/graphql` with `Content-Type: application/json` and a raw
(non-persisted) query:

```graphql
query { Catalog { searchStore(
  count: 40, start: 0, country: "US", locale: "en-US",
  category: "games/edition/base|bundles/games|editors",   # [GUESS] pipe-OR syntax
  sortBy: "releaseDate", sortDir: "DESC",                # [OBS] accepted
  keywords: "<text>",                                    # [GUESS]
  onSale: true,                                          # [GUESS]
  tag: "1216|1210",                                      # [GUESS] genre/feature tag ids
  withPrice: true                                        # [GUESS]
) {
  elements { title id namespace productSlug urlSlug offerType effectiveDate
             keyImages { type url } seller { name }
             price(country: "US") { totalPrice { discountPrice originalPrice currencyCode fmtPrice(locale: "en-US") { originalPrice discountPrice } } }
             catalogNs { mappings(pageType: "productHome") { pageSlug } } }
  paging { count total } } } }
```

Probe result: HTTP 200 with `{"data":{"Catalog":{"searchStore":{"elements":[...]}}}}`.

- `launcher.store.epicgames.com/graphql` (the host Legendary uses) answered our probe with
  **302 to an hCaptcha page**. We don't use it, and we never try to solve or avoid a
  challenge. If `store.epicgames.com/graphql` starts returning a challenge (non-JSON, 403, or
  a redirect to a captcha), that view **falls back to the embedded web store**.
- `graphql.epicgames.com` is gone (404). [OBS]

Chip mapping (Phase 6):

| Chip | Source |
|---|---|
| Featured | **[GUESS]**: not found yet. Candidates to probe in Phase 6: platform-service discover modules, store CMS collections (`top-sellers`, `most-played`). Fallback: `searchStore` sorted by `effectiveDate` DESC. |
| Free games | free-games feed |
| On sale | `searchStore(onSale: true, sortBy: "currentPrice")` |
| New releases | `searchStore(sortBy: "releaseDate", sortDir: "DESC")`, releaseDate ≤ now |

## Product pages — [OBS] / [REF]

Epic's newer REST "EGS Platform Service" (no auth):

| Need | Request | Conf. |
|---|---|---|
| slug → product/offer ids | `GET https://egs-platform-service.store.epicgames.com/api/v1/egs/mappings/<slug>?country=<CC>&locale=<lc>` → `{productId, sandboxId, modules.pageOffers[{offerId, categories, effectiveDate}]}` | [REF] |
| product summary | `GET .../api/v1/egs/products/<productId>?country=<CC>&locale=<lc>&store=EGS` → `{title, developers, publishers, shortDescription, media{card16x9, card3x4, logo}, tags{genres, features, platforms...}, supportedModules}` | [OBS] |
| editions / add-ons | `.../products/<productId>/offers/featured-editions`, `.../featured-add-ons` (exact paths [REF], verify) | [REF] |
| critic reviews | `.../products/<productId>/critic` | [REF] |
| category tiles | `GET .../api/v1/egs/category-cards?count=..&start=0&country=..&locale=..&store=EGS` | [REF] |
| publisher index | `.../publisher-index` (Legal section link) | [REF] |

Long description, screenshots, system requirements: **[GUESS]**. Not in the platform-service
product summary. Phase 6 either finds the module or opens the full product page in the embedded store.

## Achievements — [REF]

GraphQL queries `Achievement.productAchievementsRecordBySandbox(sandboxId, locale)` and
`PlayerAchievement.playerAchievementGameRecordsBySandbox(epicAccountId, sandboxId)` (bearer).
Legendary sends them to `launcher.store.epicgames.com`, which challenged our probe. Phase 7 checks
whether `store.epicgames.com/graphql` accepts them with our bearer token. If not, Achievements
stays a web page.

## Wishlist, cart, checkout — [GUESS] → embedded web

- Wishlist and cart are store-site features tied to the web session. No usable API is documented
  in our references. v1: the top-bar Wishlist/Cart buttons open `https://store.epicgames.com/wishlist`
  and `https://store.epicgames.com/cart` in the signed-in web window. Paths to verify in Phase 6.
- **Checkout is always Epic's own page** in the signed-in web window. We build a product or cart URL,
  and the user completes payment, 2FA and confirmation themselves. No purchase step is automated.
  A direct offer checkout URL of the form `https://store.epicgames.com/purchase?offers=1-<namespace>-<offerId>` is **[GUESS]**, to verify in Phase 6.
- After the web window closes, trigger a library refresh (new purchases show up within seconds).

## Caching

| Data | TTL |
|---|---|
| searchStore pages | 10 min |
| product summary | 1 h |
| free games | until next promo boundary (≤ 6 h) |
| category cards | 24 h |
