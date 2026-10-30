# ADR 0015: Import the official launcher's games automatically

Status: Accepted (2026-10-30), owner's decision ("all installed games should be imported").
Changes step 4 of the import algorithm in `docs/protocol/egl-local-files.md` (it asked per game).

- **After every library sync** Next Launcher adopts each install the Epic Games Launcher (EGL)
  knows about that is: a real game (not a content pack), complete (`bIsIncompleteInstall` false),
  with its folder, its `.egstore` build manifest and its `LaunchExecutable` present, in this
  account's library, and not installed here yet. Content packs stay with their main game.
  Installs that need validation are adopted and checked before their first launch, as before.
- **Read only.** Adopting copies the build manifest into our data folder and records the path.
  Tested on the owner's PC with Fortnite on `F:\Fortnite`: every file in that folder and in EGL's
  data folders had the same size and time before and after (733 entries).
- **Writes stay manual.** Games shared with EGL are never updated automatically, and uninstall or
  move still asks a second time, naming the folder (ADR 0009).
- **Uninstalled here means gone.** Our uninstall deletes the game's files but leaves EGL's
  records and `.egstore`, so the EGL `InstallationGuid` is remembered (`internal.import.removed`)
  and that install is never adopted again automatically. A reinstall by EGL has a new id and is
  adopted again. The executable check is a second guard.
- **Setting**: Settings → Downloads & installs → "Add games from the Epic Games Launcher", on by
  default. The Import dialog stays for checking the list by hand.
- **Not owned stays out.** Games EGL installed for another account are never adopted (no
  ownership bypass).
- **Development builds** (install guard on) skip the one-running-copy rule, so they run next to an
  installed Next Launcher with their own data folder instead of handing over to it.
