# The official launcher's local files (import, read-only)

Crate: `egs-import`. **Read-only by default.** Nothing here is written, moved or deleted without
an explicit user confirmation naming the folder. That includes EGL's `C:\ProgramData\Epic\...`
files and the game folders themselves.

Everything on this page was **observed on the developer machine on 2026-10-28** [OBS]
(EGL `20.3.6-59512232`, two installs: Fortnite and its LEGO Fortnite content in `F:\Fortnite`)
unless marked [REF].

## Locations

| What | Path |
|---|---|
| Installed list | `C:\ProgramData\Epic\UnrealEngineLauncher\LauncherInstalled.dat` (JSON despite the name) |
| Per-install manifests | `C:\ProgramData\Epic\EpicGamesLauncher\Data\Manifests\<InstallationGuid>.item` (JSON) |
| In-progress installs | `...\Data\Manifests\Pending\` |
| Selective-download metadata | `...\Data\SDMeta\<sha256>.sdmeta` (JSON, UTF-8 or UTF-16LE+BOM) |
| EGL's own build info | `...\Data\Launcher.manifest.meta` (binary; contains `EpicGamesLauncher:...:<version>`) |
| EGL user config | `%LOCALAPPDATA%\EpicGamesLauncher\Saved\Config\WindowsEditor\GameUserSettings.ini` (Legendary expects `...\Config\Windows\`, so we probe both) |
| EGL logs | `%LOCALAPPDATA%\EpicGamesLauncher\Saved\Logs\EpicGamesLauncher*.log` |
| EGL binary | `C:\Program Files\Epic Games\Launcher\Portal\Binaries\Win64\EpicGamesLauncher.exe` (64-bit Program Files, not x86) |
| Game-side data | `<InstallLocation>\.egstore\` |

Override: the ProgramData root may be relocated. Read `%PROGRAMDATA%` rather than hard-coding `C:`.

## `LauncherInstalled.dat`

```json
{ "InstallationList": [
  { "InstallLocation": "F:\\Fortnite", "NamespaceId": "fn", "ItemId": "<itemId>",
    "ArtifactId": "Fortnite", "AppVersion": "<build>", "AppName": "Fortnite" } ] }
```

One entry per installed app, including DLC/content packs (which share the folder).

## `.item` (one per installation) — fields we read

| Field | Example (redacted) | Use |
|---|---|---|
| `FormatVersion` | `0` | refuse unknown major formats |
| `AppName`, `DisplayName` | `Fortnite`, `Fortnite` | identity, UI |
| `CatalogNamespace`, `CatalogItemId` | `fn`, `<itemId>` | match to our library |
| `AppVersionString` | `++Fortnite+Release-42.40-CL-<n>-Windows` | installed build |
| `InstallLocation` | `F:\Fortnite` | install path |
| `InstallationGuid` | 32 hex | names `.item` and `.egstore\<guid>.manifest` |
| `ManifestLocation`, `CompleteManifestPath`, `PendingManifestPath` | `F:\Fortnite\.egstore\<guid>.manifest` | the binary manifest of the installed build |
| `StagingLocation` | `F:\Fortnite\.egstore/bps` | presence of files ⇒ interrupted update |
| `bIsIncompleteInstall`, `bNeedsValidation` | `false` | **don't adopt** incomplete installs. Verify if flagged |
| `LaunchExecutable`, `LaunchCommand` | `FortniteGame/Binaries/Win64/FortniteBootstrapper.exe`, ` -obfuscationid=<id>` | launch (launch.md) |
| `bCanRunOffline`, `bRequiresAuth`, `OwnershipToken` (`"false"` string), `bLaunchElevated`, `bAllowMultipleInstances` | | launch flags |
| `ProcessNames`, `BackgroundProcessNames`, `IgnoredProcessNames`, `MainWindowProcessName`, `DlcProcessNames` | `["UnrealEditorFortnite-Win64-Shipping.exe"]` | process tracking |
| `InstallTags`, `InstallComponents` | `[]`, `["<UniqueId>"]` | selected optional components (↔ `.sdmeta` `UniqueId`) |
| `InstallSize` | `37715694099` | display |
| `BaseURLs` | three CDN `.../CloudDir` URLs | download fallback |
| `AppCategories`, `TechnicalType` | `["games","applications"]` / `["hidden"]` | hide content packs from the grid |
| `MainGameAppName`, `MainGameCatalogItemId` | `Fortnite` | DLC/content → parent game |
| `MandatoryAppFolderName` | `Fortnite` | default folder name |
| `SDMetaLocation`, `SDMetaHash` | | optional component menu |
| `PrereqSHA1Hash`, `LastPrereqSucceededSHA1Hash` | `""` | prerequisites already run |
| `bIsApplication`, `bIsExecutable`, `bIsManaged`, `bIsFab` | | type flags |

Fields present but unused: `EoshRevision`, `HostInstallationGuid`, `InstallSessionId`, `ManifestHash`
(empty on this machine), `SidecarConfigRevision`, `SidecarDeploymentId`, `PreloadState`,
`VaultThumbnailUrl`, `VaultTitleText`, `ChunkDbs`, `CompatibleApps`, `AllowedUriEnvVars`,
`bAllowUriCmdArgs`, `bSDMetaMigrated`.

## `.egstore\` in the game folder

- `<InstallationGuid>.manifest`: binary manifest of the installed build (header verified, manifests.md).
- `Pending\`: manifest of an in-progress update (empty ⇒ none).
- `bps\`: staging area for patches. Exists with files ⇒ an interrupted update. Adopt as "needs verify".
- `*.mancpn`: `{FormatVersion, AppName, CatalogNamespace, CatalogItemId}` [REF]. Not present on this machine.

## `GameUserSettings.ini` (EGL user config)

Section names embed the account id (`[<accountId>_Launcher]`, `[<accountId>_Settings]`...).
`[RememberMe] Data=` holds **EGL's stored login**. **We never read it** (ADR 0003). Our users sign in
through our own login. We don't read other values either in v1.

## Registry

`HKCU\Software\Epic Games\Unreal Engine\Identifiers` has `AccountId` and `MachineId`. Not read.

## Import algorithm (Phase 3)

1. Scan (read-only): parse `LauncherInstalled.dat` + all `.item` files. Skip entries whose
   `AppCategories` contains `hidden` from the grid, attaching them to their `MainGameAppName` instead.
2. For each candidate: the folder exists, `.egstore\<guid>.manifest` exists and parses,
   `bIsIncompleteInstall == false`, and the app is in our assets list (owned).
3. Show the list: name, folder, size, build, and "needs verify" if `bps`/`Pending` aren't empty or
   `bNeedsValidation`.
4. On the user's confirmation per game ("Use this install from F:\Fortnite"), we **copy** the
   manifest into our manifest store and create our `games` row pointing at the folder. EGL's files
   stay untouched and EGL keeps working with the same folder.
5. Before the first update/verify/uninstall/move of an adopted install, show a one-time warning
   naming the folder ("This folder is also managed by the Epic Games Launcher. Updating it here
   will make the Epic Games Launcher re-check it."). No EGL `.item` is rewritten in v1.
