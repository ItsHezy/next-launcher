# Presence and social (later phases)

Not needed for v1 except the **online status selector** in the profile menu. Social chat is
not in the design. All [REF] (community docs) unless marked.

## Presence

`PATCH https://presence-public-service-prod.ol.epicgames.com/presence/api/v1/<ns>/<accountId>/presence/<connectionId>`
(namespace `_` for the launcher). The `connectionId` comes from a live **XMPP** or **EOS
Connect (STOMP)** connection, so presence cannot be set with a plain REST call. It needs a
persistent socket held while the app runs.

Plan (Phase 7):

- Statuses offered: Online, Away, Invisible (Do-not-disturb if Epic supports it, **[GUESS]**).
- Implementation needs one persistent connection. Before adding it, measure its RAM/CPU cost
  against the idle budget (≤ 150 MB, ~0% CPU). If it can't meet the budget, the selector is
  shown as "Online" only, with a note, and that trade-off is flagged to the owner.

## Friends / invites / chat notifications

Settings → Notifications lists "chat" and "invites" toggles. They depend on the same
persistent connection. Until it exists they're shown disabled with "Available when friends &
chat are supported". This is called out in [web-only.md](web-only.md).

## Lightswitch (service status)

`GET https://lightswitch-public-service-prod.ol.epicgames.com/lightswitch/api/service/<serviceId>/status`
(bearer) → `{status: "UP"|"DOWN", message, maintenanceUri, allowedActions, banned, launcherInfoDTO{appName, catalogItemId, namespace}}`.
Used by EGL for games like Fortnite. Optional: Support → Troubleshoot can show Epic service status.
