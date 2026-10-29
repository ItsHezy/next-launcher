# What is NOT available via API → embedded signed-in web page

Phase 0 result (2026-10-28), to be shown to the owner before Phase 2.

**How "signed-in web page" works:** our token → single-use exchange code → web session cookie
in a dedicated web window → Epic's page opens already signed in (auth.md §6).
**Verified in Phase 2 (2026-10-28):** with all web cookies deleted, the account Password page
opened signed in from the stored session alone. No extra sign-in is ever needed.

## A. Web page only (no usable API found)

| # | Feature | Where it lives | Notes |
|---|---|---|---|
| 1 | Change password | accounts.epicgames.com `/account/password` | |
| 2 | Two-factor authentication setup | `/account/password` | on/off *status* is read via API |
| 3 | Connect / disconnect PlayStation, Xbox, Nintendo, Steam, Discord | `/account/connections` | the *list* of connections is read via API |
| 4 | Change email, change country | `/account/personal` | values are read via API |
| 5 | Epic account privacy settings | `/account/privacy` | |
| 6 | Email / marketing preferences | `/account/email-preferences` | |
| 7 | Download my data | `/account/privacy` | |
| 8 | Delete account | `/account/personal` | always Epic's own flow |
| 9 | Active sessions, sign-in history | unknown, may not exist on Epic's site | **row hidden** if Phase 7 finds no page |
| 10 | Add funds to wallet | store.epicgames.com `/wallet` [OBS] | payment on Epic's page |
| 11 | Redeem Epic gift card / Fortnite V-Bucks card | store `/redeem` [OBS], fortnite.com `/vbuckscard` | plain codes may use the API (B) |
| 12 | Transaction history | `/account/transactions` [OBS] | |
| 13 | Payment methods | `/account/payment-management` | |
| 14 | Refund requests | from the transactions page | |
| 15 | Gifts, coupons (details) | account pages | coupon *count* via API (B) |
| 16 | Epic Rewards (details) | account pages | balance via API (B) |
| 17 | Wishlist, cart | store `/wishlist`, `/cart` | |
| 18 | Checkout, purchases, claiming a free game | store product page / checkout | user completes payment and confirmation. Nothing automated |
| 19 | Product-page extras (long description, screenshots, system requirements) | store product page | only if Phase 6 finds no API module |
| 20 | Unreal Engine tab, Fab tab | unrealengine.com, fab.com | lazy-loaded web views for now |
| 21 | Help & support, contact support, legal pages | epicgames.com help/legal pages | legal pages don't need sign-in |
| 22 | "Corrective actions" Epic may demand (accept new privacy policy, confirm birth date, etc.) | `continuationUrl` from the token error | opens automatically when Epic asks |

**Phase 7 (2026-10-29):** page paths verified and corrected in [account.md](account.md)
"Verified Epic pages". Row 9 (active sessions, sign-in history): no such page, rows hidden.

## B. API exists, permission resource present (action bits checked in Phase 7)

The Phase 2 sign-in showed 150 permissions on the launcher token, including resources for
account updates, token/session kill, fulfillment (codes), rewards (wallet) and library. Phase 7
checks the exact resource + action for each row below; any row Epic denies falls back to its page.

| Feature | API | Fallback page |
|---|---|---|
| Change display name / communication language | `PUT /account/api/public/account/<id>` | **API (permitted)** |
| Sign out of all devices | `DELETE /account/api/oauth/sessions/kill?killType=OTHERS` | **web** `/account/password` (not permitted) |
| Wallet balance, Epic Rewards, coupon count | `egs-platform-service .../account/wallet` | **API (permitted)** |
| Redeem a code | `fulfillment .../codes/<code>` | **API (permitted)** |
| Achievements | store GraphQL `Achievement` / `PlayerAchievement` | not built; no global web page exists (Phase 9) |
| Store "Featured" row | not found | overview of the other store rows (Phase 6) |

## C. Needs a persistent connection, not REST

| Feature | Plan |
|---|---|
| Online status selector (profile menu) | needs an XMPP/EOS socket. Built only if it fits the idle RAM/CPU budget. Otherwise "Online" only |
| Chat / invite notifications | disabled until friends & chat exist |

## D. Things we deliberately don't do (section 1)

- Reuse the official launcher's saved login (`RememberMe`). Users sign in through our own login page.
- Use the store GraphQL host that showed us an **hCaptcha** challenge (`launcher.store.epicgames.com`)
  or try to get past any captcha. If a store call gets challenged, that view becomes the web store.
- Use any Epic client other than the launcher's own (`launcherAppClient2`).
- Send telemetry anywhere.

Nothing found in Phase 0 requires crossing a section 1 line. Ownership tokens, manifest and
chunk keys, and download tickets all come from Epic's API for the signed-in user's own
entitlements, exactly as EGL receives them.
