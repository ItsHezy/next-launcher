# ADR 0016: Epic's account pages inside the app, in the app's look

Status: Accepted (2026-10-30), owner's decisions: "every single Setting is in the app and not a
second window", then "it still looks like the App and feels like software not web". Changes how
the web-only features in `docs/protocol/web-only.md` are shown; the sign-in bridge (auth.md §6)
is unchanged.

- **Where:** Settings rows that need Epic's own page (email, country, password, 2FA, connections,
  privacy, communication preferences, sign out of all devices, delete account, wallet, rewards,
  gift and V-Bucks cards, transactions, payment methods, refunds, gifts, help, legal pages) show
  that page inside Settings, in the section the row belongs to. The bar reads "‹ Section  Page
  title"; the back button, Escape, any other Settings section, search or leaving Settings closes
  it. The wallet menu, profile menu and account dialog fallbacks go to the same place.
- **Why not native forms:** Epic has no launcher API for these (web-only.md). Rebuilding them would
  mean scripting Epic's private website calls, which sit behind Cloudflare and Epic's own hCaptcha
  ("Talon", seen on the account pages) and re-verification. That is the "no scraping beyond what
  the UI needs" line, so the page itself is shown, dressed as the app.
- **How it's shown:** a child webview (`webpane.rs`, Tauri feature `unstable`, no new crate) on
  the main window, over the Settings content box. The UI reports the box's position; the view
  follows window and layout changes and hides while a dialog is open, since a native view always
  draws above the app's own content. It uses the same web profile as before. Only `https`
  navigation; links that ask for a new window open in the same view. Logs keep host and path only
  (no exchange codes). Closing it frees its browser process and refreshes library and wallet.
- **How it looks and behaves** (`webpane.css`, `webpane.js`, injected before Epic's own scripts,
  13 KB, cosmetic only, reads and sends nothing):
  - Epic's site header, footer and account section menu are hidden; the page's own title gives
    way to the launcher's bar.
  - The app's colours, cards, pill buttons, fields, switches, tabs, menus, dialogs and scrollbars
    via Material UI's stable class names. Epic's blue on custom controls becomes the app accent
    (the user's accent setting, validated as `#rrggbb` before it goes into CSS).
  - No context menu (except in text fields), reload, print, find, zoom, history keys, mouse
    back/forward, middle-click tabs or dragging; autofill suggestions off.
  - Hidden while a page loads: the app's spinner shows, never a white or half-built page. Sign-in
    hops stay hidden; one still on screen after 1.5 s is waiting for the user and is shown.
  - Never touched: Epic's sign-in, Cloudflare and hCaptcha checks, cookie consent.
  - If Epic redesigns, only the look degrades to Epic's own; nothing breaks.
- **Faster account pages:** `accounts.epicgames.com` pages open directly (the web profile is
  normally signed in); only if Epic redirects to its login page are they reopened, once, through
  the exchange-code hop. One navigation instead of six, and fewer Cloudflare checks. Other sites
  (store wallet, V-Bucks card) keep the hop, since they don't redirect when signed out.
- **Not changed:** the store's checkout and product pages, Epic's sign-in page and corrective
  actions still use their own window (they aren't settings). "Logs folder" still opens the folder
  in Explorer.
- **Also in-app now:** the app's license and the third-party notices (About) show in a dialog
  instead of opening Notepad (`read_bundled`).
- **Limits:** toasts that appear while the page is shown are drawn under it. Epic or Cloudflare
  may ask for a human check (seen during development with the debugger attached); the user
  completes it on Epic's page, never the app. Changing a protected setting (password page) asks
  for Epic's own 2FA check first.
- **Verified live (dev build, owner's account, no setting changed, no check clicked):** Account
  settings, Password & security (Epic's 2FA prompt shown in the app's dialog style), Transaction
  history, Communication preferences, Redeem a code, Epic Rewards and Terms of Service, each in
  the app's look; account pages open in one navigation; the view followed a resize to 1000 × 660
  and hid under a menu; a new-window link stayed in the view; back, Escape and Library closed it
  and its browser process ended; only the main window existed throughout. Release exe 9.18 MiB
  before the 13 KB of styling (no size cost from the child view).
