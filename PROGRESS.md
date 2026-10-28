# Next Launcher: progress

## Phase checklist

- [x] **Phase 0: Research** (2026-10-28). Specs in `docs/protocol/`, ledger in `research/notes.md`.
- [ ] Phase 1: Skeleton and speed (mock data)
- [ ] Phase 2: Auth (**blocked until the owner has reviewed `docs/protocol/web-only.md`**)
- [ ] Phase 3: Library
- [ ] Phase 4: Downloads and installs
- [ ] Phase 5: Launch
- [ ] Phase 6: Store
- [ ] Phase 7: Settings, wallet, profile menu
- [ ] Phase 8: Cloud saves
- [ ] Phase 9: Polish and ship

## Phase 0 summary

What exists: 13 protocol specs, each endpoint tagged [OBS]/[REF]/[GUESS]; 5 ADRs; research ledger.

Key findings:
- Everything needed for library, install, update, verify, launch and cloud saves is available
  through Epic's APIs for the user's own account. Nothing requires crossing a section 1 line.
  Manifest/chunk keys, download tickets and ownership tokens all come from Epic for the user's entitlements.
- Newer builds may use **signed chunk URLs** (download ticket + artifact-delivery service) and
  **encrypted manifests/chunks** (feature level ≥ 22/24, keys from the assets API). Both are specced.
- A token → web-session bridge exists (`/id/exchange`, `POST /id/api/exchange` validated with a dummy
  code), so account pages can open signed in. Account pages moved to `accounts.epicgames.com`.
- Wallet balance has a REST endpoint. Whether the launcher token may call it is checked in Phase 2/7.
- Store: `store.epicgames.com/graphql` works for search. The launcher store GraphQL host now shows
  an hCaptcha challenge (not used). The platform-service REST API covers products.
- Image CDN resizes server-side (489 KB → ~30 KB), so no local image encoder is needed (ADR 0004, proposed).

Not available via API → embedded web page: see `docs/protocol/web-only.md` (22 items + 6 permission-dependent).

## Open decisions for the owner

1. **License** (ADR 0001): GPL-3.0-or-later recommended, or MIT.
2. **Image format** (ADR 0004): store CDN-resized JPEG/WebP as-is instead of transcoding to WebP.
3. **Test game** for Phase 4: a small free game on the owner's account (Fortnite is unsuitable).

## Conventions

- Test installs go to `C:\Users\hezy\Documents\Claude\Next\test-installs\` (git-ignored).
- Real installs (e.g. `F:\Fortnite`) and `C:\ProgramData\Epic\...` are read-only unless the owner confirms in chat.
