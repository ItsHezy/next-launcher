# Next Launcher: progress

## Phase checklist

- [x] **Phase 0: Research** (2026-10-28). Specs in `docs/protocol/`, ledger in `research/notes.md`.
- [x] **Phase 1: Skeleton and speed** (2026-10-28). Full UI on mock data, measured in `docs/perf.md`.
- [x] **Phase 2: Auth** (2026-10-28). Live sign-in, silent restore and signed-in Epic pages verified on the owner's account.
- [x] **Phase 3: Library** (2026-10-29). Real library synced live (35 games + 3 DLC, real covers); EGL import built, live adoption awaits the owner.
- [x] **Phase 4: Downloads and installs** (2026-10-29). Live install, verify, repair, move and resume of a small free game into `test-installs/`.
- [x] **Phase 5: Launch** (2026-10-29). Test game started from the app, "Playing" shown, playtime recorded and reported to Epic.
- [x] **Phase 6: Store** (2026-10-29). Live store: Featured, free games, on sale, new releases, search, product pages; wishlist, cart and checkout on Epic's own pages, signed in.
- [x] **Phase 7: Settings, wallet, profile menu** (2026-10-29). Real account, connections and wallet data; in-app edits where Epic permits; every other row opens Epic's verified page, signed in.
- [x] **Phase 8: Cloud saves** (2026-10-29). Download, upload, conflict handling and launch/exit sync verified with one game; local saves always backed up before a download.
- [~] **Phase 9: Polish and ship** (2026-10-30). Tray, autostart, notifications, crash-safe updates, accessibility, notices done. Signing, updater and license wait for the owner.

## Phase 9 summary (what doesn't need the owner)

What works (verified live unless noted):
- **Tray**: icon while the app runs; Open, Quick Launch games, download status with pause/resume,
  Quit. Closing the window hides it to the tray (default on, like the official launcher);
  reopening takes ~31 ms. A second start shows the running window.
- **Run at startup** and **Start minimized**: the per-user Run entry is written when turned on and
  removed when turned off (verified, nothing left behind).
- **Notifications**: free games this week (shown live), download finished, sound on/off, quiet while
  playing.
- **Crash-safe updates**: an interrupted apply finishes on the next run; launching is refused until
  then (unit-tested, including a crash after the moves).
- **Accessibility**: 0 axe-core violations on 17 views, every clickable element ≥ 44 px, dialogs keep
  keyboard focus inside and give it back; `main` landmark and store card names fixed.
- **Open-source notices**: 326 components listed in Settings → About; full texts ship with the installer.
- 118 Rust tests.

Measured: idle 103.0 MB / 0.04 % CPU, installer 3.45 MiB, tray reopen 31 ms. Startup times this round
are not comparable (a game was running; see `docs/perf.md`).

Needs the owner:
1. **Code-signing certificate** for the installer (unsigned installers trigger SmartScreen).
2. **App updater**: where updates are hosted (e.g. GitHub Releases) and an update signing key pair.
3. **License** (ADR 0001).

## Phase 8 summary

What works (verified live with *while True: learn()*, whose saves live inside `test-installs/`):
- Cloud saves for every installed game whose catalog entry declares them (12 of the owner's games).
- Download restores a save written by the official launcher (4 files, SHA-1 checked); upload is
  compatible with Epic's storage and reads back byte-identically.
- Sync on start and on exit: before the game starts (nothing to do → no transfer), after it exits
  (changed saves uploaded 1.6 s later). Network trouble never blocks playing.
- Conflicts are never resolved silently with the default "Ask me": a dialog shows both times and offers
  "Use cloud save", "Keep this PC's save", or play without syncing. The first sync on a PC asks unless
  both sides match within a minute (it found the owner's 2024 cloud save and asked).
- Every download backs up the files it replaces (last 3 kept) and only touches the game's save files.
- Settings → Cloud saves lists the games with status ("In sync · synced 1 minute ago"), Sync now and a
  per-game switch.
- 117 Rust tests (30 api, 10 auth, 18 cache, 17 download, 3 import, 10 launch, 12 manifest, 17 app).

Measured: warm 418 ms, cold 429 ms, idle 105.4 MB / 0.03 % CPU, installer 3.32 MiB.

What changed on the owner's account: the test game's cloud saves gained three newer saves (the
same content as the 2024 save, then that save plus 18 s of play). Older cloud saves are untouched.

Known gaps:
- Not verified that the official launcher reads a save uploaded here (same format as Legendary's).
- Games whose saves live in AppData/Documents were not tested live (dev builds refuse folders outside
  the development root on purpose).
- No "clean up old cloud saves" action yet.

## Phase 7 summary

What works (verified live on the owner's account, read-only):
- **Account**: display name, email, country, communication language, 2FA status and connected
  accounts from Epic's API. Name and language can be changed in-app (Epic grants the launcher
  that permission); email, country, password, privacy, data download and deletion open the right
  Epic page, signed in. Rows for pages Epic doesn't have (active sessions, sign-in history) are gone.
- **Wallet & rewards**: Epic Rewards and coupon count from the API; "Not in your region" for the
  wallet balance (Epic reports the wallet as unsupported there). Redeem code is in-app.
  Add funds, gift cards, V-Bucks cards, transactions, payment settings and gifts open Epic's pages.
- **Profile menu**: Account, Gifts & coupons, Redeem code, Settings, Help, Legal, Sign out wired;
  online status says plainly that friends see you offline (no chat connection, ADR 0011).
- Every Epic page path was checked in the signed-in window; five Phase 0 guesses were wrong
  and are fixed (payments, messaging, rewards, code redemption, gifts).
- **Settings leftovers done**: hidden games (hide from the library menu, show again from
  Settings), install-folder picker (Windows' own dialog), connection / folder / disk checks,
  open logs folder, debug logging, proxy (applies on restart), real app version, "Open on".
- **Downloads**: auto-update after each library sync (not for folders shared with the official
  launcher, running games, or games switched off), "only when idle", verify after install.
- Fixed: a relative `--data-dir` put dev WebView2 profiles under `target\release`.
- 109 Rust tests (29 api, 10 auth, 18 cache, 17 download, 3 import, 10 launch, 9 manifest, 13 app).

Measured: warm 388 ms (under the 400 ms budget this run), cold 411 ms, idle 105.7 MB / 0.05 % CPU,
installer 3.26 MiB.

Not exercised live (they would change the account or need a code): changing the display name
or language, redeeming a code, auto-update (no update was available). Unit-tested instead.

Known gaps:
- Achievements list (no global Epic page; GraphQL to try in Phase 9).
- Tray, autostart, start minimized and notifications are Phase 9 (shown disabled).
- Online status / friends need Epic's chat connection (not planned yet).

## Phase 6 summary

What works (verified live):
- Store tab with chips: **Featured** (rows of Free now, On sale and New releases with "See all"),
  **Free games** (Epic's weekly giveaways with their dates), **On sale**, **New releases**, and
  "Show more" paging. Top-bar search searches the store while the Store is open.
- Product pages: price and discount, giveaway dates, tags, Epic's long description rendered as
  plain formatted text (no HTML from the catalog ever reaches the page).
- "In library" badges for games the account owns (verified with ABZÛ).
- **Buy now / Get** opens Epic's own checkout for that offer in the signed-in window; nothing is
  bought until the user confirms there. Wishlist and Cart open Epic's pages, signed in. When that
  window closes, the library syncs so purchases show up.
- Store data is cached in SQLite (10 min lists, 1 h details, free games until the next giveaway
  boundary); repeat visits cost no requests. A bot check would switch the view to Epic's own
  store page; none was seen.
- Fixed: closing the main window now quits even while an Epic web window is open.
- 103 Rust tests (25 api, 10 auth, 17 cache, 17 download, 3 import, 10 launch, 9 manifest, 12 app).

Measured: cold 410 ms, warm 401 ms (back at the 400 ms line), idle 105.9 MB / 0.03 % CPU (102.7 MB with the Store open), installer 3.21 MiB. Details in `docs/perf.md`.

Known gaps:
- No curated "Featured" feed from Epic is documented; Featured is an overview of the other rows.
- Genre/feature filters, editions and add-ons lists, screenshots and system requirements aren't
  shown yet (the product page links to Epic's page for those).
- Wishlist and cart are Epic's web pages; there's no in-app wishlist list.

## Phase 5 summary

What works (verified live with *while True: learn()*):
- Play from the library, Recently played and Quick Launch. Arguments built exactly as EGL's
  (unit-tested), with an exchange code fetched right before the start and never logged.
- Running state: the started process, anything it starts, and anything running from the
  game's folder, checked every 2 s only while a game runs. Tiles, Recently played and Quick
  Launch show "Starting…" and "Playing".
- Playtime: added locally on exit and reported to Epic (verified: Epic's total grew by the
  session). Failed reports wait in the database and go out after the next sign-in; closing the
  app mid-game keeps the session so far. "Recently played" now fills from real launches.
- Built and unit-tested, not exercised live: ownership tokens (`.ovt` written for the launch
  and deleted after), offline starts with a prompt, prerequisite installers asked once, UAC only
  when a game's own manifest requires it, EA app / Ubisoft Connect hand-off, and pausing
  downloads while playing (resumed when the last game exits).
- Fixed a Phase 4 leftover: Quick Launch's Install now opens the install dialog.
- 93 Rust tests (20 api, 10 auth, 15 cache, 17 download, 3 import, 10 launch, 9 manifest, 9 app).

Measured: cold 448 ms, warm 431–446 ms (window creation itself was ~30 ms slower than in
Phase 4's runs; see `docs/perf.md`), idle 105.9 MB / 0.03 % CPU, installer 3.14 MiB.

Known gaps:
- Per-game EULAs (`eulaIds`) aren't shown before first launch yet.
- A game already running when Next Launcher starts isn't detected; EGL's `ProcessNames`
  for adopted installs aren't used.
- Live tests of ownership tokens, offline start, third-party hand-off and pause-while-playing
  need an installed title that uses them or a second small install.

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
- Prerequisite installers (done in Phase 5) and the manifest's uninstall action (still open).
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
- "Recently played" stays empty until launches are tracked (done in Phase 5): Epic only reports totals.
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
3. **Warm start**: ~390–450 ms on an idle machine (WebView2 window creation alone is ~300–350 ms).
   Phase 9 adds close-to-tray (default on) with a ~31 ms reopen. Recommendation: accept.
4. **Mockup access**: sign in to claude.ai in the app's browser pane, or share screenshots.
5. **Signing certificate and update hosting** (Phase 9, see above).

## Conventions

- Test installs go to `C:\Users\hezy\Documents\Claude\Next\test-installs\` (git-ignored).
- Real installs (e.g. `F:\Fortnite`) and `C:\ProgramData\Epic\...` are read-only unless the owner confirms in chat.
- Debug builds keep data in `.dev-data/`. Release builds use `%LOCALAPPDATA%\NextLauncher`.
