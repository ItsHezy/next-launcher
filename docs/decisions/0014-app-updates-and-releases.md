# ADR 0014: App updates and releases

Status: Accepted (2026-10-30), owner's decision: releases on **GitHub Releases**
(`ItsHezy/next-launcher`, public). Update public key set (minisign key ID `FBF6DA559F103AFF`).
Open: the signing secrets in the repository, and (optionally) a code-signing certificate.

- **Updater: `tauri-plugin-updater`** (+287 KB exe, +~115 KB installer; below the 500 KB line). It
  reads `latest.json` from the newest published release
  (`github.com/ItsHezy/next-launcher/releases/latest/download/latest.json`, to change if the
  repository gets another name), downloads the installer and checks its minisign signature against
  the public key built into the app before running anything. A download that doesn't match the
  key is refused (tested: one changed byte, a different key).
- **The user decides.** A background check runs a minute after start, at most once a day
  (Settings → General "Check for app updates", on by default); it shows a notification and marks
  Settings → About. Installing only happens from About → Check → "Install and restart", never
  while a game runs. The NSIS installer runs in passive mode and starts the app again; playtime
  is saved first and downloads resume per file afterwards (Phase 4).
- **No key, no updates.** Without a real public key in `tauri.conf.json` the app says "Updates
  aren't set up for this build yet" and never contacts the update address. Before the first
  release is published the address answers 404, which counts as "no newer version".
- **Development override**: with the install guard on (`--dev-install-root`), `NL_UPDATE_URL` and
  `NL_UPDATE_PUBKEY` point the updater at a local test release; `--dev-action
  app-update:<status|check|download>` checks and downloads with the signature verified, never
  installs. Release builds refuse plain `http` addresses (plugin rule).
- **Releases are built by GitHub Actions** (`.github/workflows/release.yml`) from a `v<version>`
  tag: the CI checks first, then a preflight (tag = app version, real public key, update address =
  this repository), the build with the update signature (`src-tauri/tauri.release.conf.json`),
  `tools/release-manifest.mjs` for `latest.json`, and a **draft** release with the installer,
  `latest.json`, `LICENSE.txt` and `THIRD-PARTY-NOTICES.txt`. Users get it only when the owner
  publishes the draft. GitHub attaches the tag's source code (GPL-3.0 §6). Release notes come from
  `docs/releases/<tag>.md` when present.
- **The private update key never enters the repository**: the owner generates it
  (`npm run tauri signer generate -- -w <path outside the repo>`), keeps a backup, and stores it
  and its password as repository secrets. Losing it means existing installs can't verify later
  updates (they'd need a manual reinstall).
- **Authenticode signing** runs only if the `WINDOWS_CERTIFICATE` secrets exist; without a
  certificate, SmartScreen warns on first download.
