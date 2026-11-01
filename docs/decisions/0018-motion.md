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
- **Second round (same day, owner: "Add way more animations"):**
  - **Gliding highlights:** the Settings sidebar and the Store's categories (Featured, Free games…)
    have one highlight that glides to the chosen item, like the tab bar (`glide` action).
  - **Building in:** a Settings section's heading slides in and its cards follow one after
    another; the product page builds from the top (back, title, maker, picture, purchase box),
    then its details as they arrive; the picture fades in and settles from a slight zoom; store
    section headings come in with their cards; dialogs' contents and menus' items follow them in.
  - **Loading:** the store and a product's details show shimmering placeholders instead of
    "Loading…" text (the text stays for screen readers).
  - **Changes:** a game's status (Ready, Playing, Update available…) in the library and Quick
    Launch, the library's game count, the sync line, the wallet balance and the download's status
    bump when they change (`bump` action, never when first shown); a new accent colour blends
    across the app instead of switching (`@property --accent`, not at start-up); a message shows
    how long it stays with a line that runs out.
  - **Touch:** icon buttons press in; the gear turns, the wishlist heart beats, the cart and back
    arrows lean their way; switches stretch while pressed; colour swatches grow and their tick pops
    in; tiles and recent games lift with a deeper shadow; play buttons, Quick Launch pictures,
    Settings icons and the avatar grow under the pointer; the sign-in and connection banners slide
    in; the "Recently played" row and the empty-library text come in when they reappear.
  - With motion off, delays are off too, so nothing waits to appear.
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
  Svelte's transition and flip runtime plus `motion.ts`; after the second round 43.0 / 78.8 KB.
  Library scroll with 600 games and Animations On: 198 fps, p99 5.1 ms, as before (second round:
  200 fps, no frame over 1.5× vsync). A section switch still shows its page in
  8–13 ms; the fade-up plays after that. Startup is untouched (nothing animates until it's done).
- **Verified live (dev build, Animations On, through the UI view's debugger):** every animation
  above runs on its trigger; the gliding highlights sit exactly on the chosen item; the accent is
  mid-blend 120 ms after a change. Match Windows on this PC and Off run none: a closing dialog
  leaves at once, menu items are fully shown 30 ms after opening, the accent changes at once and a
  message's line is hidden. Removing one of two Quick Launch games moves the other into place.
