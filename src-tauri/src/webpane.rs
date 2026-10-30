//! Epic's own pages inside the main window, never a second window (ADR 0016, 0017): account,
//! wallet, help and legal pages in Settings, store pages and the checkout in the Store, and
//! Epic's sign-in page. A child webview sits over a box of the UI; the UI reports where that
//! box is and hides the view while a menu or dialog is open. It uses the app's Epic web profile
//! (signed in through the exchange-code bridge, auth.md §6).
//!
//! The pages wear the app's look (`webpane.css`) and behave like software, not a browser
//! (`webpane.js`): no site header or footer, no context menu, reload, print or zoom. The view
//! stays hidden while a page loads, so the UI's own loading state shows instead of web pages
//! assembling themselves.

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::time::Duration;

use serde::Deserialize;
use tauri::webview::{Color, NewWindowResponse, PageLoadEvent, WebviewBuilder};
use tauri::{AppHandle, LogicalPosition, LogicalSize, Manager, State, Url, Webview, WebviewUrl};

use crate::auth::EpicPage;
use crate::commands::{AppState, UiError};
use crate::store::StoreTarget;

const PANE: &str = "epic-pane";
/// The app's `--surface`, so an empty view never flashes white.
const SURFACE: Color = Color(0x2e, 0x30, 0x33, 0xff);
const DEFAULT_ACCENT: &str = "#0a6fd6";
/// Sign-in hops redirect on their own; one still on screen after this is waiting for the user
/// (e.g. Epic's human check) and is shown.
const HOP_GRACE: Duration = Duration::from_millis(1500);

/// The UI's box for the view, in CSS pixels of the main window.
#[derive(Debug, Clone, Copy, Deserialize, specta::Type)]
pub struct PaneBounds {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// What the view shows.
#[derive(Debug, Clone, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum PaneTarget {
    /// One of Epic's account, wallet, help or legal pages (Settings).
    Page { page: EpicPage },
    /// A store page: the checkout, a product, the wishlist or cart (Store).
    Store { target: StoreTarget },
    /// Epic's sign-in page; what it returns signs the app in (Settings › Account).
    SignIn,
    /// The step Epic asked for before the account can be used (corrective action).
    Corrective,
}

impl PaneTarget {
    /// For logs: which page, never ids or search text.
    fn name(&self) -> String {
        match self {
            Self::Page { page } => format!("{page:?}"),
            Self::Store { target } => format!("store {}", target.kind()),
            Self::SignIn => "sign-in".into(),
            Self::Corrective => "corrective action".into(),
        }
    }
}

#[derive(Default)]
pub struct PaneState {
    /// The UI box that owns the view (0: none). A view that finishes opening after its box left
    /// closes again, and a late close from a box that was already replaced does nothing.
    owner: AtomicU32,
    /// One open at a time, so two quick clicks can't both create the pane.
    opening: tokio::sync::Mutex<()>,
    /// A menu or dialog of the app is over the view.
    covered: AtomicBool,
    /// The current page has finished loading.
    loaded: AtomicBool,
    /// Counts page loads, so a late show or hide for an older page does nothing.
    generation: AtomicU64,
    /// An account page opened without the sign-in hop (the web profile is usually signed in
    /// already). If Epic sends it to its login page instead, it is opened again through the
    /// hop, once.
    direct: std::sync::Mutex<Option<String>>,
}

fn err(what: &str) -> UiError {
    UiError {
        code: "web".into(),
        message: format!("Couldn't show Epic's page ({what})."),
    }
}

fn place(webview: &Webview, b: PaneBounds) {
    let _ = webview.set_position(LogicalPosition::new(b.x, b.y));
    let _ = webview.set_size(LogicalSize::new(b.width.max(1.0), b.height.max(1.0)));
}

/// `#rrggbb` from the UI's accent setting; anything else gets the default (it goes into CSS).
fn accent(value: &str) -> &str {
    let hex = value.strip_prefix('#').unwrap_or("");
    if hex.len() == 6 && hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        value
    } else {
        DEFAULT_ACCENT
    }
}

/// The script injected into every page of the view: the stylesheet plus the behaviour rules.
fn page_script(accent_value: &str) -> String {
    let css = include_str!("webpane.css").replace("__ACCENT__", accent(accent_value));
    let css = serde_json::to_string(&css).unwrap_or_else(|_| "\"\"".to_owned());
    include_str!("webpane.js").replace("__CSS__", &css)
}

/// Account pages redirect to Epic's login page when the web profile isn't signed in, so they
/// can be tried directly (five fewer redirects than the exchange hop).
fn try_direct(url: &str) -> bool {
    url.starts_with("https://accounts.epicgames.com/")
}

/// Epic's login page: the web profile isn't signed in (any more).
fn is_login(url: &Url) -> bool {
    url.host_str() == Some("www.epicgames.com") && url.path().starts_with("/id/login")
}

/// Pages that only pass the sign-in along to the next one.
fn is_hop(url: &Url) -> bool {
    let path = url.path();
    path.starts_with("/id/exchange") || path.starts_with("/id/api/") || path.starts_with("/auth/")
}

/// Show the view if its page is loaded and nothing of the app covers it.
fn reveal(webview: &Webview, state: &PaneState) {
    if state.loaded.load(Ordering::SeqCst) && !state.covered.load(Ordering::SeqCst) {
        let _ = webview.show();
    }
}

fn on_page_load(webview: Webview, event: PageLoadEvent, url: &Url) {
    let state = webview.state::<PaneState>();
    match event {
        PageLoadEvent::Started => {
            state.loaded.store(false, Ordering::SeqCst);
            let generation = state.generation.fetch_add(1, Ordering::SeqCst) + 1;
            tauri::async_runtime::spawn(async move {
                let state = webview.state::<PaneState>();
                if state.generation.load(Ordering::SeqCst) == generation
                    && !state.loaded.load(Ordering::SeqCst)
                {
                    let _ = webview.hide();
                }
            });
        }
        PageLoadEvent::Finished => {
            let generation = state.generation.load(Ordering::SeqCst);
            let hop = is_hop(url);
            tauri::async_runtime::spawn(async move {
                if hop {
                    tokio::time::sleep(HOP_GRACE).await;
                }
                let state = webview.state::<PaneState>();
                if state.generation.load(Ordering::SeqCst) == generation {
                    state.loaded.store(true, Ordering::SeqCst);
                    reveal(&webview, &state);
                }
            });
        }
    }
}

/// Where the view goes, and the page to open again through the sign-in hop if Epic shows its
/// login page instead.
async fn address(
    app: &AppHandle,
    target: &PaneTarget,
) -> Result<(String, Option<String>), UiError> {
    Ok(match target {
        PaneTarget::Page { page } => {
            let url = page.url();
            if !page.needs_sign_in() {
                (url.to_owned(), None)
            } else if try_direct(url) {
                (url.to_owned(), Some(url.to_owned()))
            } else {
                (crate::auth::signed_in_url(app, url).await, None)
            }
        }
        // Always through the hop: a purchase lands on the account the app is signed in to.
        PaneTarget::Store { target } => {
            let url = crate::store::page_url(app, target)?;
            (crate::auth::signed_in_url(app, &url).await, None)
        }
        PaneTarget::SignIn => (crate::auth::begin_sign_in(app)?, None),
        PaneTarget::Corrective => (crate::auth::corrective_url(app)?, None),
    })
}

/// Show `target` in the view (signed in when it needs it), creating the view on first use.
/// `id` names the UI box asking; `accent` is the app's accent colour (`#rrggbb`), used by the
/// page's styling.
#[tauri::command]
#[specta::specta]
pub async fn web_pane_open(
    app: AppHandle,
    state: State<'_, AppState>,
    pane: State<'_, PaneState>,
    id: u32,
    target: PaneTarget,
    bounds: PaneBounds,
    accent: String,
) -> Result<(), UiError> {
    pane.owner.store(id, Ordering::SeqCst);
    let _one = pane.opening.lock().await;
    tracing::info!(page = %target.name(), "showing Epic page in the app");
    let sign_in = matches!(target, PaneTarget::SignIn);
    if !sign_in {
        crate::auth::cancel_sign_in(&app);
    }
    let (url, retry) = address(&app, &target).await?;
    if let Ok(mut d) = pane.direct.lock() {
        *d = retry;
    }
    let parsed: Url = url.parse().map_err(|_| err("address"))?;
    // The box asking was left (or replaced) meanwhile.
    let left = |pane: &PaneState| {
        let gone = pane.owner.load(Ordering::SeqCst) != id;
        if gone && sign_in {
            crate::auth::cancel_sign_in(&app);
        }
        gone
    };
    if left(&pane) {
        return Ok(());
    }
    if let Some(w) = app.get_webview(PANE) {
        w.navigate(parsed).map_err(|_| err("navigate"))?;
        place(&w, bounds);
        return Ok(());
    }
    let main = app.get_window("main").ok_or_else(|| err("window"))?;
    let handle = app.clone();
    let nav = app.clone();
    let builder = WebviewBuilder::new(PANE, WebviewUrl::External(parsed))
        .data_directory(state.paths.webview_web.clone())
        .background_color(SURFACE)
        .initialization_script(page_script(&accent))
        // Acts only on Epic's sign-in answer page (auth.md §3).
        .initialization_script(egs_auth::LOGIN_INIT_SCRIPT)
        .zoom_hotkeys_enabled(false)
        .general_autofill_enabled(false)
        .on_navigation(move |u| {
            // Host and path only: queries can carry exchange codes.
            tracing::debug!(
                host = u.host_str().unwrap_or(""),
                path = u.path(),
                "Epic page navigation"
            );
            // Epic's sign-in answer: handed to `auth`, never loaded.
            if let Some(cb) = egs_auth::parse_callback(u.as_str()) {
                crate::auth::finish_sign_in(&nav, cb);
                return false;
            }
            if u.scheme() != "https" {
                return false;
            }
            !(is_login(u) && sign_in_instead(&nav))
        })
        .on_page_load(|webview, payload| on_page_load(webview, payload.event(), payload.url()))
        // Links that want a new window open in the pane itself: never a second window.
        .on_new_window(move |url, _| {
            if url.scheme() == "https" {
                let h = handle.clone();
                tauri::async_runtime::spawn(async move {
                    if let Some(w) = h.get_webview(PANE) {
                        let _ = w.navigate(url);
                    }
                });
            }
            NewWindowResponse::Deny
        });
    pane.loaded.store(false, Ordering::SeqCst);
    let webview = main
        .add_child(
            builder,
            LogicalPosition::new(bounds.x, bounds.y),
            LogicalSize::new(bounds.width.max(1.0), bounds.height.max(1.0)),
        )
        .map_err(|e| {
            tracing::error!(error = %e, "could not create the Epic page pane");
            err("view")
        })?;
    // Hidden until its first page has loaded (`on_page_load`).
    if !pane.loaded.load(Ordering::SeqCst) {
        let _ = webview.hide();
    }
    // The UI left while it was being created.
    if left(&pane) {
        let _ = close_pane(&app);
    }
    Ok(())
}

/// A page opened directly ran into Epic's login page: open it through the sign-in hop instead
/// (our session → exchange code), so the user never types their password here. True if so.
fn sign_in_instead(app: &AppHandle) -> bool {
    let target = app
        .try_state::<PaneState>()
        .and_then(|s| s.direct.lock().ok().and_then(|mut d| d.take()));
    let Some(target) = target else {
        return false;
    };
    tracing::info!("Epic page needs the sign-in hop");
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let url = crate::auth::signed_in_url(&app, &target).await;
        if let (Some(w), Ok(url)) = (app.get_webview(PANE), url.parse::<Url>()) {
            let _ = w.navigate(url);
        }
    });
    true
}

/// Follow the UI's box (window resized, layout changed).
#[tauri::command]
#[specta::specta]
pub fn web_pane_bounds(app: AppHandle, bounds: PaneBounds) {
    if let Some(w) = app.get_webview(PANE) {
        place(&w, bounds);
    }
}

/// Step aside while a menu or dialog is open (the native view draws above the UI).
#[tauri::command]
#[specta::specta]
pub fn web_pane_visible(app: AppHandle, pane: State<'_, PaneState>, visible: bool) {
    pane.covered.store(!visible, Ordering::SeqCst);
    if let Some(w) = app.get_webview(PANE) {
        if visible {
            reveal(&w, &pane);
        } else {
            let _ = w.hide();
        }
    }
}

/// The user left the page (box `id`). Leaving Epic's sign-in page cancels signing in.
#[tauri::command]
#[specta::specta]
pub fn web_pane_close(app: AppHandle, pane: State<'_, PaneState>, id: u32) {
    if pane
        .owner
        .compare_exchange(id, 0, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return; // another box has the view now
    }
    crate::auth::cancel_sign_in(&app);
    if close_pane(&app) {
        // Changes made on Epic's pages (a game got, wallet, redeemed codes) show up after a
        // refresh.
        crate::library::sync_soon(&app);
        tauri::async_runtime::spawn(async move { crate::account::refresh_wallet(&app).await });
    }
}

/// Close the pane, freeing its browser processes. True if it was open.
pub fn close_pane(app: &AppHandle) -> bool {
    if let Some(state) = app.try_state::<PaneState>() {
        state.loaded.store(false, Ordering::SeqCst);
    }
    let Some(w) = app.get_webview(PANE) else {
        return false;
    };
    let _ = w.close();
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accent_must_be_a_plain_hex_colour() {
        assert_eq!(accent("#1a2B3c"), "#1a2B3c");
        for bad in [
            "",
            "#123",
            "red",
            "#12345g",
            "#123456; } body { x",
            "123456",
        ] {
            assert_eq!(accent(bad), DEFAULT_ACCENT, "{bad}");
        }
    }

    #[test]
    fn page_script_inlines_the_stylesheet_as_a_string() {
        let script = page_script("#ff0000");
        assert!(!script.contains("__CSS__") && !script.contains("__ACCENT__"));
        assert!(script.contains(r#"const CSS = "/* Epic's pages"#));
        assert!(script.contains("--nl-accent: #ff0000;"));
    }

    #[test]
    fn page_script_leaves_frames_alone() {
        // WebView2 runs start-up scripts in every frame (payment, human checks).
        assert!(page_script("#000000").contains("window.top !== window"));
    }

    #[test]
    fn targets_come_from_the_ui_and_log_without_ids() {
        let t: PaneTarget = serde_json::from_str(
            r#"{"kind":"store","target":{"kind":"checkout","namespace":"ns1","offerId":"o2"}}"#,
        )
        .expect("checkout");
        assert_eq!(t.name(), "store checkout");
        let t: PaneTarget = serde_json::from_str(
            r#"{"kind":"store","target":{"kind":"search","query":"my words"}}"#,
        )
        .expect("search");
        assert_eq!(t.name(), "store search");
        let t: PaneTarget = serde_json::from_str(r#"{"kind":"signIn"}"#).expect("sign-in");
        assert_eq!(t.name(), "sign-in");
        let t: PaneTarget =
            serde_json::from_str(r#"{"kind":"page","page":"redeem"}"#).expect("page");
        assert_eq!(t.name(), "Redeem");
    }

    #[test]
    fn only_account_pages_skip_the_hop_and_login_is_caught() {
        assert!(try_direct(
            "https://accounts.epicgames.com/account/personal"
        ));
        assert!(!try_direct("https://store.epicgames.com/wallet"));
        assert!(!try_direct("https://accounts.epicgames.com.evil.example/x"));
        let login = |u: &str| is_login(&u.parse().expect("url"));
        assert!(login("https://www.epicgames.com/id/login?redirectUrl=x"));
        assert!(!login("https://www.epicgames.com/id/exchange"));
        assert!(!login("https://accounts.epicgames.com/id/login"));
    }

    #[test]
    fn sign_in_hops_are_recognised() {
        let hop = |u: &str| is_hop(&u.parse().expect("url"));
        assert!(hop("https://www.epicgames.com/id/exchange?exchangeCode=x"));
        assert!(hop("https://accounts.epicgames.com/auth/callback"));
        assert!(!hop("https://accounts.epicgames.com/account/personal"));
        assert!(!hop("https://www.epicgames.com/id/login"));
    }
}
