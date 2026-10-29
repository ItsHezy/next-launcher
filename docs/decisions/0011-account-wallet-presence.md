# ADR 0011: Account, wallet, presence

Status: Accepted (2026-10-29), verified on the owner's account.

- **Capabilities, not guesses.** At every sign-in the app reads the token's permissions
  (`verify?includePerms=true`) and derives exact capabilities (resource + action bit). A row uses
  the API only when Epic grants it; otherwise it opens Epic's own page, signed in through the
  exchange-code bridge. Today that means in-app: display name, communication language, wallet
  summary, code redemption, connected-accounts list. Web: everything else.
- **No account change without the user's click.** Name and language changes go through a
  dialog the user submits; codes are typed by the user and sent once. We never changed the
  owner's name, language or sessions while testing; those paths are unit-tested.
- **"Sign out of all devices" is a web action.** The launcher token can only end sessions of
  the same source, which isn't what the row promises. Epic's Password & security page has the
  real control.
- **Presence: not connected.** Online status needs Epic's XMPP chat connection (a persistent
  socket, plus friends and chat to be useful). Not built, so friends see the account offline
  while Next Launcher is in use. The profile menu says exactly that ("Shown offline to
  friends") instead of offering a selector that does nothing.
- **Achievements: later.** No global achievements page exists on Epic's site (404); they're per
  game on product pages and in a GraphQL API. A list view lands with Phase 9 polish if the API
  works with the launcher token.
- **Settings that belong to Phase 9** (run at startup, start minimized, tray, notifications)
  are shown disabled with a note, rather than stored without effect.
- **Proxy** applies to Next Launcher's own requests on the next start (reqwest `Proxy::all`);
  Epic's web windows keep using Windows' proxy settings. **Debug logging** is a marker file read
  before logging starts (applies on restart).
- **Folder picker**: Windows' own `SHBrowseForFolderW` through windows-sys instead of the dialog
  plugin, which avoids a new dependency for one dialog.
- **Dev data folder**: `--data-dir` is made absolute; WebView2 resolved a relative profile folder
  from the exe's folder, which put dev web profiles under `target\release`.
