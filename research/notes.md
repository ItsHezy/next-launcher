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

## Open questions (carried into phases)

| Question | Phase |
|---|---|
| exact action bits for account update, kill OTHERS, wallet, fulfillment (resources present, L5) | 7 |
| catalog bulk max ids per request (25 works; larger untested) | 6 |
| a small free test game the owner owns (Fortnite is unsuitable: huge + anti-cheat) | 4 |
| store "Featured" source, searchStore filter args, checkout URL format | 6 |
| achievements GraphQL on store.epicgames.com with bearer | 7 |
| presence connection cost vs idle budget | 7 |
