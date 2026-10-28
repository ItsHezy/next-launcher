# Cloud saves

Crate: `egs-api::saves` (HTTP) + `egs-manifest` (save manifest/chunk build and parse).
Host `DS` = `datastorage-public-service-liveegs.live.use1a.on.epicgames.com`. All [REF] (Legendary + Heroic fork)
unless marked.

## Which games, which folder

A game supports cloud saves when its catalog item has `customAttributes.CloudSaveFolder`.
Optional filters: `CloudIncludeList`, `CloudExcludeList` (comma-separated, trim whitespace).

`CloudSaveFolder.value` is a `/`-separated path with variables (case-insensitive):

| Variable | Windows value |
|---|---|
| `{installdir}` | game install path |
| `{epicid}` | account id |
| `{appdata}` | `%LOCALAPPDATA%` (yes, *local*: matches EGL behaviour per Legendary) |
| `{userdir}` | `%USERPROFILE%\Documents` |
| `{userprofile}` | `%USERPROFILE%` |
| `{usersavedgames}` | `%USERPROFILE%\Saved Games` |

Filter matching per relative file path: a pattern ending in `/` matches a directory prefix,
otherwise it's a suffix match, otherwise a glob (`*`). Include list = keep only matches.
Exclude list = drop matches.

## Remote layout

```
<accountId>/<appName>/manifests/<YYYY.MM.DD-HH.MM.SS>.manifest
<accountId>/<appName>/<chunk path, e.g. ChunksV4/<group>/<HASH>_<GUID>.chunk>
```

## Endpoints

| Action | Request |
|---|---|
| list everything for the user | `GET https://DS/api/v1/access/egstore/savesync/<accountId>/` |
| list manifests for a game | `GET https://DS/api/v1/access/egstore/savesync/<accountId>/<appName>/manifests/` |
| get read links for files | `POST https://DS/api/v1/access/egstore/savesync/<accountId>/<appName>/` `{"files": ["<path>", ...]}` |
| get write links (upload) | same POST with the paths to create. Response has `writeLink` per path |
| delete a file | `DELETE https://DS/api/v1/data/egstore/<full path>` |

Response: `{ "files": { "<full path>": { "readLink": "<signed url>", "writeLink": "<signed url>?", "hash": "...", "lastModified": "<iso>", "size": 123 } } }`.
Read and write links are pre-signed, so `GET`/`PUT` them **without** our bearer token.

## Save manifest (what we upload)

A normal binary manifest (manifests.md), built like this:

- `meta.app_name` = `<appName><accountId>`, `meta.build_version` = UTC time `YYYY.MM.DD-HH.MM.SS`
  (also the manifest file name).
- `custom_fields`: `CloudSaveFolder` (and `CloudSaveFolder_MAC` if the catalog has it), copied from the catalog.
- Files sorted case-insensitively by relative path. Bytes are packed into chunks of **1 MiB**, and the last
  chunk is zero-padded to 1 MiB before hashing (reference behaviour).
- Chunk: header v3, `stored_as` = zlib, `hash_type` = 3, rolling hash + SHA-1 of the padded data.
- Upload order: POST all paths (chunks + manifest) → PUT each chunk to its `writeLink` → PUT the manifest last.
  A manifest is never visible before its chunks exist.

## Download

1. List manifests and take the newest (by name/timestamp).
2. GET it via `readLink`, then parse.
3. POST its chunk paths to get read links (batch ≤ 100 **[GUESS]**). GET them with ≤ 8 concurrent requests.
4. Write each file to a temp name in the save folder, then rename. A backup of the overwritten local
   files is kept in `%LOCALAPPDATA%\NextLauncher\save-backups\<appName>\<timestamp>\` (last 3 kept).

## Sync decision (Phase 8)

Inputs: newest local file mtime (UTC), newest remote manifest time, our stored last-sync time.

| Situation | Result |
|---|---|
| no remote | upload |
| no local | download |
| only remote changed since last sync (> 60 s) | download |
| only local changed since last sync | upload |
| both changed since last sync | **conflict** → setting: ask (default) / prefer local / prefer cloud |
| neither | nothing |

Sync runs before launch ("sync on start") and after exit ("sync on exit"). It's per game and
switchable in Settings → Cloud saves / Manage games.

## Cleanup

Orphaned chunks (not referenced by any manifest) may be deleted only by an explicit
"Clean up cloud saves" action with a confirmation. Never automatically.
