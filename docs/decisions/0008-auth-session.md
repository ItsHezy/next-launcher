# ADR 0008: Auth session design

Status: Accepted (2026-10-28), verified with a live sign-in.

- **Login**: Epic's own page in a separate native window (`login`), with PKCE (S256). An init
  script reads the code from `/id/api/redirect` and navigates to `https://next-launcher.invalid/auth?...`.
  The window's navigation handler takes the code and cancels the navigation. No IPC is exposed to
  remote pages, and the code never hits the network or logs. `NL_NO_PKCE=1` disables PKCE (escape hatch).
- **Storage**: refresh token + expiry in Windows Credential Manager via `keyring` 3.6
  (`windows-native` only; keyring 4 restructured into store crates with Linux backends on by default).
  `set_secret` (UTF-8 bytes, 2,560-byte limit) instead of `set_password` (UTF-16, ~1,280 chars).
  Payloads over 2,000 bytes are chunked across `<id>`, `<id>#1`, … with chunk 0 written last.
  The access token is memory-only.
- **Refresh**: one async mutex around the session, so one refresh at a time. Rotated refresh tokens
  are persisted before use. Access tokens refresh 5 minutes before expiry, using `expires_in`
  against the monotonic clock (immune to clock skew). A single sleeping task schedules this; there is no polling.
  Token grants are never auto-retried.
- **Restore on start**: only after the window is shown. Transient failures keep the stored session
  and retry with backoff (10 s → 5 min); the cached library stays visible ("Offline"). An invalid or
  expired token clears the stored credential and shows "Signed out" with the reason.
- **Sign-out**: kill the Epic session (best effort), delete the credential, mark the cached account
  signed out (library cache kept), delete the `web/` WebView2 profile.
- **TLS**: reqwest 0.13 with `rustls-no-provider` + the `ring` provider + Windows certificate store
  (via rustls-platform-verifier) instead of the default `aws-lc-rs` (smaller, no NASM/CMake build step).
- **Logs**: a redacting writer scrubs tokens, codes, auth headers, launch auth args and emails from
  every line (unit-tested). API types hold secrets in `Secret`, whose `Debug`/`Display` print `<redacted>`.
