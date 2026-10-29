# ADR 0005: Store data sources and challenge handling

Status: Accepted (2026-10-28)

- Store lists and search use `store.epicgames.com/graphql` with raw `searchStore` queries
  (works unauthenticated, observed). Product data uses the REST platform service
  (`egs-platform-service.store.epicgames.com`). Free games use the public promotions feed.
- `launcher.store.epicgames.com/graphql` (used by Legendary) answered us with an hCaptcha
  challenge. We don't use it.
- **Any challenge (captcha, bot check, unexpected HTML) is never solved or avoided.** The affected
  view switches to the embedded signed-in web store, and the event is logged once per session
  (without URLs that carry tokens).
- Purchases always happen on Epic's checkout page, completed by the user.

**Phase 6 update (2026-10-29):** product pages use only `catalogOffer` on the same GraphQL host
(the platform service isn't needed yet); queries use GraphQL variables. Store images load straight
from Epic's CDNs (`cdn1.epicgames.com`, `cdn2.unrealengine.com`, resized by the CDN), which the
UI's CSP allows for `img-src` only. A bot check is detected (HTML or a 403/429 without Epic's
error body), logged once, and the view offers "Open Epic's store" instead. No bot check was seen
during Phase 6.
