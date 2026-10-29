# ADR 0012: Cloud saves

Status: Accepted (2026-10-29), verified with one game on the owner's account.

- **Format compatible with EGL and Legendary**: feature level 18 manifests, files sorted
  case-insensitively, 1 MiB chunks zero-padded before hashing, zlib, rolling hash + SHA-1, chunks
  uploaded before the manifest. Downloading a save EGL wrote works; our uploads read back identically.
- **Decision by record, not clocks alone**: we remember the manifest and local time of the last
  agreement per game, so "changed elsewhere" and "changed here" are told apart even with clock skew.
  The first sync on a PC never guesses: unless both sides are within a minute it asks.
- **Never lose a save**: a download copies the files it replaces to `save-backups/<app>/<time>/`
  (last 3 kept), checks every file's SHA-1 before writing, writes atomically, and only touches files
  matching the game's include/exclude lists. Uploads never delete older cloud saves.
- **Conflicts follow the setting**: ask (default) → a dialog with both times; "prefer this PC" /
  "prefer cloud" resolve automatically. At launch the question comes before the game starts
  ("Use cloud save", "Keep this PC's save", "Play without syncing"); after exit it appears when the
  app is next in front.
- **Never blocks playing**: if Epic can't be reached, the launch goes ahead and the next sync catches up.
- **Development guard** covers save folders: dev builds refuse to read or write saves outside the
  development root.
- **Cleanup of old cloud saves** stays manual and is not built (cloud-saves.md "Cleanup").
