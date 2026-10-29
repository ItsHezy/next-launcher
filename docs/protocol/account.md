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
3. its `/auth/login` hop completes silently via SSO. **[OBS]** Verified 2026-10-28 with an
   empty web profile: `/account/password` opened signed in.

All `/account/*` routes redirect to login when signed out, even made-up ones, so
which page paths exist can only be confirmed once signed in (Phase 7). Paths below are
**[GUESS]** unless marked.

## Phase 7 result (2026-10-29, owner's account): what the launcher token may do

From `verify?includePerms=true` (bits 1 create, 2 read, 4 update, 8 delete) **[OBS]**:

| Resource | Action | Used for |
|---|---|---|
| `account:public:account` | 6 (read, update) | display name and communication language **in-app** |
| `account:public:account:externalAuths` | 7 | connected-accounts list |
| `fulfillment:public:code` | 1 (create) | redeem a code **in-app** |
| `payment:public:<id>:wallet`, `reward:public:<id>:reward-accounts` | 2 (read) | wallet summary |
| `account:token:otherSessionsWithSameSourceForAccount` | 8 | *not* "all devices": `killType=OTHERS` needs `otherSessionsForAccount`, which the launcher doesn't get, so that row opens Epic's page |
| `presence:_:<id>` | 4 | presence needs a chat (XMPP) connection first; not used (ADR 0011) |

The app derives these capabilities at every sign-in (`account::Capabilities`), with exact
resource + action matching; rows fall back to Epic's page when a capability is missing.

## Verified Epic pages (2026-10-29, in the signed-in web window)

`accounts.epicgames.com` was redesigned: a side menu with *Account* (Settings, Linked accounts,
Communication preferences, Password and security, Legal history), *Payment and rewards*
(Payment settings, Transactions, Subscriptions, In-game currency, Epic rewards, Redeem code)
and *Parental controls*. **[OBS]** Paths checked by title, not content:

| Page | Path | Result |
|---|---|---|
| Settings (name, email, country, privacy info, data download, delete account) | `/account/personal` | ✅ "Settings" |
| Linked accounts | `/account/connections` | ✅ |
| Communication preferences (email) | `/account/messaging` | ✅ (`/account/email-preferences` is 404) |
| Password and security (password, 2FA, sign out) | `/account/password` | ✅; no sessions or sign-in history on it |
| Purchases | `/account/transactions` → `/transactions/purchases` | ✅ |
| Gifts | `/account/transactions/gifts` | ✅ |
| Payment settings | `/account/payments` | ✅ (`/account/payment-management` is 404) |
| Epic rewards | `/account/rewards` | ✅ |
| Redeem code | `/account/code-redemption` | ✅ |
| Legal history | `/account/eula-history` | ✅ (not linked from the app yet) |
| Privacy | `/account/privacy` | ❌ 404: privacy lives on the Settings page |
| Coupons | `/account/coupons`, `/account/transactions/coupons` | ❌ 404: coupons only appear at checkout |
| Active sessions, sign-in history | — | ❌ no such page: **rows hidden** |
| Store wishlist, cart, gifts inbox, publisher index | `store.epicgames.com/{wishlist,cart,gifts,publisher-index}` | ✅ |
| Achievements | `store.epicgames.com/achievements` | ❌ 404: achievements are per game on product pages |
| V-Bucks card | `www.fortnite.com/vbuckscard` | ✅ |
| Legal | `legal.epicgames.com/epicgames/tos`, `/epicgames/privacy-policy`, `/store/refund-policy`, `/store/eula` | ✅ public |
| Help | `www.epicgames.com/help/en-US/` | ✅ public |

Note: with a DevTools port open, `store.epicgames.com` answered every navigation with its
"Just a moment…" bot check; `accounts.epicgames.com` did not. Without the debug port (normal
use) store pages load. We never try to get past that check.

## Settings → Account: source of each row (as built)

| Row | Read | Change |
|---|---|---|
| Profile card, display name, email, country | API account info | name: **API** (dialog, 3–16 chars, Epic's cooldown honoured via `canUpdateDisplayName`); email, country: web `/account/personal` |
| Communication language | API `preferredLanguage` | **API** (dialog) |
| Password | — | web `/account/password` |
| Two-factor authentication | API `tfaEnabled` | web `/account/password` |
| Connected accounts (PlayStation, Xbox, Nintendo, Steam, Discord + "Other") | API externalAuths | web `/account/connections` |
| Privacy & your data, delete account | — | web `/account/personal` |
| Communication preferences | — | web `/account/messaging` |
| Sign out of all devices | — | web `/account/password` (no permission for `OTHERS`) |
| Active sessions, sign-in history | — | hidden (no page exists) |

The same split for Wallet & rewards is in [wallet-and-redeem.md](wallet-and-redeem.md).
