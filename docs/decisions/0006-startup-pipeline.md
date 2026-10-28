# ADR 0006: Startup pipeline

Status: Accepted (2026-10-28)

1. `main()` records T0, then reads the SQLite snapshot on a side thread while Tauri initialises.
2. `setup()` joins that thread (already done, ~10 ms) and builds the main window **hidden**, with a
   native background of `#26282A`, a dedicated WebView2 data directory, and an initialization
   script that sets `window.__NL_BOOT__` (snapshot + settings). No IPC is needed for first paint.
3. The UI loads Inter through the `FontFace` API, mounts, forces the first layout, and calls
   `app_ready` with its performance marks. Rust shows the window and records the timeline.
4. A 3 s safety timer shows the window even if the UI never reports.
5. Nothing network-bound runs before `window_shown`. (Phase 2+: token refresh, sync, image fetch
   and update checks start after it.)

Why not wait for a painted frame: a hidden WebView2 window throttles `requestAnimationFrame` and
timers, so "painted" can't be observed while hidden. Forcing layout first means the compositor
only has to paint once the window is shown. Measurements are in `docs/perf.md`.

Rejected: a splash screen (the brief forbids it); showing an empty window early (visible blank
frame for ~100 ms).
