# ADR 0003: Never import the official launcher's saved login

Status: Accepted (2026-10-28)

EGL stores its login in `GameUserSettings.ini` `[RememberMe] Data=`, encrypted with a key that
Legendary ships so it can import the session. We won't do this:

- It reads another application's stored credentials, which is outside "the user's own
  tokens obtained by our client".
- Our own login page flow is short (often zero typing when Epic's cookie is still valid),
  and gives us our own refresh token in the Windows credential store.

Import of EGL *installs* (games on disk) stays in scope. It reads only the non-secret install
manifests, and only adopts an install after the user confirms (egl-local-files.md).
