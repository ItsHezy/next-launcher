# Manifests and chunks

Crate: `egs-manifest` (pure parsing, no I/O beyond byte slices). Unit tests use fixture
manifests (Phase 4). All integers are **little-endian** unless stated otherwise.

A *manifest* describes one build of one app: a list of files, each made of *chunk parts*.
Each part is a byte range inside a *chunk*, a compressed blob of at most a few MiB
(usually ≤ 1 MiB uncompressed) on the CDN. Chunks are shared between files and between builds.

How we get a manifest: [downloads-cdn.md](downloads-cdn.md).

## Format detection

- First byte `{` → JSON manifest (old titles). **[REF]**
- First 4 bytes `0C C0 BE 44` (u32 `0x44BEC00C`) → binary manifest. **[OBS]** on the dev machine's
  `F:\Fortnite\.egstore\*.manifest`.

## Primitive: FString — [REF]

```
i32 len
len > 0 : len bytes ASCII/UTF-8, last byte is NUL      → string = bytes[..len-1]
len < 0 : (-len)*2 bytes UTF-16LE, last 2 bytes are NUL → string = utf16[..(-len)-1]
len = 0 : empty string, no bytes follow
```

A GUID is 4 × u32 (16 bytes). Text form: `%08X` ×4 (chunk paths, secrets) or lower-case
`%08x-%08x-%08x-%08x` (display only).

## Binary manifest

### Header

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 4 | magic `0x44BEC00C` | [OBS] |
| 4 | 4 | `header_size` | 41 for version < 22. [OBS] 41 on dev machine |
| 8 | 4 | `size_uncompressed` | [OBS] |
| 12 | 4 | `size_compressed` | [OBS] = file length − header_size |
| 16 | 20 | `sha1` of the **uncompressed** body | [OBS] position, [REF] semantics |
| 36 | 1 | `stored_as` flags: bit0 = zlib, bit1 = encrypted | [OBS] value 1 |
| 37 | 4 | `version` (serialisation feature level) | [OBS] 17 |
| 41 | 16 | `secret_guid` (version ≥ 22) | [REF] |
| 57 | 16 | `encryption_tag` (version ≥ 22, AES-GCM tag) | [REF] |

Always seek to `header_size` after reading the known fields: Epic may append fields we don't know.
Body = bytes from `header_size` to end. If bit0: zlib-inflate (body starts `78 9C` [OBS]),
then verify `sha1(body) == header.sha1`. Mismatch is a hard error.

Feature levels seen in references (Epic's `EFeatureLevel`): 17 variable-size chunks
(no window-size info in chunk header), 18 variable-size chunks, 19 MD5 + MIME in file list,
20 SHA-256 in file list, 21 uninstall action, 22 ChunksV5 + secret paths, 24 encrypted
manifest data. **[REF]** Unknown higher levels: parse what we can, warn, and refuse to
*install* if a section's size doesn't add up.

### Body sections

Four sections in order (five when encrypted). Each starts with `u32 size` (counting from the section
start) and `u8 version`. After parsing, **always seek to `section_start + size`**.

#### 1. ManifestMeta — [REF]

```
u32 size, u8 data_version, u32 feature_level, u8 is_file_data, u32 app_id
FString app_name, build_version, launch_exe, launch_command
u32 n; FString prereq_ids[n]
FString prereq_name, prereq_path, prereq_args
if data_version >= 1: FString build_id
if data_version >= 2: FString uninstall_action_path, uninstall_action_args
```

`feature_level` here, not the header `version`, decides the chunk-list layout and chunk directory.

#### 2. ChunkDataList (column-oriented) — [REF]

```
u32 size, u8 version, u32 count
GUID      guid[count]
u64       rolling_hash[count]
u8[20]    sha1[count]                 (sha1 of uncompressed chunk data)
u8        group_num[count]            (0..99, part of the CDN path)
u32       window_size[count]          (uncompressed size)
i64       file_size[count]            (compressed size = bytes to download)
if feature_level >= 22:
  GUID    secret_guid[count]
  u32     window_size_compressed[count]
  u8[16]  encryption_tag[count]
```

#### 3. FileManifestList (column-oriented) — [REF]

```
u32 size, u8 version, u32 count
FString   filename[count]             (relative, '/' separators)
FString   symlink_target[count]
u8[20]    sha1[count]                 (sha1 of the whole file)
u8        flags[count]                (bit0 read-only, bit1 compressed, bit2 executable)
for each file: u32 n; FString install_tags[n]
for each file: u32 n; n × ChunkPart
if version >= 1:
  for each file: u32 has_md5; if has_md5 != 0: u8[16] md5
  FString mime_type[count]
if version >= 2:
  u8[32]  sha256[count]
```

`ChunkPart = u32 part_size (28), GUID chunk_guid, u32 offset_in_chunk, u32 size`. Skip to
`part_start + part_size` (it may grow). A file's size is the sum of its part sizes, and each
part's `file_offset` is the running sum.

#### 4. CustomFields — [REF]

```
u32 size, u8 version, u32 count
FString keys[count]; FString values[count]
```

Seen: `CloudSaveFolder`, `CloudSaveFolder_MAC` (save manifests), others free-form.

#### 5. EncryptedData (only if header bit1 set and feature_level >= 24) — [REF]

```
u32 size, u8 version, u32 blob_size
blob:
  u32 h_size, u32 h_version, u8 h_stored_as (bit0 zlib), u32 h_uncompressed, u32 h_compressed,
  u32 iv_len, u8 iv[iv_len]
  u32 ct_len, u8 ciphertext[ct_len]
```

The sensitive strings that are blank in the clear sections are inside the ciphertext. After
decrypting (and inflating if `h_stored_as & 1`), the plaintext holds, in order:
`launch_exe, launch_command, u32 n + prereq_ids[n], prereq_name, prereq_path, prereq_args,
uninstall_action_path, uninstall_action_args`, then for every file in list order
`filename, symlink_target`.

- Cipher: AES-GCM, key = `hex_decode(secrets[GUID_HEX(header.secret_guid)])`, nonce = `iv`,
  tag = header `encryption_tag`. **[REF]**
- `secrets` comes **only** from the assets v2 response for the signed-in user
  (`elements[0].secrets`, downloads-cdn.md). Epic hands the key to the entitled user, as it does for EGL.
  **No secret ⇒ we cannot install, and we show "Epic didn't provide the files for this game" (no workaround).**

## Chunk directory and path — [REF]

| feature_level | dir |
|---|---|
| < 3 | `Chunks` |
| 3–5 | `ChunksV2` |
| 6–14 | `ChunksV3` |
| 15–21 | `ChunksV4` |
| ≥ 22 | `ChunksV5` |

Path relative to a CDN base URL (`.../CloudDir`):

- feature_level < 22: `{dir}/{group:02}/{rolling_hash:016X}_{GUID_HEX}.chunk`
- feature_level ≥ 22: `{dir}/{S}/{group:02}/{b64u(rolling_hash as 8 LE bytes)}_{b64u(guid as 16 LE bytes)}.chunk`,
  where `S = b64u(secret_guid as 16 LE bytes)` or the literal `plain` when the secret guid is all zero,
  and `b64u` = URL-safe base64 **without padding**.
- If `group_num` is absent (never in binary, possible in JSON): `crc32(guid as 16 LE bytes) % 100`.

**[OBS] Phase 4:** a feature level 13 build downloaded end to end from
`ChunksV3/{group}/{hash}_{GUID}.chunk` (816 chunks, all SHA-1 checks passed). Feature levels 17 and 21 are parsed
live (34 owned games; Fortnite's level 21 manifest, 151,991 chunks and 891 files, parses in 38 ms)
but not downloaded yet. Level 22+ paths are covered by unit tests only.

## Chunk container (`.chunk` file) — [REF]

| Offset | Size | Field | Since header v |
|---|---|---|---|
| 0 | 4 | magic `0xB1FE3AA2` | 1 |
| 4 | 4 | header_version | 1 |
| 8 | 4 | header_size (66 for v3, 98 for v4) | 1 |
| 12 | 4 | data_size (stored bytes after header) | 1 |
| 16 | 16 | guid | 1 |
| 32 | 8 | rolling_hash | 1 |
| 40 | 1 | stored_as (bit0 zlib, bit1 encrypted) | 1 |
| 41 | 20 | sha1 of uncompressed data | 2 |
| 61 | 1 | hash_type (1 rolling, 2 sha1, 3 both) | 2 |
| 62 | 4 | uncompressed_size (default 1 MiB) | 3 |
| 66 | 16 | secret_guid | 4 |
| 82 | 16 | encryption_tag | 4 |

Decode: check magic, read header fields up to `header_size`. Stored data = next `data_size` bytes.
If encrypted: AES-GCM with key `secrets[GUID_HEX(secret_guid)]`, nonce = first 12 bytes of
`sha1`, tag = `encryption_tag`. Then if bit0: zlib-inflate. Verify `sha1(data)` against
the manifest's CDL entry. Mismatch ⇒ discard and re-download (max 3 tries, then fail the job).

## Rolling hash — [REF]

64-bit, table-driven, polynomial `0xC96C5795D7870F42` (reflected CRC-64/ECMA table):

```
table[i] = i; repeat 8×: table[i] = (table[i] & 1) ? (table[i] >> 1) ^ POLY : table[i] >> 1
h = 0; for byte b: h = rotl64(h, 1) ^ table[b]
```

Only needed when *creating* chunks (cloud-save upload). Verification uses SHA-1.

## JSON manifest — [REF]

```json
{
  "ManifestFileVersion": "013000000000", "bIsFileData": false, "AppID": "000000000000",
  "AppNameString": "<appName>", "BuildVersionString": "<build>",
  "LaunchExeString": "Bin/Game.exe", "LaunchCommand": "", "PrereqIds": [], "PrereqName": "",
  "PrereqPath": "", "PrereqArgs": "",
  "FileManifestList": [ { "Filename": "Bin/Game.exe", "FileHash": "<blob 20 bytes>",
      "bIsReadOnly": false, "bIsCompressed": false, "bIsUnixExecutable": false, "InstallTags": [],
      "FileChunkParts": [ { "Guid": "<32 hex>", "Offset": "<blob>", "Size": "<blob>" } ] } ],
  "ChunkHashList": { "<32 hex>": "<blob u64>" }, "ChunkShaList": { "<32 hex>": "<40 hex>" },
  "DataGroupList": { "<32 hex>": "<blob u8>" }, "ChunkFilesizeList": { "<32 hex>": "<blob i64>" },
  "CustomFields": {}
}
```

- **Blob numbers**: each byte written as 3 decimal digits, bytes in little-endian order.
  `"013000000000"` = bytes `[13,0,0,0]` = 13. Decode: `sum(int(s[3i..3i+3]) << 8i)`.
- `FileHash` blob decodes to the 20 SHA-1 bytes in little-endian order (byte i = digit group i).
- GUID strings are 32 hex chars read **big-endian** as 4 × u32.
- JSON manifests have no window size. Assume 1 MiB.

## Install tags and selective download

- A file with **no** install tags is always installed. A file with tags is installed when any
  of its tags is selected. **[REF]**
- The menu of optional components comes from **SDMeta** JSON. **[OBS]** local files
  `C:\ProgramData\Epic\EpicGamesLauncher\Data\SDMeta\<sha256>.sdmeta`, UTF-8 or UTF-16LE with BOM:

  ```json
  { "DataType": "SelDlMeta",
    "Data": [ { "Title": "...", "Description": "...", "IsRequired": "true",
                "UniqueId": "<id>", "Tags": ["<tag>", "..."],
                "Title_fr": "", "Description_fr": "" } ] }
  ```
  `UniqueId` matches the `InstallComponents` entries in EGL's `.item` files. **[OBS]**
  Remote copy: `selectiveDownload[].uri` in the assets v2 response. **[REF]**
- Selection = tags of all `IsRequired` components + user-chosen ones. Stored per install.

## Updates (diff) — [REF]

Compare old vs new FileManifestList by `filename`: same sha1 ⇒ unchanged, different ⇒
changed, missing in new ⇒ removed (deleted after the update commits), missing in old ⇒ added.
For changed files, chunk parts that already exist in the old file at a known offset are
copied locally instead of downloaded (see downloads-cdn.md "reuse").

Optional optimisation: `GET {base_url}/Deltas/{new_build_id}/{old_build_id}.delta` (a manifest
holding only changed files and new chunks, merged over the new manifest). 404 is normal.
`build_id` = meta `build_id` if present, else
`b64u_nopad(sha1(u32 app_id LE ‖ app_name ‖ build_version ‖ launch_exe ‖ launch_command))` with `+/` → `-_`.
