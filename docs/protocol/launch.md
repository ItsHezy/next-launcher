# Launch: arguments, game auth, ownership tokens, tracking

Crate: `egs-launch`. Unit tests cover argument building (pure function: inputs → `Vec<OsString>`).

## Inputs

| Input | Source |
|---|---|
| executable, manifest launch command | manifest meta `launch_exe`, `launch_command` (EGL copies them to `.item` `LaunchExecutable` / `LaunchCommand` [OBS]) |
| extra game args | catalog `customAttributes.AdditionalCommandLine.value` [REF] |
| needs ownership token | catalog `customAttributes.OwnershipToken.value == "true"` [REF]. EGL `.item` field `OwnershipToken` [OBS] |
| can run offline | catalog `customAttributes.CanRunOffline.value` (default `true`) [REF]. `.item` `bCanRunOffline` [OBS] |
| launch elevated | `.item` `bLaunchElevated` [OBS]. For our installs: not known from the API, default false |
| EOS deployment id | `sidecar.config` (JSON string) `deploymentId`, from the assets v2 element `sidecar {config, rvn}` [REF] |
| third-party store | `customAttributes.ThirdPartyManagedApp` / `ThirdPartyManagedProvider` [REF] |
| user args | Settings → Manage games → launch arguments |

## Argument order — [REF]

```
<install>\<launch_exe>
  <launch_command split shell-style, Windows rules>          (e.g. Fortnite: -obfuscationid=<id> [OBS])
  <AdditionalCommandLine split>
  -AUTH_LOGIN=unused
  -AUTH_PASSWORD=<exchange code>        (online; omitted value when offline)
  -AUTH_TYPE=exchangecode
  -epicapp=<appName>
  -epicenv=Prod
  [-epicovt=<path to .ovt file>]        (only when ownership token required)
  -EpicPortal
  -epicusername=<displayName>
  -epicuserid=<accountId>
  -epiclocale=<lang, e.g. en-US>
  -epicsandboxid=<namespace>
  [-epicdeploymentid=<deploymentId>]
  <user args>
```

- Working directory: the directory containing the executable.
- Exchange code: `GET /account/api/oauth/exchange` (auth.md §4) **immediately** before launch.
  It's valid for 5 minutes and single use, and is never logged. The log shows `-AUTH_PASSWORD=<redacted>`.
- Offline launch (no network, or the user chose it): skip the exchange code. If
  `CanRunOffline` is `false`, warn ("This game needs to be online to start") but let the user try.
- The game itself handles anti-cheat. We start exactly the manifest's executable with exactly these
  arguments and never touch EAC/BattlEye files or services.

## Ownership token — [REF]

`POST https://ecommerceintegration-public-service-ecomprod02.ol.epicgames.com/ecommerceintegration/api/public/platforms/EPIC/identities/<accountId>/ownershipToken`
(bearer, `Content-Type: application/x-www-form-urlencoded`)

```
nsCatalogItemId=<ns>:<itemId>
```

→ 200, opaque body (write bytes as-is). Saved to `%LOCALAPPDATA%\NextLauncher\tmp\<ns><itemId>.ovt`
and passed as `-epicovt=<path>`. Deleted when the game exits. Epic issues the token only for
the signed-in user's own entitlement. **We never create, modify or cache tokens across launches.**
Failure (e.g. 403) ⇒ "Epic couldn't confirm you own this game" and no launch.

## Third-party launchers — [REF]

Owned titles managed by another store are handed to that launcher with an exchange code:

- EA app: `link2ea://launchgame/<appName>?AUTH_PASSWORD=<code>&AUTH_TYPE=exchangecode&epicusername=<n>&epicuserid=<id>&epiclocale=<lc>[&<AdditionalCommandline pairs>]`
- Ubisoft Connect: `uplay://launch/<customAttributes.GameID or appName>?<same params>`

Opened with `ShellExecuteW`. If the protocol isn't registered, we show "Install <launcher> to play this game".

## EULA — [OBS] host/route in EGL log, [REF] shape

`GET https://eulatracking-public-service-prod06.ol.epicgames.com/eulatracking/api/public/agreements/<eulaId>/account/<accountId>?locale=<lc>`
→ 204 when nothing is pending, else 200 with the agreement (`key`, `version`, `title`, `body`, ...).
EGL checks `epicgames_tos` at start. Phase 2: check `epicgames_tos` after sign-in. If one
is pending, show Epic's text in-app with a link to the full page and **accept only on an explicit
user click** (`POST .../agreements/<eulaId>/version/<v>/account/<accountId>/accept?locale=<lc>`).
Never auto-accept.

## Process tracking and playtime

- Start via `CreateProcessW` (or `ShellExecuteExW` with `runas` if `bLaunchElevated`, so UAC is shown).
- Running state: the launched PID plus its descendants, found by a Toolhelp32 snapshot every
  2 s **only while a game is running** (idle CPU stays at 0 otherwise), and processes matching the
  title's known process names (EGL `.item` `ProcessNames`/`MainWindowProcessName` when imported [OBS]).
  This handles bootstrappers that start the real game and exit (e.g. `FortniteBootstrapper.exe` [OBS]).
  No hooks, no handles opened beyond `PROCESS_QUERY_LIMITED_INFORMATION`, no job objects
  (they can interfere with anti-cheat launchers).
- "Playing" state is emitted as `game://state { appName, state: "running" }`.
- On exit: playtime += (end − start), write SQLite, `PUT` playtime to Epic (library.md), delete
  the `.ovt`, resume paused downloads.
- "Pause downloads while playing" pauses the queue on `running` and resumes on exit.
