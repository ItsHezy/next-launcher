# Account: profile, connections, settings (API vs embedded web)

Crate: `egs-api::account`. Host `ACCOUNT` = `account-public-service-prod03.ol.epicgames.com`.

Rule: use a real API when one exists **and** the launcher client has permission for it.
Otherwise open Epic's own page in the signed-in **web window** (auth.md §6). The user
never types their password just to reach a setting.

## Endpoints

### Account info — [REF]

`GET https://ACCOUNT/account/api/public/account/<accountId>` (bearer)

```json
{
  "id": "<accountId>", "displayName": "<name>", "name": "<first>", "lastName": "<last>",
  "email": "<email>", "emailVerified": true, "country": "US", "preferredLanguage": "en",
  "tfaEnabled": true, "canUpdateDisplayName": true, "lastDisplayNameChange": "<iso>",
  "numberOfDisplayNameChanges": 0, "ageGroup": "ADULT", "headless": false,
  "minorVerified": false, "cabinedMode": false, "hasHashedEmail": false
}
```

The email is shown in Settings → Account only. It's never written to logs (README, error envelope).
It's cached in SQLite `accounts` so the profile card renders on cold start.

### Display names for other ids — [REF]

`GET https://ACCOUNT/account/api/public/account?accountId=<id>&accountId=<id>...` (≤ 100 ids). Used for friends (later).

### Connected accounts (read) — [REF]

`GET https://ACCOUNT/account/api/public/account/<accountId>/externalAuths` (bearer)

```json
[ { "type": "psn", "externalDisplayName": "<name>", "externalAuthIdType": "psn_user_id",
    "authIds": [{ "id": "<redacted>", "type": "psn_user_id" }], "dateAdded": "<iso>" } ]
```

`type` values seen in references: `psn`, `xbl`, `nintendo`, `steam`, `github`, `twitch`, `google`, `apple`, `lego`.
We show PlayStation, Xbox, Nintendo, Steam and Discord, plus anything else returned.
Connect and disconnect go through the web page: linking needs the other platform's own login.

### Update account (display name, language) — [REF], permission [GUESS]

`PUT https://ACCOUNT/account/api/public/account/<accountId>` (bearer), JSON body with any of
`displayName`, `preferredLanguage`, `name`, `lastName`. Requires
`account:public:account UPDATE`. Email/password/username need `...:sensitive UPDATE`, which
we never attempt.

Phase 7: call `verify?includePerms=true` once per session. If the launcher token has the
permission, edit in-app. Otherwise the row opens the web page. Display-name changes are
rate-limited by Epic (`canUpdateDisplayName`, `lastDisplayNameChange`), so the UI shows the next allowed date.

### Avatar — [GUESS]

Not covered by our references. Until Phase 7 verifies an avatar endpoint, the avatar is the
user's initials on an accent-colored circle. Epic's own avatar art is not bundled.

### Corrective actions — [REF]

A token request can fail with `corrective_action_required` (`correctiveAction` e.g.
`PRIVACY_POLICY_ACCEPTANCE`, `DATE_OF_BIRTH`, `DISPLAY_NAME_CHANGE`) plus `continuationUrl`. We open that URL in the web
window and let the user complete it on Epic's page.

## Web pages (signed in through the exchange-code bridge)

Account pages are served from `accounts.epicgames.com` **[OBS]** (`www.epicgames.com/account/*`
returns 302 to `accounts.epicgames.com/account/*`). That site logs in through
`/auth/login?clientId=007c0bfe154c4f5396648f013c641dcf&returnPath=...` **[OBS]**. That's Epic's
website client, using SSO from the `www.epicgames.com/id` session. So the bridge is:

1. establish the id session (auth.md §6),
2. navigate to `https://accounts.epicgames.com/account/<page>`,
3. its `/auth/login` hop completes silently via SSO. **[GUESS]** This must be verified in Phase 2.
   If it prompts for a password, that is a blocker to report, not to work around.

All `/account/*` routes redirect to login when signed out, even made-up ones, so
which page paths exist can only be confirmed once signed in (Phase 7). Paths below are
**[GUESS]** unless marked.

## Settings → Account: source of each row

| Row | Read | Change | Notes |
|---|---|---|---|
| Profile card (name, email, "signed in") | API account info | — | initials avatar until avatar API verified |
| Display name | API | API if permitted, else web `/account/personal` | honour change cooldown |
| Email | API | web `/account/personal` | sensitive |
| Country | API | web `/account/personal` | Epic restricts country changes |
| Communication language | API `preferredLanguage` | API if permitted, else web `/account/personal` | |
| Password | — | web `/account/password` | |
| Two-factor authentication | API `tfaEnabled` | web `/account/password` | |
| Active sessions | — | web, if Epic has such a page (verify) | hide the row if no page exists |
| Sign-in history | — | web, if Epic has such a page (verify) | hide the row if no page exists |
| Connected accounts | API externalAuths | web `/account/connections` | |
| Privacy toggles | — | web `/account/privacy` | no Epic-account privacy API found (GlobalService privacy is Fortnite-scoped) |
| Email / marketing preferences | — | web `/account/email-preferences` | |
| Download my data | — | web `/account/privacy` | |
| Sign out of all devices | — | API `sessions/kill?killType=OTHERS` if permitted, else web `/account/password` | confirmation dialog first |
| Delete account | — | **web only** `/account/personal` | never an API call, irreversible, Epic's own confirmation flow |

The same split applies to Wallet & rewards rows: see [wallet-and-redeem.md](wallet-and-redeem.md).
The full list of web-only features is in [web-only.md](web-only.md).
