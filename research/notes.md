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

## Open questions (carried into phases)

| Question | Phase |
|---|---|
| `/id/exchange` or `POST /id/api/exchange` actually establishes a session, and accounts.epicgames.com SSO completes silently | 2 |
| PKCE accepted for `launcherAppClient2` | 2 |
| launcher token permissions: account update, kill OTHERS, wallet, fulfillment | 2 / 7 |
| real keyImage type set in the library, catalog bulk max ids | 3 |
| a small free test game the owner owns (Fortnite is unsuitable: huge + anti-cheat) | 4 |
| store "Featured" source, searchStore filter args, checkout URL format | 6 |
| achievements GraphQL on store.epicgames.com with bearer | 7 |
| presence connection cost vs idle budget | 7 |
