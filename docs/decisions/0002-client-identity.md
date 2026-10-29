# ADR 0002: Client identity and webview profiles

Status: Accepted (2026-10-28)

## Decision

1. All OAuth calls use EGL's own public client `launcherAppClient2`
   (`34a02cf8f4414e29b15921876da36f9a`). That's the only client Epic grants launcher permissions
   (library, downloads, ownership tokens), and the one Legendary/Heroic use. No other Epic client id is ever used.
2. API requests send `UELauncher/<version> Windows/<build>.1.256.64bit`. `<version>` is read from
   the local EGL install's `Launcher.manifest.meta` if present (observed `20.3.6-59512232...`), else
   a bundled default updated with each release. Web views use WebView2's normal UA.
3. Two isolated WebView2 data directories (revised in Phase 2, see below):
   - `ui/`: the app UI. Loads only local assets.
   - `web/`: Epic's login page **and** signed-in Epic pages (account, wallet, checkout, Fab/UE).
     Navigation is limited to `https`. The init script that reads `/id/api/redirect` runs only in
     the login window and only acts on that one path.
   The UI webview never holds Epic cookies, and remote pages never get Tauri IPC.

## Revision (Phase 2, 2026-10-28)

Login and Epic pages originally had separate profiles. They now share `web/`: the session
cookie from signing in also serves account pages, and when it expires the exchange-code bridge
(auth.md §6, verified live) restores it. Sign-out deletes the whole `web/` profile. Both
windows only ever load Epic's own sites, so separating them bought no real isolation.

## Why

- Using the launcher client is what makes an interoperability client possible. Inventing or
  borrowing other product clients (e.g. Fortnite's) would misrepresent us to Epic and isn't needed.
- Isolated profiles keep session cookies out of the UI process and limit damage if a remote page misbehaves.
