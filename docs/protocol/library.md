# Library: owned items, builds, entitlements, playtime

Crate: `egs-api::library`, synced into `egs-cache` by `src-tauri`'s library service.

Hosts: `LAUNCHER` = `launcher-public-service-prod06.ol.epicgames.com`,
`LIBRARY` = `library-service.live.use1a.on.epicgames.com`,
`ENT` = `entitlement-public-service-prod08.ol.epicgames.com`,
`CATALOG` = `catalog-public-service-prod06.ol.epicgames.com`. All calls use `Authorization: bearer <access>`.

## Endpoints

### Assets (installable builds the account may download) — [REF]

`GET https://LAUNCHER/launcher/api/public/assets/Windows?label=Live`

```json
[
  { "appName": "<appName>", "labelName": "Live-Windows", "buildVersion": "1.2.3-CL-123-Windows",
    "catalogItemId": "<itemId>", "namespace": "<ns>", "assetId": "<appName>",
    "metadata": { "installationPoolId": "<pool>" }, "sidecarRvn": 0 }
]
```

- Lists only what the user is entitled to download. **A game missing from this list cannot
  be installed by us.** That is the ownership gate, and we never work around it.
- `buildVersion` changing for an installed app means an **update is available**.
- `namespace == "ue"` entries are Unreal Engine builds (shown under the Unreal Engine tab, not Library).
- `sidecarRvn` > 0: an app sidecar config exists (EOS deployment id for launch; launch.md).

### Library items (everything owned, including non-installable) — [REF]

`GET https://LIBRARY/library/api/public/items?includeMetadata=true[&cursor=<c>][&platform=Windows]`

```json
{
  "responseMetadata": { "nextCursor": "<b64 json>", "stateToken": "<uuid>" },
  "records": [
    { "namespace": "<ns>", "catalogItemId": "<itemId>", "appName": "<appName>",
      "productId": "<productId>", "sandboxName": "<title>", "sandboxType": "PUBLIC",
      "recordType": "APPLICATION", "acquisitionDate": "<iso>", "dependencies": [] }
  ]
}
```

- Paginate until `nextCursor` is absent.
- `recordType` other than `APPLICATION` (e.g. `SUBSCRIPTION`) is not a game tile.
- Records with no matching asset are owned titles we cannot install (third-party launcher
  titles such as EA/Ubisoft-managed games, or items not available on Windows). They are shown as "Not
  installable here" with an "Open in store" link. Hidden by "Show uninstalled games = off" only if
  the user wants that.
- `GET https://LAUNCHER/launcher/api/public/library/items` is a deprecated proxy. Don't use it. **[REF]**

### Library state token (cheap change check) — [REF]

`GET https://LIBRARY/library/api/public/stateToken/<stateToken>/status` → `{"valid": true|false}`

If still valid, the owned-items list hasn't changed since the token was issued. Skip
re-paginating items. Assets are still fetched, because build versions change independently.

### Catalog metadata (title, images, custom attributes) — [REF]

`GET https://CATALOG/catalog/api/shared/namespace/<ns>/bulk/items?id=<itemId>[&id=<itemId>...]&includeDLCDetails=true&includeMainGameDetails=true&country=<CC>&locale=<lc>`

Returns `{ "<itemId>": CatalogItem }`. Fields we use:

| Field | Use |
|---|---|
| `title`, `description`, `developer`, `developerId` | tile text, detail |
| `keyImages[] {type, url, width, height, md5}` | covers (catalog-and-images.md) |
| `categories[] {path}` | `games`, `applications`, `addons`, `addons/launchable`, `digitalextras` |
| `customAttributes{ Key: {type, value} }` | `CanRunOffline`, `OwnershipToken`, `CloudSaveFolder`, `CloudSaveFolder_MAC`, `CloudIncludeList`, `CloudExcludeList`, `AdditionalCommandLine`, `FolderName`, `ThirdPartyManagedApp`, `ThirdPartyManagedProvider`, `PresenceId`, `MonitorPresence`, `partnerLinkType`, `partnerLinkId` |
| `mainGameItem` | present ⇒ this item is DLC of that game |
| `dlcItemList[]` | DLC of this game |
| `releaseInfo[] {appId, platform}` | `appId` is the *artifact id* used by playtime APIs |
| `lastModifiedDate` | cache freshness |

Batching: one request per namespace with up to 50 `id`s. **[GUESS]** on the max count;
start at 25 and verify. Cached in SQLite and re-fetched only when the asset's
`buildVersion` changes, the item is new, or the cache entry is older than 7 days.

### Entitlements — [REF]

`GET https://ENT/entitlement/api/account/<accountId>/entitlements?start=<n>&count=1000` → array, paginate while length == 1000.

Each has `entitlementName`, `namespace`, `catalogItemId`, `entitlementType`, `active`,
`consumable`. Used only to mark which DLC are owned. Fetched only when assets changed (as Legendary does).

### Playtime — [REF]

- Read all: `GET https://LIBRARY/library/api/public/playtime/account/<accountId>/all` →
  `[{"accountId","artifactId","totalTime"}]` (seconds). Merged into `games.playtime`.
- Report a session: `PUT https://LIBRARY/library/api/public/playtime/account/<accountId>`

  ```json
  { "machineId": "<uuid v4, stable per install>", "artifactId": "<appId from releaseInfo>",
    "startTime": "<iso>", "endTime": "<iso>", "startSegment": true, "endSegment": true }
  ```
  → 204. Sent once when the game process exits (launch.md). Locally accumulated playtime is
  kept even if the PUT fails. It's retried from the `events_log` queue on next start.

### Collections — [REF]

`GET https://LIBRARY/library/api/public/collection/account/<accountId>` → `collections[]`;
`EGS_STATIC_FAV_COLLECTION` is EGL's "Favorites". Quick Launch pins are **local only**
(SQLite `games.pinned`). We don't write to Epic's collections. Optional, Phase 3: offer
"Import favorites from Epic as Quick Launch pins" once.

## Sync algorithm (Phase 3)

```
on start (after first paint) and every 15 min while open, or on manual refresh:
  assets  = GET assets/Windows?label=Live                     # always (build updates)
  if stored stateToken && status(stateToken).valid:
      items = cached items
  else:
      items = paginate library items; store new stateToken
  new_or_changed = items/assets whose (ns,itemId) is new or whose buildVersion changed
  catalog = bulk items for new_or_changed, grouped by namespace (≤ 6 concurrent)
  if assets changed: entitlements = paginate entitlements
  diff vs SQLite → write in one transaction → emit library://updated { changed_ids }
  enqueue cover downloads for new keyImages (catalog-and-images.md)
```

The UI only re-renders tiles whose id is in `changed_ids`.
