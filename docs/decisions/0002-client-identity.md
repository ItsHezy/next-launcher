# ADR 0002: Client identity and webview profiles

Status: Accepted (2026-10-28)

## Decision

1. All OAuth calls use EGL's own public client `launcherAppClient2`
   (`34a02cf8f4414e29b15921876da36f9a`). That's the only client Epic grants launcher permissions
   (library, downloads, ownership tokens), and the one Legendary/Heroic use. No other Epic client id is ever used.
2. API requests send `UELauncher/<version> Windows/<build>.1.256.64bit`. `<version>` is read from
   the local EGL install's `Launcher.manifest.meta` if present (observed `20.3.6-59512232...`), else
   a bundled default updated with each release. Web views use WebView2's normal UA.
3. Three isolated WebView2 data directories:
   - `ui/`: the app UI. Loads only local assets. No remote navigation allowed (navigation handler rejects all non-app URLs).
   - `login/`: Epic's login page only. The init script that reads `/id/api/redirect` lives here.
   - `web/`: signed-in Epic pages (account, wallet, store checkout, Fab/UE). Navigation limited to the
     SSO domains plus the payment providers Epic's checkout redirects to (list built in Phase 6).
   The UI webview never holds Epic cookies, and remote pages never get Tauri IPC.

## Why

- Using the launcher client is what makes an interoperability client possible. Inventing or
  borrowing other product clients (e.g. Fortnite's) would misrepresent us to Epic and isn't needed.
- Isolated profiles keep session cookies out of the UI process and limit damage if a remote page misbehaves.
