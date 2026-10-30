# ADR 0017: No second window: checkout, store pages and sign-in inside the app

Status: Accepted (2026-10-30), owner's report: "some things are STill in pic to pic mode and not
in the App like getting games" (Epic's checkout opened in its own "Epic Games" window). Extends
ADR 0016 from Settings to every Epic page the app opens; the app now has one window.

- **Where:**
  - **Store:** Get / Buy now (Epic's checkout), "View on Epic's store", the top bar's Wishlist
    and Cart, and "Open Epic's store" (when the store answers with a bot check) show Epic's page
    inside the Store view. The bar reads "‹ Game or Store  Page title"; the back button, Escape,
    the Store tab, typing a search or any other section closes it. The store grid or product page
    under it stays where it was.
  - **Settings › Account:** Epic's sign-in page (every Sign in button), and the step Epic asks
    for before an account can be used (corrective action, "Open Epic" on the banner).
  - Settings pages as in ADR 0016.
- **How:** the same child view (`webpane.rs`), now told what to show (`PaneTarget`: an account
  page, a store page, sign-in, corrective action) and which UI box it belongs to. A box that
  leaves while its page is still opening, or that a newer box replaced, can't close or claim the
  view (owner id), so jumping from the cart straight to a Settings page shows the right page.
  The separate web window, the login window and their commands (`store_open`, `sign_in`,
  `open_corrective_action`) are gone.
- **Store pages:** always through the exchange-code hop (auth.md §6), so a purchase lands on the
  account the app is signed in to, as before. The user reviews and confirms on Epic's page; the
  app never presses anything there. When the page closes, the library syncs and the wallet
  refreshes; an open product page re-checks ownership when the library changes, so a game just
  got shows "In your library".
- **Sign-in:** the same login URL, PKCE and answer capture as before (auth.md §1,
  `LOGIN_INIT_SCRIPT`, which acts only on `/id/api/redirect`). The view catches the answer
  address and never loads it; the view closes before the code is exchanged, so Epic's raw answer
  page is never shown. Leaving the sign-in page cancels signing in (signed out again, as closing
  the login window did). Success returns to where Sign in was pressed; a corrective action goes
  straight on to Epic's page for it; a failure shows its reason. We still never see or type a
  password.
- **Look:** store pages get the app's palette too: Epic's store names its colours with generated
  custom properties, and where those hold Epic's greys and blue they become the app's surfaces and
  accent. The store's header placeholder, its own bar (search, browse, wishlist, cart: the app's
  top bar has them) and footer are hidden. Only stylesheets the page can read are mapped; others
  keep Epic's look.
- **Frames stay Epic's:** WebView2 runs start-up scripts in every frame, not only the page
  (wry: "scripts are always added to subframes" on Windows). The styling script now returns at
  once inside frames, so the checkout's payment frame and human checks (hCaptcha "Talon",
  Cloudflare) are exactly as Epic made them. This also applies to the Settings pages of ADR 0016,
  where it previously reached Epic's own frames (cosmetic only).
- **Limits:** the checkout itself is drawn inside Epic's payment frame, so it keeps Epic's look
  (deliberately). The wishlist page is a separate Epic app whose styles we can't map or name;
  it shows in the app with Epic's colours and its own small bar. Sign-in through PlayStation,
  Xbox, Google etc. behaves as it did in the login window (full-page redirects in the same view;
  providers that refuse embedded browsers refuse here too).
- **Development:** `NL_DEV_UI_DEBUG_PORT` (debug builds only) gives just the app's own view a
  debugger port. With WebView2's global debugging switch Epic's views got one too, and Cloudflare
  then challenged the sign-in hop on every store page; without it the checkout opened straight
  away. The `page:` and `open:` dev actions are gone (they opened the removed window).
- **Verified live (dev build, owner's account, nothing on Epic's pages pressed):** Get on a free
  giveaway showed Epic's checkout ("Add to library", not clicked) inside the Store with
  "‹ Sorry We're Closed  Checkout"; back returned to the product page; the view hid under the
  profile menu; Epic's product page and the wishlist showed in the Store; the product page in the
  app's colours with Epic's bars hidden; cart → Settings › Redeem a code mid-load showed Redeem
  and never the cart. In a fresh signed-out folder, Sign in showed Epic's sign-in page inside
  Settings › Account in the app's look; back cancelled it (signed out, view's browser process
  ended). Not verifiable by us: a completed sign-in (needs the owner's password) and a corrective
  action (Epic decides when one is needed).
