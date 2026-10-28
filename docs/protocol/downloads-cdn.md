# Downloads: manifest lookup, CDN layout, signed URLs, verify

Crates: `egs-api::launcher` (endpoints), `egs-download` (engine), `egs-manifest` (formats).

## 1. Get the build's manifest — [REF]

`GET https://launcher-public-service-prod06.ol.epicgames.com/launcher/api/public/assets/v2/platform/Windows/namespace/<ns>/catalogItem/<itemId>/app/<appName>/label/Live` (bearer)

Requires `launcher:download:{label}:{appName} READ`, which Epic grants only for owned apps.
**403 here means not owned or not downloadable: we stop and say so.**

```json
{
  "elements": [{
    "appName": "<appName>", "labelName": "Live-Windows", "buildVersion": "<build>",
    "hash": "<sha1 hex of the manifest file>",
    "useSignedUrl": false,
    "isPreloaded": false,
    "metadata": { "installationPoolId": "<pool>" },
    "manifests": [
      { "uri": "https://<cdn>/Builds/<org-or-app>/CloudDir/<name>.manifest",
        "queryParams": [ { "name": "cf_token", "value": "<redacted>" } ] }
    ],
    "selectiveDownload": [ { "uri": "https://selective-download-egs.distro.on.epicgames.com/.../<sha256>.sdmeta",
                             "queryParams": [ { "name": "cf_token", "value": "<redacted>" } ] } ],
    "secrets": { "<GUID_HEX>": "<key hex>" }
  }]
}
```

- More than one element has never been seen. Treat it as an error. **[REF]**
- Manifest URL = `uri` + `?` + `name=value&...` from `queryParams` (may be absent).
- Try `manifests[]` in order. Accept the first 200 whose `sha1(body) == hash`.
- **CDN base URLs** = each `uri` minus its last path segment, de-duplicated, order kept. EGL
  stores the same list as `BaseURLs` in `.item` files. **[OBS]** e.g.
  `https://egs-cloudfront-chunks.epicgamescdn.com/Builds/Fortnite/CloudDir`,
  `https://epicgames-download1.akamaized.net/...`, `https://egdownload.fastly-edge.com/...`.
- `secrets` (manifest/chunk AES keys) present only for encrypted builds. Kept in memory for the
  job, then persisted next to the install's manifest copy, since verify/repair needs them.
  Never logged.
- `isPreloaded: true`: a pre-load build (installable before release, not launchable until
  unlock). Show "Pre-loaded" state.

Public list of distribution hosts (no auth): `GET https://launcher-public-service-prod06.ol.epicgames.com/launcher/api/public/distributionpoints`
→ `{"distributions": ["https://cloudflare.epicgamescdn.com/", "https://download.epicgames.com/", ...]}`. **[REF]**

## 2. Chunk URLs

### Unsigned (`useSignedUrl: false`) — [REF]

`{base_url}/{chunk_path}` with `chunk_path` from [manifests.md](manifests.md#chunk-directory-and-path).
Workers rotate to the next base URL for a chunk after 2 failures on one base. Each
host's error rate is tracked; a host above 20% failures is demoted for the rest of the job.

### Signed (`useSignedUrl: true`) — [REF] Heroic fork + community docs

1. Ticket:
   `POST https://launcher-public-service-prod.ak.epicgames.com/launcher/api/public/assets/v2/ticket` (bearer)
   ```json
   { "catalogItemId": "<itemId>", "buildVersion": "<build>", "appName": "<appName>",
     "namespace": "<ns>", "label": "Live", "platform": "Windows" }
   ```
   → `{ "signedTicket": "<ticket>", "expiresAt": "<iso>" }`
   (`label`/`platform` come from splitting `labelName` `Live-Windows`.)
2. Sign a batch of chunk paths (≤ 50 per request):
   `POST https://artifact-delivery-service-public-prod.ol.epicgames.com/artifact-delivery/api/public/v1/delivery/account/<accountId>/downloadurls` (bearer)
   ```json
   { "signedTicket": "<ticket>", "chunkIds": ["ChunksV4/12/ABCD..._....chunk"], "deltaId": "", "clientMetrics": {} }
   ```
   → `{ "<chunk path>": "https://download.epicgames.com/...?Policy=...&Signature=...&Key-Pair-Id=..." }`
3. Rules: sign just ahead of the workers (bounded queue of about 2 batches). Request a new
   ticket when < 5 min remain or on `errors.com.epicgames.artifactdelivery.forbidden`
   ("Ticket has expired"). Signed URLs are themselves time-limited, so never persist them
   for resume. Re-sign after a restart.

## 3. Download engine (Phase 4 design summary)

Planning, from the old manifest (if updating), the new manifest and the selected install tags:

1. File diff: added / changed / removed / unchanged by file SHA-1 (manifests.md "Updates").
2. **Chunk reuse**: for a changed file, any new chunk part fully contained in the *old*
   file's chunk parts (same chunk guid, overlapping offset range) is read from the old file
   instead of downloaded.
3. Reference-count chunks across all files to download. Order files so files sharing
   chunks are adjacent, which minimises the in-memory chunk cache. Simulate the job to compute
   the peak cache. Cap it with a memory limit (default 256 MiB, much lower than Legendary's 1 GiB).
   Chunks that don't fit are evicted and re-fetched when needed again (counted in the estimate).
4. Disk space required = sum(new sizes of added/changed files) − sum(old sizes of changed/removed files)
   + largest single changed file (temp copy). Check free space on the target volume before
   starting. Block with a clear message if short.

Execution:

- N download workers (default 16), with a global token bucket for the speed limit (`governor`).
- Each chunk: download → (decrypt) → inflate → SHA-1 verify → cache.
- One writer task assembles files in order into `<file>.nl-tmp` and fsyncs. When a file completes,
  it checks the file SHA-1 and does an atomic rename over the target. Executable bits and read-only flags are
  ignored on Windows.
- **Resume**: `downloads` table rows hold the job's manifest hash and per-file completion
  (file name + SHA-1 verified). On restart, completed files are skipped and partially written
  `.nl-tmp` files are discarded (simpler and safe). Chunks are never cached on disk across
  restarts in v1.
- Pause = stop handing out chunk tasks and let in-flight ones finish. Cancel = pause plus delete
  `.nl-tmp` files. A cancelled *update* leaves the old install intact and launchable.
- Removed files are deleted only after every changed/added file has committed.

## 4. Verify / repair — [REF] semantics

Verify: for every file in the manifest (respecting install tags), compare size and SHA-1.
Missing, wrong size or hash mismatch ⇒ repair list. Repair = run the engine with only those
files as "changed" and no reuse from the broken copies. Hashing is streamed (`sha1` crate),
bounded parallelism (2 files at once on HDD, 4 on SSD), and shows progress.

## 5. Install metadata we keep per game

`games` row: install path, build version, manifest hash, selected install tags, secrets
(only if encrypted; stored in a separate keyring entry if large values allow, otherwise in an
ACL-restricted file next to the DB), base URLs, prerequisites info, `can_run_offline`,
`requires_ownership_token`. A copy of the raw manifest goes in
`%LOCALAPPDATA%\NextLauncher\manifests\<appName>\<build>.manifest` (needed for updates/verify/uninstall).

## 6. Prerequisites — [REF]

Manifest meta `prereq_ids/prereq_name/prereq_path/prereq_args` describe an installer
(typically a VC++ redistributable) shipped inside the game folder. EGL runs it after install.
We ask first ("<Game> needs <prereq_name>. Install now?"), run it with the given args,
remember success per prereq id (like EGL's `LastPrereqSucceededSHA1Hash` [OBS]), and never
silently elevate.

## 7. Uninstall and move

- Uninstall: if the manifest has `uninstall_action_path`, run it first (ask). Then delete exactly
  the files listed in the manifest plus empty directories, never the whole folder blindly.
  Delete our manifest copy last.
- Move: copy files (verified) to the new location, update `install_path`, then delete the old
  copy. Same-volume moves use rename.
- **Never** run either on an install we adopted from EGL without an explicit confirmation
  that names the folder (section 1 of the brief).
