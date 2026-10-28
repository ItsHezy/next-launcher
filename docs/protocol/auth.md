# Auth: login, tokens, exchange codes, sign-out, web session

Crate: `egs-auth`. Host: `account-public-service-prod03.ol.epicgames.com` (`ACCOUNT` below).

## Summary

1. The user signs in on **Epic's own login page** inside an embedded webview. 2FA, captcha,
   EULA prompts and parental flows all happen in that page. We never see a password and
   never automate any step.
2. The page ends at `/id/api/redirect`, which returns a single-use **authorization code**.
3. We exchange that code for an `eg1` access token and a refresh token.
4. The refresh token goes into Windows Credential Manager (`keyring`). The access token
   stays in memory only.
5. On start we refresh silently. On sign-out we kill the session and delete the keyring entry.

## 1. Login page → authorization code

### 1a. Redirect flow (primary) — [REF] Legendary, community docs; host + redirect API [OBS]

Open in a dedicated login webview (own WebView2 data directory, see ADR 0002):

```
https://www.epicgames.com/id/login?redirectUrl=<urlencoded R>
R = https://www.epicgames.com/id/api/redirect?clientId=34a02cf8f4414e29b15921876da36f9a&responseType=code
    [&codeChallenge=<b64url(sha256(verifier))>&codeChallengeMethod=S256]   ← PKCE, [REF] verify in Phase 2
```

After sign-in the page navigates to `R`, which responds with JSON (rendered as the document body):

```json
{
  "warning": "Do not share this code with any 3rd party service. It allows full access to your Epic account.",
  "redirectUrl": "https://accounts.epicgames.com/fnauth?code=<code>",
  "authorizationCode": "<code>",
  "exchangeCode": null,
  "sid": null
}
```

How we read it (Phase 2 picks one; both keep the code out of logs):

- **Init script + navigation intercept** (preferred). An initialization script injected
  into the login webview runs only when `location.origin === 'https://www.epicgames.com'`
  and `location.pathname === '/id/api/redirect'`. It parses `document.body.innerText` and
  navigates to `https://next-launcher.invalid/auth#code=<code>`. The Rust `on_navigation`
  handler matches that URL, takes the fragment and **cancels the navigation**. No IPC is
  exposed to the remote origin.
- **Fallback:** if the page is not plain JSON (e.g. Epic adds a wrapper), the same script
  reads `<pre>` text.

If the user is already signed in (cookie still valid), the login page skips straight to `R`.
That's fine: "sign in" then completes without typing.

### 1b. EGL JS bridge (alternative) — [REF] Legendary `webview_login.py`

When the login page runs inside EGL it calls `window.ue.signinprompt.requestexchangecodesignin(code, param)`
to hand over an **exchange code**, and `registersignincompletecallback(...)`. A client can
define `window.ue` with those functions. We keep this as a fallback only. It depends on
an undocumented EGL interface that Epic can change.

### 1c. Not used

- `password` grant, `device_auth`, `device_code`, `otp`: never. They would mean handling
  credentials or creating persistent device credentials.
- `POST https://www.epicgames.com/id/api/login` (email+password+captcha): never.

## 2. Token endpoint — [REF], token format [OBS]

`POST https://ACCOUNT/account/api/oauth/token`

Headers:
- `Authorization: Basic base64(<clientId>:<clientSecret>)`
- `Content-Type: application/x-www-form-urlencoded`
- `User-Agent: UELauncher/...` (see README)

Bodies:

| Purpose | Body |
|---|---|
| first login | `grant_type=authorization_code&code=<code>&token_type=eg1[&code_verifier=<v>]` |
| silent refresh | `grant_type=refresh_token&refresh_token=<refresh>&token_type=eg1` |
| from exchange code (EGL bridge) | `grant_type=exchange_code&exchange_code=<code>&token_type=eg1` |
| anonymous (store only, optional) | `grant_type=client_credentials&token_type=eg1` |

Response (redacted):

```json
{
  "access_token": "eg1~<jwt>",
  "expires_in": 28800,
  "expires_at": "2026-10-28T20:00:00.000Z",
  "token_type": "bearer",
  "refresh_token": "eg1~<jwt>",
  "refresh_expires": 1987200,
  "refresh_expires_at": "2026-11-20T12:00:00.000Z",
  "account_id": "<accountId>",
  "client_id": "34a02cf8f4414e29b15921876da36f9a",
  "internal_client": true,
  "client_service": "launcher",
  "displayName": "<name>",
  "app": "launcher",
  "in_app_id": "<accountId>",
  "device_id": "<id>"
}
```

- `eg1` tokens are JWTs prefixed `eg1~`. **[OBS]** EGL's log shows an `eg1~eyJ...` token in a
  session-kill URL. (EGL leaks token fragments into its log. We must not.)
- Lifetimes: **[GUESS]** about 8 h access and about 23 days refresh for the launcher client. Always use
  the `expires_at` / `refresh_expires_at` values from the response, never constants.
- **Refresh tokens rotate.** Each refresh returns a new refresh token. Persist the new one to
  the keyring *before* the old access token is dropped. If writing fails, keep the old
  token and retry. Never hold two refreshes in flight (one async mutex).

Errors that change UI state:

| errorCode | Meaning | Action |
|---|---|---|
| `errors.com.epicgames.account.oauth.authorization_code_not_found` | code used/expired | restart login |
| `errors.com.epicgames.account.auth_token.invalid_refresh_token` | refresh revoked/expired | sign-in screen (keep cached library visible) |
| `errors.com.epicgames.oauth.corrective_action_required` | e.g. accept new privacy policy, confirm DOB | open `continuationUrl` in the embedded web window, then retry |
| `errors.com.epicgames.account.oauth.single_use_code_not_found` (18001) | bad exchange/auth code **[OBS]** | restart flow |

## 3. Verify — [REF]

`GET https://ACCOUNT/account/api/oauth/verify[?includePerms=true]` with `Authorization: bearer <access>`.
Returns `account_id`, `display_name`, `expires_at`, `auth_method`, and optionally `perms`
(`{resource, action}`; action is a bitmask: 1 create, 2 read, 4 update, 8 delete).

Used on start when a cached access token is still within its lifetime, and in Phase 7 to
check whether the launcher client has a permission before showing an API-backed setting
(see account.md).

## 4. Exchange codes — [REF]

`GET https://ACCOUNT/account/api/oauth/exchange` (bearer) →

```json
{ "expiresInSeconds": 300, "code": "<code>", "creatingClientId": "34a02cf8f4414e29b15921876da36f9a" }
```

- Single use, 5 minutes. Optional `consumingClientId`, and PKCE `codeChallenge(Method)`.
- Uses: (a) the game launch argument `-AUTH_PASSWORD` (launch.md), (b) signing a web view in (§6).
- Never cached, never logged.

## 5. Sign-out — [REF], endpoint host [OBS] logs

| Action | Request |
|---|---|
| Sign out (this app) | `DELETE https://ACCOUNT/account/api/oauth/sessions/kill/<access>` → 204 |
| Sign out of all devices | `DELETE https://ACCOUNT/account/api/oauth/sessions/kill?killType=OTHERS` (requires account permission `account:token:otherSessionsForAccount DELETE`; **[GUESS]** that launcher has it, verify in Phase 7; fallback: web page) |

After a local sign-out: delete the keyring entry, clear the login/web WebView2 profile
cookies for `*.epicgames.com`, and keep the SQLite library cache (marked "signed out") so the
UI stays instant. Clear it fully if the user chooses "Remove account data".

## 6. Web session from our token (no second login) — routes [OBS], success path [GUESS]

Goal: open Epic web pages (account, payment methods, checkout) **already signed in**.

Route A (preferred, one navigation):

```
1. GET  https://ACCOUNT/account/api/oauth/exchange           (bearer)  → <code>
2. navigate web window to:
   https://www.epicgames.com/id/exchange?exchangeCode=<code>&redirectUrl=<urlencoded target>
```

`/id/exchange` exists and serves the identity SPA (HTTP 200). **[OBS]** That it then
exchanges the code and redirects to `target` is **[GUESS]**, to verify in Phase 2.

Route B (fallback, scripted inside the web window, same origin):

```
1. GET  https://www.epicgames.com/id/api/csrf                → 204, sets cookie XSRF-TOKEN   [OBS]
2. POST https://www.epicgames.com/id/api/exchange
        x-xsrf-token: <XSRF-TOKEN cookie>
        Content-Type: application/json
        {"exchangeCode":"<code>"}                            → [OBS] validates code (400/18001 for invalid);
                                                                [GUESS] 204 + session cookies when valid
3. navigate to target
```

- SSO domains that share the session (`GET https://ACCOUNT/account/api/epicdomains/ssodomains`,
  no auth) **[OBS]**: `unrealengine.com`, `unrealtournament.com`, `fortnite.com`, `epicgames.com`.
- Web cookies live only in the dedicated "web" WebView2 profile, never in the main UI webview.
- Web session lifetime is independent of our tokens. Before opening any web page, if
  `/id/api/account` (GET, cookie) returns 401, run Route A again. **[REF]** `/id/api/account` returns
  `{email(masked), displayName, accountStatus, hasPassword, ...}` when signed in.

## 7. Storage

| Item | Where |
|---|---|
| refresh token + `refresh_expires_at` + `account_id` | Windows Credential Manager via `keyring`, service `NextLauncher`, user `<accountId>` |
| access token | memory only |
| display name, account id, avatar url | SQLite `accounts` (not secret) |
| web cookies | WebView2 profile `.../NextLauncher/web-profile/` |

Logs: the `tracing` layer has a redaction filter for anything matching `eg1~`, 32-hex values
after `code=`/`token=`, `Authorization:` headers and email addresses. Tested in `egs-auth`.
