# ADR 0018: Motion

Status: Accepted (2026-11-01), owner's request: "upgrade the Launcher by adding Animations".
Amends ADR 0007's accessibility line on `prefers-reduced-motion` (see **Setting**).

- **Rules:** transform and opacity only (the compositor moves them; nothing re-lays out the page
  per frame); 150–340 ms with the existing tokens (`--ease-out`, `--ease-spring`); nothing moves
  while the app starts (the first screen appears as it is, start-up budgets unchanged); the
  virtualised library grid never animates while scrolling.
- **Where:**
  - **Moving around:** the tab bar and the library filters have one selection pill that slides to
    the chosen option; a main section fades up into place; Settings opens over the app with a slight
    zoom and the app comes back from under it; a Settings section fades up; the Store page fades
    back after Epic's page.
  - **Cards:** a new filter or sort, or the first games arriving, plays the tiles in view in one
    after another (capped at 12 steps); store cards do the same as they load; the product page
    fades up. Cover art fades in as it arrives (art already cached just shows) and zooms slightly
    under the pointer.
  - **Comes and goes:** dialogs, menus and popovers now also animate out; toasts leave and the rest
    move into place; Quick Launch games slide in and out, the others moving into place; the
    download card slides in and out, and a light runs along its bar while it works.
  - **Status:** "Playing" has a softly pulsing dot; the loading skeleton has a moving sheen.
- **How:** CSS for what is always there (hover, the pill, sheens, the dot); Web Animations from
  `ui/src/lib/motion.ts` for one-off entrances; Svelte transitions (`fade`, `pop`, `drop`,
  `slide`, `shift`) for things that leave. Svelte's transitions and Web Animations aren't covered
  by a CSS rule, so they check `moving()` and run with no duration when motion is off.
- **Epic's page:** it is a native view placed from the layout and can't move with the app. A
  section fading up shifts its box for a moment, so when the app's motion ends (`nl-settled`) the
  box is measured again. The panel around Epic's page only fades (no movement). Checked live: the
  cart opened from the Library sits exactly in its box.
- **Setting:** Settings › General › Appearance › **Animations: Match Windows** (default), **On**,
  **Off**. Match Windows keeps ADR 0007's rule: no motion when Windows' "Animation effects" are
  off (`prefers-reduced-motion`). On is for people who turned Windows' effects off for other
  reasons and still want them here; Off turns them off in the app only. The owner's PC has
  Windows' effects off, so the app showed none of its motion so far, not even the existing menu and
  dialog animations; On shows all of it.
- **Cost:** first-screen JS 39.2 → 42.6 KB gz (budget 80), all JS 74.0 → 77.9 KB gz (budget 250):
  Svelte's transition and flip runtime plus `motion.ts`. Library scroll with 600 games and
  Animations On: 198 fps, p99 5.1 ms, as before. A section switch still shows its page in
  8–13 ms; the fade-up plays after that. Startup is untouched (nothing animates until it's done).
- **Verified live (dev build, Animations On, through the UI view's debugger):** every animation
  above runs on its trigger; Match Windows on this PC and Off run none, and a closing dialog then
  leaves at once; removing one of two Quick Launch games moves the other into place.
