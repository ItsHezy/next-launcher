# Next Launcher: progress

## Phase checklist

- [x] **Phase 0: Research** (2026-10-28). Specs in `docs/protocol/`, ledger in `research/notes.md`.
- [x] **Phase 1: Skeleton and speed** (2026-10-28). Full UI on mock data, measured in `docs/perf.md`.
- [x] **Phase 2: Auth** (2026-10-28). Live sign-in, silent restore and signed-in Epic pages verified on the owner's account.
- [ ] Phase 3: Library
- [ ] Phase 4: Downloads and installs
- [ ] Phase 5: Launch
- [ ] Phase 6: Store
- [ ] Phase 7: Settings, wallet, profile menu
- [ ] Phase 8: Cloud saves
- [ ] Phase 9: Polish and ship

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
3. **Warm start**: accept ~460 ms for a fresh process plus near-instant reopen from tray (Phase 9), or
   spend time on WebView2 flags (risk to scroll smoothness).
4. **Test game** for Phase 4: a small free game on the owner's account (Fortnite is unsuitable).
5. **Mockup access**: sign in to claude.ai in the app's browser pane, or share screenshots.

## Conventions

- Test installs go to `C:\Users\hezy\Documents\Claude\Next\test-installs\` (git-ignored).
- Real installs (e.g. `F:\Fortnite`) and `C:\ProgramData\Epic\...` are read-only unless the owner confirms in chat.
- Debug builds keep data in `.dev-data/`. Release builds use `%LOCALAPPDATA%\NextLauncher`.
