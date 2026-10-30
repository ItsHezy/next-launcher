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

## Search / browse — [OBS] (Phase 6: all arguments below verified)

`POST https://store.epicgames.com/graphql` with `Content-Type: application/json` and a
non-persisted query. Since Phase 6 the app sends **GraphQL variables** (`$keywords`, `$country`,
...) so search text is never pasted into the query string. Variables work as typed below
(`count/start: Int`, `country: String!`, `onSale: Boolean`, the rest `String`). **[OBS]**

```graphql
query { Catalog { searchStore(
  count: 40, start: 0, country: "US", locale: "en-US",
  category: "games/edition/base|bundles/games|editors",   # [OBS] pipe-OR works
  sortBy: "releaseDate", sortDir: "DESC",                # [OBS]; also "currentPrice", "relevancy"
  keywords: "<text>",                                    # [OBS] all words must match
  onSale: true,                                          # [OBS]
  releaseDate: "[,2026-10-29T23:59:59.999Z]",            # [OBS] released up to then
  tag: "1216|1210",                                      # [GUESS] genre/feature tag ids (unused)
) {
  elements { title id namespace productSlug urlSlug offerType effectiveDate
             keyImages { type url } seller { name } items { id }
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
| Featured | An overview of the three rows below (6 each, "See all" per row). No documented curated feed: `egs-platform-service .../api/v2/public/discover/home` exists but rejects every `platform` value we tried (`pc`, `Windows`), and we stopped there rather than guess further. `sortBy: "relevancy"` without keywords is just catalog order. **[OBS]** |
| Free games | free-games feed |
| On sale | `searchStore(onSale: true, sortBy: "currentPrice", sortDir: "DESC")` (sorting ASC lists 100 %-off giveaways first) |
| New releases | `searchStore(sortBy: "releaseDate", sortDir: "DESC", releaseDate: "[,<now>]")` |

Ownership badges: library rows carry the *launcher's* catalog item id, which differs from the
store offer's `items[].id` for the same game **[OBS]** (ABZÛ: `1e4e…` in the library, `e877…`
in the store). The namespace matches. "In library" = every offer item is owned, or the offer is a
`BASE_GAME` and the account owns a base game in that namespace. Editions, bundles and add-ons
need the item ids.

## Product pages — [OBS] / [REF]

**As built (Phase 6):** one GraphQL call, `Catalog.catalogOffer(namespace, id, locale)` with
`description longDescription developerDisplayName publisherDisplayName releaseDate tags { name
groupName }` plus the search fields, has everything the product view shows **[OBS]**.
`longDescription` is Markdown with `<!--textBlock-->`-style comments; the UI parses it into plain
data (headings, paragraphs, lists, bold/italic, images from Epic's CDNs only) and never inserts
HTML. The platform-service calls below also work unauthenticated **[OBS]** (`mappings/<slug>` →
`productId`, `sandboxId` = offer namespace; `products/<id>`; `category-cards`) but aren't needed yet.

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

## Wishlist, cart, checkout — [OBS] → embedded web

- Wishlist and cart are store-site features tied to the web session. No usable API is documented
  in our references. The top-bar Wishlist/Cart buttons show `https://store.epicgames.com/<lang>/wishlist`
  and `/<lang>/cart` inside the Store view, signed in (exchange-code bridge, auth.md §6; ADR 0017).
- **Checkout is always Epic's own page**, shown inside the Store view, signed in. We build a product URL,
  and the user completes payment, 2FA and confirmation themselves. No purchase step is automated.
  `https://store.epicgames.com/purchase?offers=1-<namespace>-<offerId>` opens Epic's checkout
  for that offer, signed in **[OBS]** (Phase 6: a free giveaway showed "Add to library"; nothing
  was clicked). Product pages: `https://store.epicgames.com/<lang>/p/<slug>` **[OBS]**.
  Slugs and ids are checked (`[A-Za-z0-9_-]`) before they go into a URL.
- After Epic's page closes, trigger a library refresh (new purchases show up within seconds); an
  open product page re-checks ownership when the library changes.
- The checkout's payment step is a frame from Epic's payment site; the app's styling never runs
  in frames (ADR 0017).

## Caching

| Data | TTL |
|---|---|
| searchStore pages (browse and search) | 10 min |
| offer details | 1 h |
| free games | until next promo boundary (≤ 6 h) |
| category cards | 24 h |
