# Cloud saves

Crate: `egs-api::saves` (HTTP) + `egs-manifest::savegame` (save manifest/chunk build and parse);
sync logic in `src-tauri/src/saves.rs` (ADR 0012).
Host `DS` = `datastorage-public-service-liveegs.live.use1a.on.epicgames.com`. [REF] (Legendary + Heroic
fork); the Phase 8 live check below confirms listing, read links, write links, upload and
download **[OBS]**.

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
3. The game listing already carries a read link for every chunk **[OBS]**, so no extra request is
   needed; chunks are fetched one after another (saves are small).
4. Write each file to a temp name in the save folder, then rename, and set its modified time to the
   manifest's time (so the next check sees "in sync"). Before writing, the local files about to be
   replaced are copied to `%LOCALAPPDATA%\NextLauncher\save-backups\<appName>\<timestamp>\` (last 3 kept).
   Only files that pass `CloudIncludeList`/`CloudExcludeList` are written; other files in the folder are
   never touched or deleted. Each file's SHA-1 is checked before anything is written.

## Sync decision (as built)

Inputs: newest local save file mtime, the newest cloud manifest (name and time), and our record of
the last agreement (`internal.saves.<app>` = manifest name + local mtime at that moment).

| Situation | Result |
|---|---|
| no saves anywhere | nothing |
| no remote | upload |
| no local | download |
| record: local newer than recorded (> 2 s), same newest manifest | upload |
| record: a different newest manifest, local unchanged | download |
| record: both changed | **conflict** → setting: ask (default) / prefer this PC / prefer cloud |
| record: neither changed | in sync |
| no record yet (first sync on this PC): local and cloud within 60 s | in sync (record it) |
| no record yet, times differ | **conflict** (never guess on the first sync) |

Using a record instead of comparing clocks alone (Legendary's approach) means a save edited on
another PC after our last sync is noticed even when this PC's clock is ahead.

Sync runs before launch ("sync on start") and after exit ("sync on exit"). It's per game and
switchable in Settings → Cloud saves / Manage games.

## Cleanup

Orphaned chunks (not referenced by any manifest) may be deleted only by an explicit
"Clean up cloud saves" action with a confirmation. Never automatically.

## Phase 8 live check (2026-10-29, owner's account, *while True: learn()* only)

- `CloudSaveFolder` = `{InstallDir}/Saves/` (so every local read and write stayed inside
  `test-installs/`); no include/exclude lists. 12 of the owner's installable games declare cloud saves.
- The account already had a cloud save for this game from 2024-10-24 (written by EGL). First sync
  on this PC: local file from today's test launches vs that save → **conflict**, nothing changed.
- Download of the EGL-written save: 4 files (≈ 490 KB), SHA-1 verified, mtimes set to the save time,
  the replaced local file backed up first. → EGL save manifests and chunks read correctly.
- Upload of the same content: 4 files in 1 chunk accepted by Epic's storage; after moving the local
  files aside, auto-sync downloaded our upload and the files were byte-identical.
- Simulated conflict (local touched + an older recorded manifest): detected, files untouched with
  "ask"; resolving with "use cloud" restored them and kept a second backup.
- Launch hooks: before start "in sync" (nothing transferred); after exit the game's updated saves
  were uploaded 2 s after it closed.
- Not verified: the official launcher reading a save that Next Launcher uploaded (same format as
  Legendary's, which EGL reads); games with `{AppData}`/`{UserDir}` folders (dev builds refuse save
  folders outside the development root).
