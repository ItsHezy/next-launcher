# ADR 0009: Downloads and installs

Status: Accepted (2026-10-29), verified with a live install, verify, repair, move and resume of
a small free game into the git-ignored `test-installs/` folder.

- **Engine without I/O dependencies**: `egs-download` gets chunk bytes from a caller-supplied
  fetcher and has no HTTP or Tauri code, so planning, writing, resume, staging, verify and move
  are all tested offline with fixture manifests. `src-tauri/src/install.rs` adds Epic's
  endpoints, CDN rotation, URL signing, progress events and the SQLite queue.
- **Atomic files**: every file is written to `<file>.nl-tmp`, checked against the manifest SHA-1,
  then renamed into place. A file is recorded as done only after the rename, so resume after a
  crash re-downloads at most the files in flight.
- **Staged updates and repairs**: new and changed files go to `<install>/.nl-staging/` and replace
  the old ones only when all of them are written and checked. Files removed by the update are
  deleted last. A cancelled or failed update leaves the old build working.
- **Memory**: an in-memory chunk cache with a 256 MiB default budget (configurable), reserved in
  fetch order so it cannot deadlock. The planner simulates the job to report the peak (16 MB for
  the test game). No chunks are cached on disk across restarts.
- **Speed limit**: a small token bucket of our own instead of `governor` (one less dependency;
  the limit changes live from Settings).
- **Uninstall deletes the manifest's files only**; anything the game wrote itself stays.
  **Move takes the whole folder** (the user moves the game, including its configs). Same drive:
  rename. Another drive: copy, check sizes, flush, then delete the old folder; until that delete
  the old folder is never modified. Details in `docs/protocol/downloads-cdn.md` §7.
- **Content keys** of encrypted builds are kept as `<build>.secrets.json` next to our manifest
  copy in `%LOCALAPPDATA%\NextLauncher\manifests\` (per-user folder), not in Credential Manager:
  they are per-build content keys Epic gives to entitled accounts, not account credentials, and
  verify, uninstall and updates need them offline. Never logged; removed with the manifest copy.
- **Folders shared with the official launcher** (adopted in Phase 3): uninstall and move need a
  second, explicit confirmation naming the folder. After a move the install is ours alone; we
  never edit EGL's own files.
- **Development guard**: `--dev-install-root <dir>` makes every install, update, repair, move and
  uninstall refuse paths outside `<dir>`. All live testing during development runs with it,
  pointed at `test-installs/`, so a bug can't touch a real game folder.
- **Dev tooling**: `--dev-action <install|verify|repair|uninstall>:<app>` and
  `--dev-action move:<app>=<folder>` run the same code paths as the buttons after the first sync.

Not built yet: `uninstall_action_path` (prerequisite installers came with Phase 5), SSD-aware verify
parallelism, cross-drive move tested live (unit-tested only; the only other drive holds the
owner's real games).
