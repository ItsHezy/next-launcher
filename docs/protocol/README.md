# Epic Games Store protocol notes

These specs describe how Next Launcher talks to Epic's services **with the signed-in
user's own account**, the same way the official Epic Games Launcher (EGL) does.
Every spec is written before the code that depends on it and kept current.

All examples are redacted. Placeholders:

| Placeholder | Meaning |
|---|---|
| `<accountId>` | 32-hex Epic account id of the signed-in user |
| `<access>` / `<refresh>` | OAuth tokens (never logged, never committed) |
| `<code>` | single-use authorization / exchange code |
| `<ns>` / `<itemId>` / `<appName>` | catalog namespace, catalog item id, app name |
| `<ticket>` | signed download ticket |

## Confidence legend

Every endpoint carries one of these tags.

- **[OBS]** observed on the developer machine: a local launcher file, a log line, or a
  live probe made during Phase 0 (probes used invalid codes or public data only).
- **[REF]** taken from a reference implementation or documentation (see *Sources*).
  Behaviour is documented and used by working clients, but not yet seen by us.
- **[GUESS]** inferred. Must be verified in the phase that first needs it before the
  code relies on it.

## Files

| File | Covers |
|---|---|
| [auth.md](auth.md) | login, token exchange/refresh, exchange codes, sign-out, web session bridging |
| [account.md](account.md) | account info, connected accounts, account settings: API vs embedded web |
| [library.md](library.md) | owned items, assets/builds, entitlements, playtime, collections |
| [catalog-and-images.md](catalog-and-images.md) | catalog metadata, key images, CDN resizing |
| [manifests.md](manifests.md) | binary + JSON manifests, chunk container, install tags |
| [downloads-cdn.md](downloads-cdn.md) | manifest lookup, CDN layout, signed URLs, verify |
| [launch.md](launch.md) | launch arguments, exchange code for games, ownership tokens, EULAs |
| [store.md](store.md) | free games, search/browse, product pages, achievements, checkout |
| [wallet-and-redeem.md](wallet-and-redeem.md) | wallet, rewards, coupons, code redemption |
| [cloud-saves.md](cloud-saves.md) | save sync service and save manifest format |
| [egl-local-files.md](egl-local-files.md) | the official launcher's local files (import, read-only) |
| [presence-social.md](presence-social.md) | online status, friends (later phases) |
| [web-only.md](web-only.md) | **features with no usable API → embedded signed-in web view** |

## Client identity

Next Launcher identifies itself as the launcher client because Epic's services only
grant launcher permissions (library, downloads, ownership tokens) to that client. This
is what Legendary and Heroic do. See [ADR 0002](../decisions/0002-client-identity.md).

- OAuth client: `launcherAppClient2`, id `34a02cf8f4414e29b15921876da36f9a`. The client
  secret is public (it ships inside EGL and is published by Legendary and community docs).
  It lives in `crates/egs-auth` as a constant, not in config. **[REF]**
- `User-Agent` for API hosts: `UELauncher/<egl-version> Windows/<os-build>.1.256.64bit`. **[REF]**
  - EGL version on the dev machine: `20.3.6-59512232+++UE5+Release-Distro-5.5-Windows`
    (from `C:\ProgramData\Epic\EpicGamesLauncher\Data\Launcher.manifest.meta`). **[OBS]**
  - Legendary falls back to an older version string; we read the version from the local EGL
    install when present and otherwise use a bundled default that we update with releases.
  - The version can also be queried: `GET https://launcher-public-service-prod06.ol.epicgames.com/launcher/api/public/assets/v2/platform/Windows/launcher?label=Live-EternalKnight` (auth required). **[REF]**
- Store web hosts get a browser-style UA from WebView2 itself; we do not spoof one there.
- No other client ids are ever used (no game clients, no web clients).

## Hosts

| Purpose | Host | Source |
|---|---|---|
| OAuth / account | `account-public-service-prod03.ol.epicgames.com` (also `-prod.ol.`, `-prod.ak.`) | [OBS] logs, [REF] |
| Launcher assets / builds | `launcher-public-service-prod06.ol.epicgames.com`, `launcher-public-service-prod.ak.epicgames.com` | [REF] |
| Library | `library-service.live.use1a.on.epicgames.com` | [REF] |
| Catalog | `catalog-public-service-prod06.ol.epicgames.com` | [REF] |
| Entitlements | `entitlement-public-service-prod08.ol.epicgames.com` | [REF] |
| Ownership tokens | `ecommerceintegration-public-service-ecomprod02.ol.epicgames.com` | [REF] |
| Cloud saves | `datastorage-public-service-liveegs.live.use1a.on.epicgames.com` | [REF] |
| Artifact delivery (signed chunk URLs) | `artifact-delivery-service-public-prod.ol.epicgames.com` | [REF] Heroic fork |
| EULA tracking | `eulatracking-public-service-prod06.ol.epicgames.com` | [OBS] logs, [REF] |
| Code redemption | `fulfillment-public-service-prod.ol.epicgames.com` | [REF] |
| Store GraphQL | `store.epicgames.com/graphql` | [OBS] probe |
| Store REST (product, mapping, wallet) | `egs-platform-service.store.epicgames.com` | [OBS] probe, [REF] |
| Free games feed | `store-site-backend-static-ipv4.ak.epicgames.com` | [OBS] probe |
| Web identity | `www.epicgames.com/id/...` | [OBS] probe |
| Images | `cdn1.epicgames.com`, `cdn2.unrealengine.com` | [OBS] probe |
| Chunk CDNs | `egs-cloudfront-chunks.epicgamescdn.com`, `epicgames-download1.akamaized.net`, `egdownload.fastly-edge.com`, `download.epicgames.com` | [OBS] `.item` files |
| Telemetry (EGL) | `datarouter.ol.epicgames.com`, `api.epicgames.dev/telemetry` | [OBS] logs. **We send nothing.** |

## Being a good API citizen (applies to every crate)

- One shared `reqwest::Client` (HTTP/2, gzip/brotli, rustls). Pool idle timeout 90 s.
- Concurrency caps: API hosts ≤ 6 in flight, image CDN ≤ 8, chunk CDN = the download
  engine's worker count (default 16, user-configurable).
- Timeouts: connect 5 s, request 20 s (API), 60 s (chunks).
- Retries: only idempotent requests, only on connection errors, 429, 500, 502, 503, 504.
  Exponential backoff with full jitter, base 1 s, cap 60 s, max 5 attempts. Honour `Retry-After`.
- Never poll faster than EGL: library sync is at most every 15 minutes while open, plus on
  user request. Update checks are at most hourly. Store responses are cached ≥ 10 minutes.
- No telemetry to Epic or anyone else. EGL sends `datarouter` telemetry; we don't.

## Error envelope

Epic services share one error JSON shape: **[OBS]** (probe) **[REF]**

```json
{
  "errorCode": "errors.com.epicgames.account.oauth.single_use_code_not_found",
  "errorMessage": "Sorry, the OAuth exchange or authorization code you are using is not valid.",
  "messageVars": [],
  "numericErrorCode": 18001,
  "originatingService": "com.epicgames.account.public",
  "intent": "prod",
  "correlationId": "<uuid>"
}
```

Some web-identity routes use `message` instead of `errorMessage`. `egs-api` maps the
codes we act on to typed errors (`errors.com.epicgames.common.oauth.invalid_token`,
`...oauth.corrective_action_required`, `...unauthorized`, `...forbidden`,
`...artifactdelivery.forbidden` (ticket expired), `...common.not_found`). Everything else
becomes `EpicError::Other { code, numeric }`. Messages shown to users are written by us,
never Epic's raw text.

## Sources

| Ref | What | License | Used for |
|---|---|---|---|
| Legendary (`derrod/legendary`, master, Oct 2026) | Python EGS client | GPL-3.0 | endpoint list, manifest/chunk formats, launch args, cloud saves, EGL import |
| Heroic fork of Legendary (`Heroic-Games-Launcher/legendary`, master) | adds download tickets, signed chunk URLs, EULA tracking | GPL-3.0 | downloads-cdn, launch (EULA) |
| `LeleDerGrasshalmi/FortniteEndpointsDocumentation` (`EpicGames/` tree) | community endpoint docs | no license declared | account, library playtime, wallet, fulfillment, platform service, web id routes |
| Local EGL install on the dev machine | `.item`, `LauncherInstalled.dat`, `.sdmeta`, logs, binary manifest header | user-owned data | egl-local-files, manifests (header), hosts |
| Phase 0 probes (2026-10-28) | public endpoints, invalid-code probes | n/a | store, web session bridge, image CDN |

These sources are used for **facts about the protocol only**. No code was copied or
translated line by line. Our implementation is written from these specs. Reference
sources were downloaded to `research/refs/` (git-ignored) for reading only.
