# Performance

Every number here is measured, with the command that produced it. Updated at the end of each phase.

## Test machine

AMD Ryzen 5 5600X (6C/12T), 32 GB RAM, NVMe SSD, RTX 3060 Ti, 200 Hz display, Windows 11 Home
10.0.26200, WebView2 runtime 156.0.4314.50. That's upper mid-range; numbers on a true mid-range PC
will be somewhat higher.

## Animations (2026-11-01, ADR 0018)

Release build, `--bench-scroll --mock-games 600`, Settings › General › Animations **On** (worst
case: cover art fades as tiles mount while scrolling).

| Metric | Budget | Measured |
|---|---|---|
| Library scroll, 600 games | 60 fps | **198.4 fps** mean (5.04 ms/frame, 200 Hz), p99 5.1 ms, 2/359 frames > 1.5× vsync ✅ (unchanged) |
| Tab switch (DOM shown; the fade-up plays after it) | ≤ 50 ms | **7.7–12.9 ms** ✅ |
| Initial / total JS (gz) | 80 / 250 KB | **42.6 / 77.9 KB** ✅ (+3.4 / +3.9 KB: Svelte transitions, flip, `motion.ts`) |
| Accessibility (axe-core, 17 views) | 0 serious | **0 violations** ✅ |

## Phase 9 (2026-10-30): polish

| Metric | Budget | Measured |
|---|---|---|
| Reopen from the tray (window hidden → visible), 5 runs | | **31 ms** (76 ms first) |
| Idle RAM / CPU, signed in, tray icon present | ≤ 150 MB / ~0 % | **103.0 MB / 0.04 %** ✅ |
| Installer (NSIS, incl. 830 KB of license texts) | ≤ 15 MB | **3.45 MiB** ✅ (exe 9.01 MiB) |
| Initial / total JS (gz) | 80 / 250 KB | 38.5 / 70.3 KB ✅ (licenses list lazy) |
| Notification plugin cost | ≤ 500 KB per dependency | +207 KB exe, +62 KB installer |
| Updater plugin cost | ≤ 500 KB per dependency | +287 KB exe, +115 KB installer |
| Installer / exe with the updater (final Phase 9 build) | ≤ 15 MB | **3.57 MiB** ✅ / 9.33 MiB |
| Accessibility (axe-core, 17 views, + 44 px hit targets) | 0 serious | **0 violations** ✅ |
| Warm / cold start, median of 9 | ≤ 400 ms / ≤ 1.0 s | 587 / 651 ms **not comparable**: a game (≈ 4 cores busy) was running. The Phase 8 exe measured at the same time: 608 ms warm. To re-measure on an idle machine. |

## Phase 8 (2026-10-29): cloud saves

| Metric | Budget | Measured |
|---|---|---|
| Warm start, standard bench, median of 7 | ≤ 400 ms | **418 ms** (min 390): run-to-run spread since Phase 5 is ±30 ms |
| Cold start, median of 7 | ≤ 1.0 s | **429 ms** ✅ |
| Idle RAM / CPU, signed in | ≤ 150 MB / ~0 % | **105.4 MB / 0.03 %** ✅ |
| Installer (NSIS) | ≤ 15 MB | **3.32 MiB** ✅ (exe 8.70 MiB) |
| Initial / total JS (gz) | 80 / 250 KB | 38.4 / 65.5 KB ✅ |
| Upload after exit (4 files, 1 chunk) | | 1.6 s after the game closed |

## Phase 7 (2026-10-29): settings, wallet, profile menu

| Metric | Budget | Measured |
|---|---|---|
| Warm start, standard bench, median of 7 | ≤ 400 ms | **388 ms** (min 375) ✅ |
| Cold start, standard bench, median of 7 | ≤ 1.0 s | **411 ms** ✅ (one outlier run at 1.4 s) |
| Idle RAM, signed in | ≤ 150 MB | **105.7 MB** ✅ |
| Idle CPU | ~0 % | **0.05 %** ✅ |
| Installer (NSIS) | ≤ 15 MB | **3.26 MiB** ✅ (exe 8.53 MiB) |
| Initial / total JS (gz) | 80 / 250 KB | 38.0 / 63.2 KB ✅ (account dialog lazy) |
| Connection check (token verify) | | 120 ms |

Wallet and account details load after sign-in in the background (wallet cached 5 min,
details 2 min); nothing new runs before first paint.

## Phase 6 (2026-10-29): store

| Metric | Budget | Measured |
|---|---|---|
| Warm start, standard bench, median of 7 | ≤ 400 ms | **401 ms** (min 387) ≈ at the line |
| Cold start, standard bench, median of 7 | ≤ 1.0 s | **410 ms** ✅ |
| Idle RAM, signed in, Store not opened | ≤ 150 MB | **105.9 MB** ✅ |
| Idle CPU | ~0 % | **0.03 %** ✅ |
| Installer (NSIS) | ≤ 15 MB | **3.21 MiB** ✅ (exe 8.35 MiB) |
| Initial / total JS (gz) | 80 / 250 KB | 36.6 / 58.8 KB ✅ (Store route lazy; store state in the initial chunk for the top-bar search) |
| Store Featured, cold (free feed + 2 searches in parallel) | | 344 ms |
| Store search (network) / product details (network) | | 215–314 ms / 188 ms |
| Same views again (SQLite cache) | | 0 ms network, no requests |
| Idle RAM / CPU with the Store open (Featured, 17 cards) | ≤ 150 MB | **102.7 MB / 0.05 %** ✅ |

Window creation was back to 317 ms median (Phase 5's runs: 348 ms with identical code before
that mark), which confirms Phase 5's slower warm start was the machine, not the code.

## Phase 5 (2026-10-29): launch

| Metric | Budget | Measured |
|---|---|---|
| Warm start, standard bench, median | ≤ 400 ms | **431 ms** (7 runs), **446 ms** (9 runs) ❌ see below |
| Cold start, standard bench, median | ≤ 1.0 s | **447–448 ms** ✅ |
| Idle RAM, signed in, nothing running | ≤ 150 MB | **105.9 MB** ✅ |
| Idle CPU | ~0 % | **0.03 %** ✅ |
| Installer (NSIS) | ≤ 15 MB | **3.14 MiB** ✅ (exe 8.12 MiB) |
| Initial / total JS (gz) | 80 / 250 KB | 36.3 / 55.0 KB ✅ |
| Launch request → game process started (online: exchange code, build info, manifest, spawn) | | 300–330 ms (3 runs) |
| Game closed → "Playing" cleared, playtime saved and reported | | ≤ 2 s poll + ~0.1 s PUT |

Warm start: window creation alone (`window_built`, before any of our UI code runs) moved from
317 to 348 ms median between these runs and Phase 4's, with identical Rust code before that
mark; the UI part after it moved 89 → 98 ms (+1.2 KB initial JS for the shared play action).
Treat the warm number as at the line with ±30 ms run-to-run noise; the decision from
Phase 1 (accept, or tray reopen in Phase 9) still stands.

## Phase 4 (2026-10-29): downloads and installs

Test game *while True: learn()* (feature level 13, 153 files, 153 MB download, 887 MB installed)
into `test-installs/`, home connection, release build.

| Metric | Budget | Measured |
|---|---|---|
| Warm start, standard bench (48 mock games), median of 7 | ≤ 400 ms | **406 ms** (min 403) ≈ at the line, unchanged |
| Cold start, standard bench, median of 7 | ≤ 1.0 s | **416 ms** ✅ |
| Idle RAM, signed in, one game installed | ≤ 150 MB | **108.6 MB** ✅ |
| Idle CPU | ~0 % | **0.01 %** ✅ |
| Installer (NSIS) | ≤ 15 MB | **3.10 MiB** ✅ (exe 8.0 MiB) |
| Initial / total JS (gz) | 80 / 250 KB | 35.1 / 53.1 KB ✅ (install, uninstall, move dialogs lazy) |
| Full install: 816 chunk fetches | | 21 s (~7 MB/s), peak chunk cache 16 MB |
| Resume after a forced kill 7 s in (141 of 153 files done) | | 12 files / 106 MB left, 12 s; partial `.nl-tmp` discarded |
| Verify, 153 files / 887 MB | | 334–371 ms (warm file cache) |
| Repair of one damaged file | | 5 chunks, ~1 MB, < 1 s, staged |
| Move, same drive | | 2 ms (one rename) |
| Manifest parse, Fortnite (151,991 chunks, 891 files) | | 38 ms |

Not measured yet: RAM while downloading, and throughput on a link faster than this one (the
engine ran 16 workers; the connection's own speed wasn't measured separately).

## Phase 3 (2026-10-29): library sync

| Metric | Budget | Measured |
|---|---|---|
| Warm start, owner's real profile (35 games, real covers), 6 runs | ≤ 400 ms | **364–392 ms** (median ~372) ✅ |
| Warm start, standard bench (48 mock games), median of 7 | ≤ 400 ms | **409 ms** (min 390) ≈ at the line |
| Cold start, standard bench, median of 7 | ≤ 1.0 s | **431 ms** ✅ |
| Idle RAM, signed in, 35 covers, after a sync | ≤ 150 MB | **108.9 MB** ✅ |
| Idle CPU | ~0 % | **0.01 %** ✅ |
| First full sync (62 records, 38 catalog items) | | 2.06 s, in background |
| Re-sync with valid state token | | 0.74 s, no catalog or image traffic |
| Cover art (76 images, resized by CDN, 8 in flight) | | ~13.6 s, in background, 0 failures |
| Initial / total JS (gz) | 80 / 250 KB | 33.8 / 46.5 KB ✅ |

Today's runs are faster across the board than Phase 2's (WebView2 creation 326 vs 359 ms
median), so day-to-day variance is about ±40 ms. Warm start now sits right at the target:
met on the real profile, 9 ms over on the synthetic bench.

## Phase 2 (2026-10-28): auth

Network starts only after the window is shown, so startup is unchanged within run-to-run noise.

| Metric | Budget | Phase 1 | Phase 2 |
|---|---|---|---|
| Cold start (fresh profile, median of 7) | ≤ 1.0 s | 474 ms | **469 ms** ✅ |
| Warm start (median of 7) | ≤ 400 ms | 458 ms | **485 ms** ❌ (same gap; noise ±30 ms) |
| Idle RAM, signed in, refresh timer armed | ≤ 150 MB | 92–96 MB (no account) | **100.1 MB** ✅ |
| Idle CPU, signed in | ~0 % | 0.01–0.03 % | **0.01 %** ✅ |
| Release exe | | 5.56 MiB | 7.20 MiB (+reqwest, rustls/ring, keyring) |
| Installer (NSIS) | ≤ 15 MB | 2.07 MiB | **2.84 MiB** ✅ |
| Session restore after launch (refresh + verified call) | | | ~1.2 s, in background |

## Phase 1 (2026-10-28): skeleton on mock data

Release build (`npx tauri build`), 48 mock games unless noted.

| Metric | Budget | Measured | Status |
|---|---|---|---|
| Cold start → library visible (fresh WebView2 profile) | ≤ 1.0 s | **474 ms** median (457–610, 9 runs) | ✅ |
| Warm start (existing profile, process not running) | ≤ 400 ms | **458 ms** median (438–499, 9 runs) | ❌ 58 ms over (see below) |
| Tab switch, already visited | ≤ 50 ms | **3–7 ms** | ✅ |
| Tab switch, first visit | ≤ 300 ms to skeleton | **10–11 ms** (chunk already prefetched at idle) | ✅ |
| Idle RAM, app + WebView2 (Task Manager "Memory" = private working set) | ≤ 150 MB | **92–96 MB** (7 processes); commit 177–187 MB | ✅ |
| Idle CPU | ~0 % | **0.01–0.03 %** of all cores over 10 s | ✅ |
| Installer (NSIS, unsigned) | ≤ 15 MB | **2.07 MiB** | ✅ |
| Library scroll, 600 games | 60 fps | **198 fps** mean (5.06 ms/frame on a 200 Hz display), p99 5.1 ms, 2/359 frames > 1.5× vsync, 25 tiles in the DOM | ✅ |
| Initial-route JS (gzip) | ≤ 80 KB | **29.7 KB** | ✅ |
| Total JS (gzip) | ≤ 250 KB | **39.2 KB** (6 chunks) | ✅ |

"Visible" = the moment Rust shows the window, after the UI has mounted the library DOM and done
its first layout. The compositor presents that DOM on the next display frame (≤ 5 ms here).

### Where startup time goes (warm run, ms since `main`)

```
    0.0  main
    9.5  cache_open            SQLite open + migrations check (+ mock seed on first run)
   10.1  snapshot_read         48-game snapshot (600 games: same, ~1-2 ms)
   10.1  tauri_setup           boot data already prepared on a side thread
  ~355   window_built          ← WebView2 environment + controller creation (~345 ms, outside our code)
  ~395   ui:script             index.html + 82 KB JS fetched and parsed
  ~405   ui:font               Inter loaded via FontFace before mount
  ~415   ui:mounted            Svelte mounts Library from the injected boot data (no IPC)
  ~440   ui:laid_out           first style + layout, still hidden
  ~450   window_shown
```

Findings that shaped the pipeline:

1. **A hidden WebView2 window throttles `requestAnimationFrame` and timers.** Waiting for a
   painted frame before showing the window doesn't work (rAF never fires). Instead we force the
   first layout synchronously, then show. The window is never shown blank (background is `#26282A`
   natively and in CSS, so there's no white flash).
2. **Font before layout.** Laying out before Inter was loaded made Chromium resolve system
   fallback fonts first (first layout 57 ms → 25 ms once the font is loaded beforehand), and the
   font response queued behind that work (+25 ms). Loading the font via the `FontFace` API before
   `mount` cut script→visible from ~120 ms to ~50 ms.
3. **No IPC on the critical path.** Rust reads the snapshot on a side thread while Tauri
   initialises and injects it as `window.__NL_BOOT__` via an initialization script.

### Warm start: the gap

About 345 ms of the 458 ms is WebView2 creating its browser/renderer/GPU processes. That happens
inside `WebviewWindowBuilder::build()` and isn't reducible from our code as built today.
Everything we control (Rust boot + page + mount + layout) is ~110 ms. Options, to decide with the
owner:

- **Tray-resident warm start (Phase 9).** With "minimize to tray on close" (default on), reopening
  shows an existing window: no process start, no WebView2 creation. Expected ≪ 100 ms. This is
  probably what most users experience as "warm start".
- **WebView2 browser arguments** (e.g. fewer helper processes, `--disable-gpu`) could cut creation
  time, but may cost scroll smoothness. To be measured with the scroll bench before adopting.
- **Shave the remaining ~110 ms** (smaller entry chunk, less first-layout work): worth maybe
  20–40 ms. Not enough alone to reach 400.

## Contrast (section 6 requires ≥ 4.5:1 for text)

Measured with WCAG relative luminance. The resulting rules are documented at the top of `ui/src/styles/app.css`.

| Foreground | on `--bg` #26282A | on `--surface` #2E3033 | on selected row |
|---|---|---|---|
| `--text` #F2F2F5 | 13.24 | 11.84 | 7.13 |
| `--text-2` #A7A7B2 | 6.21 | 5.55 | 3.34 ✗ |
| `--text-3` #8E8E99 | 4.56 | **4.08 ✗** | 2.46 ✗ |
| `--success` #7EE2A0 | 9.35 | 8.37 | 5.03 |
| `--warning` #FFD27A | 10.39 | 9.30 | 5.59 |
| `--danger` #FF8A80 | 6.48 | 5.80 | 3.49 ✗ |
| raw accent as text | — | 2.19–3.01 ✗ | — |
| white on raw accent | 4.40 (green) – 6.04 | | |

Rules applied (tokens unchanged): `--text-3` only on `--bg`/`--surface-inset`; text on a selected
row is `--text`; accent-coloured text uses `--accent-text` = accent mixed 45 % with white
(≥ 5.28:1 on surfaces); filled accent controls use `--accent-fill` = accent mixed 12 % with black
(white text ≥ 5.46:1 for all four accents).

## How to reproduce

```
npx tauri build                                    # release exe + NSIS installer
npm run budget                                     # bundle budget (CI runs this)
node tools/bench-startup.mjs 9                     # cold/warm startup, 9 runs each
powershell -ExecutionPolicy Bypass -File tools/measure-idle.ps1                       # idle RAM/CPU
powershell -ExecutionPolicy Bypass -File tools/measure-idle.ps1 -DataDir .dev-data/bench600 -Route settings:general
target/release/next-launcher.exe --bench-scroll --mock-games 600 --data-dir .dev-data/bench600
                                                   # → .dev-data/bench600/logs/bench-scroll.json
target/release/next-launcher.exe --bench-startup   # prints the timeline (also logs/startup.json)
```
