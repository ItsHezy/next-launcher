# Next Launcher: progress

## Phase checklist

- [x] **Phase 0: Research** (2026-10-28). Specs in `docs/protocol/`, ledger in `research/notes.md`.
- [x] **Phase 1: Skeleton and speed** (2026-10-28). Full UI on mock data, measured in `docs/perf.md`.
- [x] **Phase 2: Auth** (2026-10-28). Live sign-in, silent restore and signed-in Epic pages verified on the owner's account.
- [x] **Phase 3: Library** (2026-10-29). Real library synced live (35 games + 3 DLC, real covers); EGL import built, live adoption awaits the owner.
- [x] **Phase 4: Downloads and installs** (2026-10-29). Live install, verify, repair, move and resume of a small free game into `test-installs/`.
- [ ] Phase 5: Launch
- [ ] Phase 6: Store
- [ ] Phase 7: Settings, wallet, profile menu
- [ ] Phase 8: Cloud saves
- [ ] Phase 9: Polish and ship

## Phase 4 summary

What works (verified live on the owner's account, test game *while True: learn()*, into
`test-installs/` with the development guard on):
- Install: Epic's build info and manifest (SHA-1 checked, CDN fallback), install size and free
  space shown first, optional components from Epic's selective-download data. 153 files /
  153 MB in 21 s with a 16 MB peak chunk cache. Progress, speed and time left in the sidebar card.
- Engine (`egs-download`): parallel chunk downloads across Epic's CDNs, zlib, AES for encrypted
  builds, SHA-1 on every chunk and file, atomic `.nl-tmp` → rename, pause / resume / cancel,
  live speed limit, chunk reuse and diffing for updates.
- Resume after the app is killed mid-download: finished files are skipped on the next start.
- Verify and repair: one flipped byte found in 0.36 s over 153 files; repair re-downloaded only
  that file (5 chunks) through a staging folder; re-verify clean.
- Updates and repairs are staged, so a cancelled update leaves the old build playable.
- Uninstall removes only the game's own files. Move: instant on the same drive; across drives
  copy → check → delete, with cancel and progress. Both ask twice for folders shared with the
  official launcher.
- Manifest parser for binary and JSON manifests at every feature level, plus the chunk format;
  Fortnite's 151,991-chunk manifest parses in 38 ms.
- 77 Rust tests (16 api, 10 auth, 13 cache, 17 download, 3 import, 9 manifest + 1 opt-in test on
  a real manifest, 9 app).

Measured: cold 416 ms, warm 406 ms (unchanged, at the 400 ms line), idle 108.6 MB / 0.01 % CPU
with a game installed, installer 3.10 MiB. Details in `docs/perf.md`.

Known gaps:
- None of the owner's 34 installable games use signed CDN URLs or encryption, so those paths
  are unit-tested only. Cross-drive move is unit-tested only (the only other drive holds real games).
- Pause, resume and cancel from the download card are unit-tested in the engine, not clicked
  through live (resume after a forced kill was tested live).
- Prerequisite installers and the manifest's uninstall action wait for Phase 5.
- Live adoption of the owner's Fortnite install (Phase 3) still awaits the owner.

## Phase 3 summary

What works (verified live on the owner's account):
- Sync of owned items, Windows builds, catalog metadata and playtime into SQLite, after sign-in and
  every 15 min while open, plus a refresh button. First sync 2.1 s; unchanged library 0.74 s via
  Epic's state token (no catalog or image traffic). Mock games removed on first real sync.
- Cover art resized by Epic's CDN, downloaded once (8 in flight, atomic writes), served from disk;
  tiles update in place. 76/76 covers for the owner's library.
- UI merges snapshots by identity, so only changed tiles re-render. "Not on Windows" and
  "Opens in <launcher>" states for owned items we can't install.
- DLC stays attached to its game (3 hidden from the grid). Library is per account.
- Import from the official launcher: read-only scan, adoption only on an explicit two-step
  confirmation naming the folder; copies the build manifest into our data folder, writes nothing on
  the game drive or in EGL's data.
- 46 Rust tests (14 api, 10 auth, 11 cache, 3 import, 8 app).

Measured: warm 372 ms on the real profile (409 ms synthetic, at the 400 ms line), cold 431 ms,
idle 108.9 MB / 0.01 % CPU with 35 covers.

Known gaps:
- Live adoption of the owner's Fortnite install not yet tried (needs the owner's go-ahead).
- "Recently played" stays empty until launches are tracked (Phase 5): Epic only reports totals.
- Hidden-games management and "Import favorites from Epic" are not built yet.

## Phase 2 summary

What works (verified live on the owner's account):
- Sign-in on Epic's own page in a separate window, with PKCE. 2FA and captcha stay on Epic's page.
- Refresh token in Windows Credential Manager (chunked if ever too long); access token in memory only.
- Silent restore after launch (~1.2 s, after first paint); proactive refresh 5 min before expiry;
  offline handling keeps the saved session and cached library, retrying with backoff.
- Sign-out: ends the Epic session, deletes the credential, clears Epic web cookies.
- **No second login**: Epic account pages open signed in via the exchange-code bridge, verified with
  all web cookies deleted. Settings → Account/Wallet rows with a known page now open it.
- Log redaction for tokens, codes, auth headers, launch auth args and emails (unit-tested).
- `egs-api` (HTTP client, error envelope, retry/backoff, OAuth + account endpoints) and `egs-auth`
  (PKCE, login callback, keyring store, session) with 20 tests incl. a real Credential Manager round-trip.

Measured: cold 469 ms, warm 485 ms (gap unchanged), idle signed-in 100 MB / 0.01 % CPU, installer 2.84 MiB.

Known gaps:
- Mock games remain until Phase 3 syncs the real library.
- Online-status selector, achievements, wallet balance and code redemption wait for Phase 7.
- Permission *action bits* (update vs read) are checked in Phase 7; resource names are present.

## Phase 1 summary

What works:
- Cargo workspace with the seven `egs-*` crates (six documented stubs, `egs-cache` implemented:
  schema v1 from section 7, versioned migrations, snapshot, settings, pinning, image index, 5 tests).
- Tauri 2.12 shell: frameless hidden window, custom window controls, startup pipeline (ADR 0006),
  `img` protocol, single-instance, rotating logs, typed IPC via tauri-specta.
- The whole section 6 UI on mock data: sidebar (Quick Launch, download card, Settings),
  top bar (segmented nav, search, wishlist, cart, wallet popover, settings, profile menu, window
  controls), Library (count, filters, recently played, virtualised grid, pin via context menu),
  Store (chips, grid), Unreal Engine / Fab placeholders, full Settings (11 sections, search).
- Tooling: `--bench-startup`, `--bench-scroll`, `--dev-route`, `tools/bench-startup.mjs`,
  `tools/measure-idle.ps1`, `tools/screenshot.ps1`, `tools/bundle-budget.mjs`, CI workflow.

Measured vs budget (details in `docs/perf.md`):

| Metric | Budget | Measured |
|---|---|---|
| Cold start | ≤ 1.0 s | 474 ms ✅ |
| Warm start | ≤ 400 ms | 458 ms ❌ (WebView2 creation alone is ~345 ms) |
| Tab switch visited / first | ≤ 50 / ≤ 300 ms | 3–7 / 10–11 ms ✅ |
| Idle RAM / CPU | ≤ 150 MB / ~0 % | 92–96 MB / 0.01–0.03 % ✅ |
| Installer | ≤ 15 MB | 2.07 MiB ✅ |
| Scroll, 600 games | 60 fps | 198 fps (200 Hz display) ✅ |
| JS initial / total (gz) | 80 / 250 KB | 29.7 / 39.2 KB ✅ |

Known gaps:
- Warm start misses by ~58 ms (options in `docs/perf.md`; needs a decision).
- The design mockup link needs a claude.ai sign-in, so the UI follows the section 6 text, not the screenshots.
- Text-contrast rules required usage constraints on `--text-3` and derived accent shades (tokens unchanged).
- Covers are placeholders until Phase 3 downloads real art.
- Installer is unsigned until Phase 9.

## Open decisions for the owner

1. **License** (ADR 0001): GPL-3.0-or-later recommended, or MIT.
2. **Image format** (ADR 0004): store CDN-resized JPEG/WebP as-is instead of transcoding to WebP.
3. **Warm start**: now ~370–410 ms (at the 400 ms line). Options unchanged: accept plus tray reopen
   (Phase 9), or try WebView2 flags (risk to scroll smoothness).
4. **Mockup access**: sign in to claude.ai in the app's browser pane, or share screenshots.

## Conventions

- Test installs go to `C:\Users\hezy\Documents\Claude\Next\test-installs\` (git-ignored).
- Real installs (e.g. `F:\Fortnite`) and `C:\ProgramData\Epic\...` are read-only unless the owner confirms in chat.
- Debug builds keep data in `.dev-data/`. Release builds use `%LOCALAPPDATA%\NextLauncher`.
