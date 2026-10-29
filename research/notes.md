# Phase 0 research ledger (2026-10-28)

How each fact was obtained. Raw downloads and probe output live in `research/refs/` and
`research/probe/` (git-ignored). Nothing here contains tokens, cookies or the user's account id.

## Observed on the dev machine (read-only)

| # | Finding | Evidence | Spec |
|---|---|---|---|
| O1 | EGL 20.3.6-59512232 installed at `C:\Program Files\Epic Games\Launcher` (64-bit path) | dir listing, `Launcher.manifest.meta` | README, egl-local-files |
| O2 | Two installs (Fortnite + LEGO Fortnite content) in `F:\Fortnite`, listed in `LauncherInstalled.dat` + 2 `.item` files | file contents | egl-local-files |
| O3 | `.item` field set (≈60 fields), including BaseURLs (3 CDNs), process-name lists, InstallComponents | file contents | egl-local-files, launch, downloads-cdn |
| O4 | `.sdmeta` = `SelDlMeta` JSON, UTF-16LE+BOM or UTF-8. `UniqueId` = `.item` InstallComponents | file contents | manifests |
| O5 | Binary manifest header: magic `0x44BEC00C`, header 41 B, sizes, SHA-1, stored_as=1, version 17, zlib body | `od` of first 64 bytes | manifests |
| O6 | `GameUserSettings.ini` lives in `Config\WindowsEditor\` (not `Windows\`). Sections keyed by account id. RememberMe present (not read) | key names only, values masked | egl-local-files, ADR 0003 |
| O7 | EGL log hosts: account-public-service-prod03 (`/oauth/token`), eulatracking (`epicgames_tos`), datarouter + api.epicgames.dev telemetry. Log leaks an `eg1~` token fragment in a sessions/kill URL | grep of hostnames with ids masked | README, auth |

## Live probes (unauthenticated, public data or deliberately invalid codes)

| # | Request | Result | Spec |
|---|---|---|---|
| P1 | `GET account .../epicdomains/ssodomains` | 4 SSO domains | auth §6 |
| P2 | `GET www.epicgames.com/id/api/csrf` | 204 + `XSRF-TOKEN` cookie | auth §6 |
| P3 | `POST www.epicgames.com/id/api/exchange` with dummy code | 400 `single_use_code_not_found` (18001), so the route validates exchange codes | auth §6 |
| P4 | `GET www.epicgames.com/id/exchange?exchangeCode=..` | 200 identity SPA | auth §6 |
| P5 | `GET www.epicgames.com/account/*` | 302 → `accounts.epicgames.com/account/*` → `/auth/login?clientId=007c0bfe…` (all paths, including made-up ones). `/transactions` → `/transactions/purchases` | account, wallet |
| P6 | free-games feed | 200, 13 elements, keyImages types listed | store |
| P7 | platform-service product `prod-fn` | 200 | store |
| P8 | `store.epicgames.com/graphql` raw searchStore | 200 data | store |
| P9 | `launcher.store.epicgames.com/graphql` | 302 → hCaptcha page | store, ADR 0005 |
| P10 | `graphql.epicgames.com/graphql` | 404 Gone | store |
| P11 | image CDN resize params on cdn1/cdn2 | resized 15–45 KB, format chosen by CDN (Akamai Image Server), no `Vary: Accept` | catalog-and-images, ADR 0004 |
| P12 | `store.epicgames.com/redeem`, `/wallet` | 403 to curl (bot protection) / 301 → `/wallet` | wallet |

## From references

- Legendary master + Heroic fork: auth flows, all launcher/library/catalog/entitlement/ownership/
  datastorage endpoints, binary/JSON manifest + chunk formats (incl. feature level 22/24
  encryption), rolling hash, download planning, launch args, cloud-save packaging, EGL import.
  Heroic-only: download ticket + artifact-delivery signed URLs (batches of 50, ticket refresh < 5 min), EULA endpoints.
- Community endpoint docs: token types, exchange-code PKCE params, sessions/kill types,
  library state token/collections/playtime, wallet, fulfillment redeem, platform-service
  product/mapping/category-cards, presence, lightswitch, error-code table.

## Phase 2 live verification (2026-10-28, owner's own account)

| # | Finding | Evidence |
|---|---|---|
| L1 | Login page → `/id/api/redirect` → authorization code works in an embedded WebView2 window | owner signed in; app log `signed in` |
| L2 | Epic accepts PKCE (`codeChallenge` S256 on redirect + `code_verifier` on the token grant) for `launcherAppClient2` | same sign-in used PKCE (default on) |
| L3 | Refresh token fits one Credential Manager entry (< 2,000 bytes JSON); chunking stays as a safeguard | `cmdkey /list` shows one `<id>.NextLauncher` entry |
| L4 | Refresh-token restore after restart works; session usable ~1.2 s after launch | app log after relaunch |
| L5 | Launcher token has 150 permissions incl. resources for account update, token kill, fulfillment, reward, library | `verify?includePerms=true`, resource names only |
| L6 | No pending `epicgames_tos` agreement for this account | EULA tracking returned 204 |
| L7 | **Exchange-code bridge works**: with the web profile deleted, `/id/exchange?exchangeCode=…&redirectUrl=accounts.epicgames.com/account/password` opened signed in | owner confirmed; app log `opening signed-in Epic page page=Password` |

## Phase 3 live verification (2026-10-29, owner's account)

| # | Finding | Evidence |
|---|---|---|
| L8 | Library items: 62 records, 38 `APPLICATION` records with `appName`; all 38 have a Windows asset | sync log (counts only) |
| L9 | Catalog bulk items return `keyImages`, `releaseInfo[].appId`, `customAttributes`; batching per namespace worked (max batch size still untested, kept at 25) | sync log |
| L10 | Real key image types (see catalog-and-images.md); tall/wide preference lists cover all games | sync log |
| L11 | Playtime `all` works (27 artifacts with time); `releaseInfo.appId` matches playtime `artifactId` | DB counts |
| L12 | Library state token stays valid across syncs when nothing changed (second sync 742 ms) | sync log |
| L13 | 1 owned game is third-party managed (`ThirdPartyManagedApp`/`Provider`) | DB count |
| L14 | EGL `.item` booleans are `bIsIncompleteInstall` etc. (lowercase `b`); PascalCase deserialisation misses them | unit test caught it |

## Phase 4 live verification (2026-10-29, owner's account, test game only)

| # | Finding | Evidence |
|---|---|---|
| L15 | Build info (`assets/v2/.../label/Live`) works with the launcher token for an owned free game, and the manifest's `sha1(body)` matches `hash` | install proceeded (a mismatch fails the job) |
| L16 | Owned installable games: feature levels 17 (16), 21 (13), 13 (5); 0 signed-URL builds, 0 encrypted builds; 1 with selective-download components (Fortnite, 2) | size probe, manifests only |
| L17 | Feature level 13 chunk path `ChunksV3/{group:02}/{hash:016X}_{GUID}.chunk` downloads and passes chunk + file SHA-1 | 153-file install, 816 chunks |
| L18 | Verify catches a one-byte change in place (size unchanged) via file SHA-1; repair fetched only its 5 chunks | verify / repair / verify log |
| L19 | Resume after a forced kill skips files already renamed into place (141/153) | restart log `skipped=141` |

## Phase 5 live verification (2026-10-29, owner's account, test game only)

| # | Finding | Evidence |
|---|---|---|
| L20 | `GET /account/api/oauth/exchange` right before start + EGL argument order starts *while True: learn()* (Unity); it ignores the auth args | `game started ... offline=false`, game window up |
| L21 | Playtime `PUT .../playtime/account/<id>` with `startSegment`/`endSegment` true → 204; the next `playtime/all` total grew by the session (14,110 → 14,129 s after an 18 s session) | `playtime reported`, DB before/after sync |
| L22 | Owned installable games: 5 of 38 need an ownership token, 1 can't run offline, 2 have `AdditionalCommandLine` | catalog `customAttributes`, counts only |
| L23 | The one third-party title uses `ThirdPartyManagedProvider: UbisoftConnect` and `GameID` | catalog `customAttributes` |

## Phase 6 live verification (2026-10-29, public store data; signed-in pages on the owner's account)

| # | Finding | Evidence |
|---|---|---|
| L24 | `store.epicgames.com/graphql` `searchStore` accepts GraphQL variables; `keywords`, `onSale`, `releaseDate` range, pipe-OR `category`, `sortBy` `releaseDate`/`currentPrice`/`relevancy` all work; no bot check in ~15 requests with the launcher UA | probes `research/probe/p6/`, app log |
| L25 | `catalogOffer(namespace, id, locale)` returns `longDescription` (Markdown with `<!--…-->` block comments), developer/publisher, tags with `groupName` | probe, product view |
| L26 | Platform service `mappings/<slug>`, `products/<id>`, `category-cards` work unauthenticated; `sandboxId` = offer namespace; `api/v2/public/discover/home` exists but its `platform` value is unknown (400 for `pc`, `Windows`) | probes |
| L27 | Library catalog item ids differ from the store offer's `items[].id` for the same game; namespaces match | ABZÛ comparison |
| L28 | `store.epicgames.com/<lang>/p/<slug>` and `/purchase?offers=1-<ns>-<offer>` open signed in through the exchange-code bridge; the checkout shows Epic's own confirm button | screenshots in `.dev-data/shots/` (not committed) |

## Phase 7 live verification (2026-10-29, owner's account; nothing on the account was changed)

| # | Finding | Evidence |
|---|---|---|
| L29 | Launcher token: `account:public:account` 6, `externalAuths` 7, `fulfillment:public:code` 1, wallet + reward-accounts read 2, `otherSessionsWithSameSourceForAccount` 8 (not `otherSessionsForAccount`), `presence:_:<id>` 4 | verify perms, resource names with the account id masked |
| L30 | Wallet summary works with the launcher token: `EpicWalletUnsupported` in the owner's region, `EpicRewardsSupported`, coupon count present | app log (type names only) |
| L31 | Account info + externalAuths: 2FA on, 4 linked accounts (`lego`, `nintendo`, `steam`, `ubisoft`) | app log (kinds only) |
| L32 | Redesigned `accounts.epicgames.com`: `/personal`, `/connections`, `/messaging`, `/password`, `/payments`, `/rewards`, `/code-redemption`, `/eula-history`, `/transactions/{purchases,gifts}` exist; `/privacy`, `/payment-management`, `/email-preferences`, `/coupons` are 404 | page titles via the web window |
| L33 | No active-sessions or sign-in-history page; Password and security has password, 2FA and sign-out | page keywords |
| L34 | `store.epicgames.com/achievements` is 404; `publisher-index`, `gifts` exist; legal pages live on `legal.epicgames.com` | screenshots, titles |
| L35 | With a DevTools port open, the store answers navigations with "Just a moment…"; the accounts site doesn't | CDP test runs (not used in normal operation) |

## Open questions (carried into phases)

| Question | Phase |
|---|---|
| exact action bits for account update, kill OTHERS, wallet, fulfillment | done (L29) |
| catalog bulk max ids per request (25 works; larger untested) | 6 |
| a small free test game the owner owns: *while True: learn()* chosen by the owner (Phase 4) | done |
| store "Featured" source (discover v2 `platform` value) | open; Featured = overview rows |
| achievements GraphQL on store.epicgames.com with bearer | 9 |
| presence connection cost vs idle budget | open (not built, ADR 0011) |
