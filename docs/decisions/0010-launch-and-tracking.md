# ADR 0010: Launching games and tracking play

Status: Accepted (2026-10-29).

- **Exactly the manifest's program, exactly EGL's arguments.** `egs-launch::build` is a pure
  function from the manifest, catalog attributes, account and settings to the executable,
  arguments and working folder, in the documented order (launch.md). Unit-tested; the
  exchange code's `Debug` output is redacted and arguments are never logged.
- **Start**: `CreateProcessW` through `std::process::Command`. A program whose manifest asks for
  administrator rights fails with `ERROR_ELEVATION_REQUIRED`; only then we use
  `ShellExecuteExW("runas")`, so Windows shows its own UAC prompt. Nothing is elevated silently.
- **Tracking without touching the game**: every 2 s, only while a game runs, one Toolhelp32
  process snapshot. A game counts as running while the started process, anything it started
  (parent ids), or any process whose image lives in the install folder is alive. Only
  processes whose file name matches an executable in the manifest are opened, with
  `PROCESS_QUERY_LIMITED_INFORMATION`, to read their path. No hooks, no job objects, no
  injection, so anti-cheat launchers and bootstrappers behave as with EGL.
- **Playtime**: added locally when the game exits and reported to Epic with a stable random
  machine id. A failed report is kept in `events_log` and sent after the next sign-in, before
  the library sync. Closing the app while a game runs records the session so far.
- **Ownership tokens** are fetched only for titles whose catalog says so, written to
  `<data>\tmp\<ns><item>.ovt`, and deleted when the game exits. Never cached across launches.
- **Offline**: without a session (or when Epic can't be reached) the user is asked before an
  offline start; titles that declare `CanRunOffline: false` get a clearer warning.
- **Prerequisites** shipped with a game are offered once per prerequisite id set ("Install and
  play" / "Play without it"); the installer runs through the shell so its own UAC prompt shows.
- **Downloads while playing** (setting on by default): the running job is paused and the queue
  held; when the last game exits, only the jobs we paused are resumed.
- **Third-party titles** (EA app, Ubisoft Connect) are handed to that launcher with an exchange
  code via its URL scheme; we never start their executables ourselves.
