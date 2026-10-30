//! Sign-in, session restore, silent refresh, sign-out and signed-in Epic web pages.
//! Protocol: docs/protocol/auth.md. Session logic lives in `egs-auth`; this module owns app
//! state and UI events. Epic's pages, its sign-in page included, show inside the main window
//! (`webpane`).

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use egs_api::{Client, Secret};
use egs_auth::{AccountSummary, AuthError, Callback, Pkce, Session};
use egs_cache::AccountUpsert;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};
use tauri_specta::Event;

use crate::commands::{AppState, UiError};

/// What the UI shows about the account.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(
    tag = "state",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum AuthStatus {
    /// `reason` is shown when the user didn't choose to sign out (e.g. session ended).
    SignedOut { reason: Option<String> },
    /// Restoring the saved session in the background (cached library stays visible).
    Restoring { display_name: String },
    /// Epic's sign-in page is open (Settings › Account).
    SigningIn,
    SignedIn {
        account_id: String,
        display_name: String,
        email: Option<String>,
        country: Option<String>,
    },
    /// Saved session kept, but Epic can't be reached right now.
    Offline {
        display_name: String,
        retry_in_secs: u32,
    },
    /// Epic wants the user to finish something on its website first.
    NeedsAction {
        display_name: String,
        action: String,
    },
}

/// Emitted whenever [`AuthStatus`] changes.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, tauri_specta::Event)]
pub struct AuthChanged(pub AuthStatus);

pub struct AuthState {
    pub session: Arc<Session>,
    status: Mutex<AuthStatus>,
    verifier: Mutex<Option<Secret>>,
    corrective_url: Mutex<Option<String>>,
    login_active: AtomicBool,
    restore_started: AtomicBool,
    /// Bumped on sign-out so background loops from the old session stop.
    epoch: AtomicU64,
    use_pkce: bool,
}

impl AuthState {
    pub fn new(api: Client, initial: AuthStatus) -> Self {
        Self {
            session: Arc::new(Session::new(api, Arc::new(egs_auth::KeyringStore))),
            status: Mutex::new(initial),
            verifier: Mutex::new(None),
            corrective_url: Mutex::new(None),
            login_active: AtomicBool::new(false),
            restore_started: AtomicBool::new(false),
            epoch: AtomicU64::new(0),
            // Escape hatch while PKCE support is being verified against Epic (Phase 2).
            use_pkce: std::env::var_os("NL_NO_PKCE").is_none(),
        }
    }
}

impl AuthState {
    pub fn status_snapshot(&self) -> AuthStatus {
        self.status
            .lock()
            .map(|s| s.clone())
            .unwrap_or(AuthStatus::SignedOut { reason: None })
    }
}

fn set_status(app: &AppHandle, status: AuthStatus) {
    if let Some(auth) = app.try_state::<AuthState>() {
        if let Ok(mut s) = auth.status.lock() {
            *s = status.clone();
        }
    }
    if let Err(e) = AuthChanged(status).emit(app) {
        tracing::warn!(error = %e, "could not emit auth status");
    }
}

fn friendly(e: &AuthError) -> String {
    match e {
        AuthError::NotSignedIn => "You're signed out.".into(),
        AuthError::SessionExpired => {
            "Your Epic session ended. Sign in again to sync your library.".into()
        }
        AuthError::CorrectiveAction { .. } => {
            "Epic needs you to confirm something on its website.".into()
        }
        AuthError::LoginFailed(_) => "Sign-in didn't finish. Please try again.".into(),
        AuthError::Api(api) if api.is_transient() => {
            "Couldn't reach Epic's servers. Check your connection and try again.".into()
        }
        AuthError::Api(_) => "Epic didn't accept the sign-in. Please try again.".into(),
        AuthError::Store(_) => {
            "Couldn't use the Windows credential store. Details are in the log.".into()
        }
    }
}

impl From<AuthError> for UiError {
    fn from(e: AuthError) -> Self {
        tracing::warn!(error = %e, "auth error");
        Self {
            code: match &e {
                AuthError::NotSignedIn => "notSignedIn",
                AuthError::SessionExpired => "sessionExpired",
                AuthError::CorrectiveAction { .. } => "needsAction",
                AuthError::Api(a) if a.is_transient() => "offline",
                _ => "auth",
            }
            .into(),
            message: friendly(&e),
        }
    }
}

fn cached_display_name(app: &AppHandle) -> String {
    app.try_state::<AppState>()
        .and_then(|s| s.cache.snapshot().ok())
        .and_then(|s| s.account)
        .map(|a| a.display_name)
        .unwrap_or_default()
}

// ---- restore on start --------------------------------------------------------

/// Called once after the window is shown. Never blocks first paint.
pub fn start_restore(app: &AppHandle) {
    let Some(auth) = app.try_state::<AuthState>() else {
        return;
    };
    if auth.restore_started.swap(true, Ordering::SeqCst) {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move { restore(app).await });
}

async fn restore(app: AppHandle) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let Some(auth) = app.try_state::<AuthState>() else {
        return;
    };
    let account_id = match state.cache.signed_in_account() {
        Ok(Some(id)) => id,
        Ok(None) => {
            set_status(&app, AuthStatus::SignedOut { reason: None });
            return;
        }
        Err(e) => {
            tracing::error!(error = %e, "could not read the cached account");
            set_status(&app, AuthStatus::SignedOut { reason: None });
            return;
        }
    };
    let name = cached_display_name(&app);
    let epoch = auth.epoch.load(Ordering::SeqCst);
    set_status(
        &app,
        AuthStatus::Restoring {
            display_name: name.clone(),
        },
    );

    let mut delay = 10u32;
    loop {
        if auth.epoch.load(Ordering::SeqCst) != epoch {
            return;
        }
        match auth.session.resume(&account_id).await {
            Ok(acct) => {
                on_signed_in(&app, acct).await;
                return;
            }
            Err(e) if e.is_transient() => {
                tracing::info!(
                    retry_in = delay,
                    "Epic unreachable; keeping the saved session"
                );
                set_status(
                    &app,
                    AuthStatus::Offline {
                        display_name: name.clone(),
                        retry_in_secs: delay,
                    },
                );
                tokio::time::sleep(Duration::from_secs(u64::from(delay))).await;
                delay = (delay * 2).min(300);
            }
            Err(AuthError::CorrectiveAction { action, url }) => {
                if let Ok(mut u) = auth.corrective_url.lock() {
                    *u = Some(url);
                }
                set_status(
                    &app,
                    AuthStatus::NeedsAction {
                        display_name: name.clone(),
                        action,
                    },
                );
                return;
            }
            Err(e) => {
                tracing::info!(error = %e, "saved session can't be restored");
                let _ = state.cache.mark_signed_out(&account_id);
                let reason = match e {
                    AuthError::NotSignedIn => None,
                    other => Some(friendly(&other)),
                };
                set_status(&app, AuthStatus::SignedOut { reason });
                return;
            }
        }
    }
}

/// Persist the account, publish the status, then load details in the background.
async fn on_signed_in(app: &AppHandle, acct: AccountSummary) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let Some(auth) = app.try_state::<AuthState>() else {
        return;
    };
    if let Err(e) = state.cache.upsert_signed_in_account(&AccountUpsert {
        account_id: &acct.account_id,
        display_name: &acct.display_name,
        email: None,
        country: None,
    }) {
        tracing::error!(error = %e, "could not cache the account");
    }
    set_status(
        app,
        AuthStatus::SignedIn {
            account_id: acct.account_id.clone(),
            display_name: acct.display_name.clone(),
            email: None,
            country: None,
        },
    );

    let epoch = auth.epoch.load(Ordering::SeqCst);
    let app2 = app.clone();
    tauri::async_runtime::spawn(async move { after_sign_in(app2, acct, epoch).await });
}

async fn after_sign_in(app: AppHandle, acct: AccountSummary, epoch: u64) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let Some(auth) = app.try_state::<AuthState>() else {
        return;
    };
    let session = auth.session.clone();
    let Ok(access) = session.access_token().await else {
        return;
    };
    let api = session.api().clone();

    // Account details (email, country) for Settings → Account.
    match egs_api::account::account_info(&api, &access, &acct.account_id).await {
        Ok(info) => {
            let _ = state.cache.upsert_signed_in_account(&AccountUpsert {
                account_id: &info.id,
                display_name: &info.display_name,
                email: info.email.as_deref(),
                country: info.country.as_deref(),
            });
            if auth.epoch.load(Ordering::SeqCst) == epoch {
                set_status(
                    &app,
                    AuthStatus::SignedIn {
                        account_id: info.id,
                        display_name: info.display_name,
                        email: info.email,
                        country: info.country,
                    },
                );
            }
        }
        Err(e) => tracing::warn!(error = %e, "could not load account details"),
    }

    // Library sync uses the account's country for catalog data, so it starts now.
    if auth.epoch.load(Ordering::SeqCst) == epoch {
        // Playtime kept from offline sessions goes first, so the sync sees Epic's new totals.
        crate::launch::flush_playtime(&app).await;
        crate::library::sync_soon(&app);
        // This week's free games, if they're new to the user (Settings → Notifications).
        let notify_app = app.clone();
        tauri::async_runtime::spawn(async move { crate::notify::free_games(&notify_app).await });
        // Downloads interrupted by closing the app continue (they need the session).
        crate::install::kick(&app);
    }

    // What this token may do (Phase 7 rows use the API only where Epic allows it).
    match egs_api::oauth::verify(&api, &access, true).await {
        Ok(v) => {
            let caps = crate::account::Capabilities::from_verify(&v, &acct.account_id);
            if let Some(a) = app.try_state::<crate::account::AccountState>() {
                if let Ok(mut c) = a.caps.lock() {
                    *c = caps;
                }
            }
            tracing::info!(
                perms = v.perms.len(),
                update_account = caps.update_account,
                connections = caps.connections,
                redeem = caps.redeem,
                wallet = caps.wallet,
                sign_out_others = caps.sign_out_others,
                "launcher token permissions"
            );
            crate::account::refresh_wallet(&app).await;
        }
        Err(e) => tracing::warn!(error = %e, "verify failed"),
    }

    // Epic's Terms of Service (informational in Phase 2; see docs/protocol/launch.md).
    match egs_api::account::eula_status(&api, &access, "epicgames_tos", &acct.account_id, "en")
        .await
    {
        Ok(None) => tracing::info!("no pending Epic Terms of Service"),
        Ok(Some(a)) => tracing::warn!(
            version = a.version,
            "Epic Terms of Service acceptance pending"
        ),
        Err(e) => tracing::warn!(error = %e, "EULA check failed"),
    }

    refresh_loop(app, epoch).await;
}

/// Refresh the access token shortly before it expires. Sleeps otherwise (no polling).
async fn refresh_loop(app: AppHandle, epoch: u64) {
    loop {
        let Some(auth) = app.try_state::<AuthState>() else {
            return;
        };
        if auth.epoch.load(Ordering::SeqCst) != epoch {
            return;
        }
        let Some(due) = auth.session.refresh_due_in().await else {
            return;
        };
        tokio::time::sleep(due.max(Duration::from_secs(5))).await;
        if auth.epoch.load(Ordering::SeqCst) != epoch {
            return;
        }
        match auth.session.access_token().await {
            Ok(_) => tracing::debug!("access token refreshed"),
            Err(e) if e.is_transient() => tokio::time::sleep(Duration::from_secs(30)).await,
            Err(e) => {
                tracing::info!(error = %e, "session ended during refresh");
                if let Some(state) = app.try_state::<AppState>() {
                    if let Ok(Some(id)) = state.cache.signed_in_account() {
                        let _ = state.cache.mark_signed_out(&id);
                    }
                }
                set_status(
                    &app,
                    AuthStatus::SignedOut {
                        reason: Some(friendly(&e)),
                    },
                );
                return;
            }
        }
    }
}

// ---- commands ----------------------------------------------------------------

#[tauri::command]
#[specta::specta]
pub fn auth_status(auth: State<'_, AuthState>) -> AuthStatus {
    auth.status
        .lock()
        .map(|s| s.clone())
        .unwrap_or(AuthStatus::SignedOut { reason: None })
}

/// Start signing in: the address of Epic's sign-in page, shown in the app (`webpane`), which
/// hands its answer to [`finish_sign_in`]. Completion is reported through [`AuthChanged`].
pub(crate) fn begin_sign_in(app: &AppHandle) -> Result<String, UiError> {
    let auth = app
        .try_state::<AuthState>()
        .ok_or_else(|| UiError::internal("auth"))?;
    let pkce = if auth.use_pkce {
        Some(Pkce::generate().map_err(|e| UiError {
            code: "auth".into(),
            message: format!("Couldn't start sign-in ({e})."),
        })?)
    } else {
        None
    };
    let url = egs_auth::login_url(pkce.as_ref().map(|p| p.challenge.as_str()));
    if let Ok(mut v) = auth.verifier.lock() {
        *v = pkce.map(|p| p.verifier);
    }
    auth.login_active.store(true, Ordering::SeqCst);
    set_status(app, AuthStatus::SigningIn);
    Ok(url)
}

/// Epic's sign-in page answered. Only acted on while signing in.
pub(crate) fn finish_sign_in(app: &AppHandle, cb: Callback) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move { complete_login(app, cb).await });
}

/// The sign-in page was left before it finished.
pub(crate) fn cancel_sign_in(app: &AppHandle) {
    let Some(auth) = app.try_state::<AuthState>() else {
        return;
    };
    if auth.login_active.swap(false, Ordering::SeqCst) {
        let previous = auth.status.lock().map(|s| s.clone()).ok();
        if matches!(previous, Some(AuthStatus::SigningIn)) {
            set_status(app, AuthStatus::SignedOut { reason: None });
        }
    }
}

async fn complete_login(app: AppHandle, cb: Callback) {
    // Epic's answer page is next in the view: never shown (it holds the code).
    let _ = crate::webpane::close_pane(&app);
    let Some(auth) = app.try_state::<AuthState>() else {
        return;
    };
    if !auth.login_active.swap(false, Ordering::SeqCst) {
        return; // already handled, or not signing in
    }
    let verifier = auth.verifier.lock().ok().and_then(|mut v| v.take());
    let used_pkce = verifier.is_some();
    let result = auth.session.sign_in(cb, verifier).await;
    match result {
        Ok(acct) => on_signed_in(&app, acct).await,
        Err(AuthError::CorrectiveAction { action, url }) => {
            if let Ok(mut u) = auth.corrective_url.lock() {
                *u = Some(url);
            }
            set_status(
                &app,
                AuthStatus::NeedsAction {
                    display_name: String::new(),
                    action,
                },
            );
        }
        Err(e) => {
            tracing::warn!(error = %e, used_pkce, "sign-in failed");
            set_status(
                &app,
                AuthStatus::SignedOut {
                    reason: Some(friendly(&e)),
                },
            );
        }
    }
}

#[tauri::command]
#[specta::specta]
pub async fn sign_out(
    app: AppHandle,
    auth: State<'_, AuthState>,
    state: State<'_, AppState>,
) -> Result<(), UiError> {
    auth.epoch.fetch_add(1, Ordering::SeqCst);
    let cached = state.cache.signed_in_account().ok().flatten();
    auth.session.sign_out().await?;
    if let Some(id) = cached {
        auth.session.forget(&id)?;
        state.cache.mark_signed_out(&id)?;
    }
    if let Some(a) = app.try_state::<crate::account::AccountState>() {
        a.clear();
    }
    let _ = crate::webpane::close_pane(&app);
    // Forget Epic's web cookies too (the next sign-in starts clean).
    let dir = state.paths.webview_web.clone();
    std::thread::spawn(move || {
        for _ in 0..20 {
            std::thread::sleep(Duration::from_millis(300));
            match std::fs::remove_dir_all(&dir) {
                Ok(()) => return,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => return,
                Err(_) => continue, // WebView2 still shutting down
            }
        }
        tracing::warn!("could not clear the Epic web profile");
    });
    set_status(&app, AuthStatus::SignedOut { reason: None });
    Ok(())
}

/// Epic pages that open signed in (docs/protocol/account.md, wallet-and-redeem.md).
#[derive(Debug, Clone, Copy, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum EpicPage {
    AccountPersonal,
    Password,
    Connections,
    EmailPreferences,
    Transactions,
    PaymentMethods,
    Wallet,
    Redeem,
    Gifts,
    Rewards,
    VbucksCard,
    Support,
    Terms,
    PrivacyPolicy,
    RefundPolicy,
    PublisherIndex,
    Eula,
}

impl EpicPage {
    pub(crate) fn url(self) -> &'static str {
        match self {
            Self::AccountPersonal => "https://accounts.epicgames.com/account/personal",
            Self::Password => "https://accounts.epicgames.com/account/password",
            Self::Connections => "https://accounts.epicgames.com/account/connections",
            Self::EmailPreferences => "https://accounts.epicgames.com/account/messaging",
            Self::Transactions => "https://accounts.epicgames.com/account/transactions",
            Self::PaymentMethods => "https://accounts.epicgames.com/account/payments",
            Self::Wallet => "https://store.epicgames.com/wallet",
            Self::Redeem => "https://accounts.epicgames.com/account/code-redemption",
            Self::Gifts => "https://accounts.epicgames.com/account/transactions/gifts",
            Self::Rewards => "https://accounts.epicgames.com/account/rewards",
            Self::VbucksCard => "https://www.fortnite.com/vbuckscard",
            Self::Support => "https://www.epicgames.com/help/en-US/",
            Self::Terms => "https://legal.epicgames.com/epicgames/tos",
            Self::PrivacyPolicy => "https://legal.epicgames.com/epicgames/privacy-policy",
            Self::RefundPolicy => "https://legal.epicgames.com/store/refund-policy",
            Self::PublisherIndex => "https://store.epicgames.com/publisher-index",
            Self::Eula => "https://legal.epicgames.com/store/eula",
        }
    }

    /// Public pages open as they are; the rest go through the exchange-code bridge.
    pub(crate) fn needs_sign_in(self) -> bool {
        !matches!(
            self,
            Self::Support
                | Self::Terms
                | Self::PrivacyPolicy
                | Self::RefundPolicy
                | Self::PublisherIndex
                | Self::Eula
        )
    }
}

pub(crate) fn encode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// The address that opens `url` signed in through an exchange code when we have a session
/// (else `url` itself, as a guest; Epic's page offers its own sign-in).
pub(crate) async fn signed_in_url(app: &AppHandle, url: &str) -> String {
    let Some(auth) = app.try_state::<AuthState>() else {
        return url.to_owned();
    };
    match auth.session.exchange_code().await {
        Ok(code) => format!(
            "https://www.epicgames.com/id/exchange?exchangeCode={}&redirectUrl={}",
            code.expose(),
            encode(url)
        ),
        Err(e) => {
            tracing::info!(error = %e, "opening Epic page without signing in");
            url.to_owned()
        }
    }
}

/// The page Epic asked for in a corrective action (e.g. accept a new policy), shown in the app.
pub(crate) fn corrective_url(app: &AppHandle) -> Result<String, UiError> {
    app.try_state::<AuthState>()
        .and_then(|a| a.corrective_url.lock().ok().and_then(|u| u.clone()))
        .ok_or(UiError {
            code: "auth".into(),
            message: "There's nothing Epic needs right now.".into(),
        })
}
