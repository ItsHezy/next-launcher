//! Epic's own account, wallet, help and legal pages inside the main window (Settings), instead
//! of a second window (ADR 0016). A child webview sits over the Settings content area; the UI
//! reports where that area is and hides the view while a menu or dialog is open. It uses the
//! same signed-in web profile as the old web window (exchange-code bridge, auth.md §6).
//!
//! The pages wear the app's look (`webpane.css`) and behave like software, not a browser
//! (`webpane.js`): no site header or footer, no context menu, reload, print or zoom. The view
//! stays hidden while a page loads, so the UI's own loading state shows instead of web pages
//! assembling themselves.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use serde::Deserialize;
use tauri::webview::{Color, NewWindowResponse, PageLoadEvent, WebviewBuilder};
use tauri::{AppHandle, LogicalPosition, LogicalSize, Manager, State, Url, Webview, WebviewUrl};

use crate::auth::EpicPage;
use crate::commands::{AppState, UiError};

const PANE: &str = "epic-pane";
/// The app's `--surface`, so an empty view never flashes white.
const SURFACE: Color = Color(0x2e, 0x30, 0x33, 0xff);
const DEFAULT_ACCENT: &str = "#0a6fd6";
/// Sign-in hops redirect on their own; one still on screen after this is waiting for the user
/// (e.g. Epic's human check) and is shown.
const HOP_GRACE: Duration = Duration::from_millis(1500);

/// The Settings content area, in CSS pixels of the main window.
#[derive(Debug, Clone, Copy, Deserialize, specta::Type)]
pub struct PaneBounds {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Default)]
pub struct PaneState {
    /// False once the UI left the page: a pane that finishes opening after that closes again.
    wanted: AtomicBool,
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

/// Show `page` in the pane (signed in when it needs it), creating the pane on first use.
/// `accent` is the app's accent colour (`#rrggbb`), used by the page's styling.
#[tauri::command]
#[specta::specta]
pub async fn web_pane_open(
    app: AppHandle,
    state: State<'_, AppState>,
    pane: State<'_, PaneState>,
    page: EpicPage,
    bounds: PaneBounds,
    accent: String,
) -> Result<(), UiError> {
    pane.wanted.store(true, Ordering::SeqCst);
    let _one = pane.opening.lock().await;
    tracing::info!(?page, "showing Epic page in the app");
    let direct = !page.needs_sign_in() || try_direct(page.url());
    let url = if direct {
        page.url().to_owned()
    } else {
        crate::auth::signed_in_url(&app, page.url()).await
    };
    if let Ok(mut d) = pane.direct.lock() {
        *d = (direct && page.needs_sign_in()).then(|| page.url().to_owned());
    }
    let parsed: Url = url.parse().map_err(|_| err("address"))?;
    if !pane.wanted.load(Ordering::SeqCst) {
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
        .zoom_hotkeys_enabled(false)
        .general_autofill_enabled(false)
        .on_navigation(move |u| {
            // Host and path only: queries can carry exchange codes.
            tracing::debug!(
                host = u.host_str().unwrap_or(""),
                path = u.path(),
                "Epic page navigation"
            );
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
    if !pane.wanted.load(Ordering::SeqCst) {
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

/// Follow the Settings content area (window resized, layout changed).
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

/// The user left the page.
#[tauri::command]
#[specta::specta]
pub fn web_pane_close(app: AppHandle, pane: State<'_, PaneState>) {
    pane.wanted.store(false, Ordering::SeqCst);
    if close_pane(&app) {
        // Changes made on Epic's pages (wallet, redeemed codes) show up after a refresh.
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
