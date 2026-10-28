# Performance

Every number here is measured, with the command that produced it. Updated at the end of each phase.

## Test machine

AMD Ryzen 5 5600X (6C/12T), 32 GB RAM, NVMe SSD, RTX 3060 Ti, 200 Hz display, Windows 11 Home
10.0.26200, WebView2 runtime 156.0.4314.50. That's upper mid-range; numbers on a true mid-range PC
will be somewhat higher.

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
