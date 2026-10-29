//! Account, wallet and redemption (Phase 7). Spec: docs/protocol/account.md,
//! wallet-and-redeem.md. API where the launcher token is permitted; Epic's own page
//! (signed in) otherwise. Nothing here changes the account without the user's click.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use egs_api::account::{self as api_account, AccountUpdate};
use egs_api::oauth::{VerifyResponse, CREATE, READ, UPDATE};
use egs_api::wallet::{self as api_wallet, RedeemError};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};
use tauri_specta::Event;

use crate::auth::AuthState;
use crate::commands::{settings_value, AppState, UiError};

const DETAILS_TTL: Duration = Duration::from_secs(120);
const WALLET_TTL: Duration = Duration::from_secs(300);

/// What the launcher token may do, from `verify?includePerms=true` (account.md).
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Capabilities {
    /// Display name and communication language in-app.
    pub update_account: bool,
    pub connections: bool,
    pub redeem: bool,
    pub wallet: bool,
    /// "Sign out of all devices" through the API (else Epic's page).
    pub sign_out_others: bool,
}

impl Capabilities {
    pub fn from_verify(v: &VerifyResponse, account_id: &str) -> Self {
        Self {
            update_account: v.allows("account:public:account", UPDATE),
            connections: v.allows("account:public:account:externalAuths", READ),
            redeem: v.allows("fulfillment:public:code", CREATE),
            wallet: v.allows(&format!("payment:public:{account_id}:wallet"), READ)
                && v.allows(&format!("reward:public:{account_id}:reward-accounts"), READ),
            sign_out_others: v.allows(
                "account:token:otherSessionsForAccount",
                egs_api::oauth::DELETE,
            ),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Connection {
    /// `psn`, `xbl`, `nintendo`, `steam`, `discord`, ...
    pub kind: String,
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AccountDetails {
    pub display_name: String,
    pub email: Option<String>,
    pub country: Option<String>,
    pub preferred_language: Option<String>,
    pub tfa_enabled: Option<bool>,
    /// Epic allows a display name change now.
    pub can_change_name: bool,
    pub last_name_change: Option<String>,
    /// `None` when the token can't read them (the row then opens Epic's page).
    pub connections: Option<Vec<Connection>>,
    pub capabilities: Capabilities,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct WalletInfo {
    /// Formatted by Epic in the account currency, shown as-is.
    pub balance: Option<String>,
    pub rewards: Option<String>,
    pub coupons: Option<f64>,
    /// The summary came from Epic (else only the web links work).
    pub available: bool,
}

/// Account details or the wallet changed (the UI re-reads them).
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
pub struct AccountChanged {
    pub what: String,
}

#[derive(Default)]
pub struct AccountState {
    pub caps: Mutex<Capabilities>,
    details: Mutex<Option<(Instant, AccountDetails)>>,
    wallet: Mutex<Option<(Instant, WalletInfo)>>,
}

impl AccountState {
    /// Forget everything (sign-out).
    pub fn clear(&self) {
        if let Ok(mut c) = self.caps.lock() {
            *c = Capabilities::default();
        }
        if let Ok(mut d) = self.details.lock() {
            *d = None;
        }
        if let Ok(mut w) = self.wallet.lock() {
            *w = None;
        }
    }
}

fn err(code: &str, message: impl Into<String>) -> UiError {
    UiError {
        code: code.into(),
        message: message.into(),
    }
}

fn caps(state: &AccountState) -> Capabilities {
    state.caps.lock().map(|c| *c).unwrap_or_default()
}

fn notify(app: &AppHandle, what: &str) {
    let _ = AccountChanged { what: what.into() }.emit(app);
}

async fn fetch_details(
    auth: &AuthState,
    account: &AccountState,
) -> Result<AccountDetails, UiError> {
    let acct = auth
        .session
        .current()
        .await
        .ok_or_else(|| err("notSignedIn", "Sign in to see your account."))?;
    let access = auth.session.access_token().await?;
    let api = auth.session.api();
    let c = caps(account);
    let (info, links) = tokio::join!(
        api_account::account_info(api, &access, &acct.account_id),
        async {
            if c.connections {
                api_account::external_auths(api, &access, &acct.account_id)
                    .await
                    .ok()
            } else {
                None
            }
        }
    );
    let info = info?;
    Ok(AccountDetails {
        display_name: info.display_name,
        email: info.email,
        country: info.country,
        preferred_language: info.preferred_language,
        tfa_enabled: info.tfa_enabled,
        can_change_name: info.can_update_display_name.unwrap_or(false),
        last_name_change: info.last_display_name_change,
        connections: links.map(|l| {
            l.into_iter()
                .map(|x| Connection {
                    kind: x.kind,
                    name: x.external_display_name,
                })
                .collect()
        }),
        capabilities: c,
    })
}

#[tauri::command]
#[specta::specta]
pub async fn account_details(
    auth: State<'_, AuthState>,
    account: State<'_, AccountState>,
    force: bool,
) -> Result<AccountDetails, UiError> {
    if !force {
        if let Some((at, d)) = account.details.lock().ok().and_then(|d| d.clone()) {
            if at.elapsed() < DETAILS_TTL {
                return Ok(d);
            }
        }
    }
    let d = fetch_details(&auth, &account).await?;
    if let Ok(mut slot) = account.details.lock() {
        *slot = Some((Instant::now(), d.clone()));
    }
    Ok(d)
}

#[derive(Debug, Clone, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AccountEdit {
    pub display_name: Option<String>,
    pub preferred_language: Option<String>,
}

/// Change the display name or communication language (the UI asks first). Only fields
/// the token may update are sent; everything else stays on Epic's pages.
#[tauri::command]
#[specta::specta]
pub async fn update_account(
    app: AppHandle,
    auth: State<'_, AuthState>,
    account: State<'_, AccountState>,
    state: State<'_, AppState>,
    edit: AccountEdit,
) -> Result<AccountDetails, UiError> {
    if !caps(&account).update_account {
        return Err(err("web", "Change this on Epic's account page."));
    }
    let name = edit
        .display_name
        .as_deref()
        .map(str::trim)
        .filter(|n| !n.is_empty());
    if let Some(n) = name {
        let len = n.chars().count();
        if !(3..=16).contains(&len) {
            return Err(err("invalid", "Display names are 3 to 16 characters."));
        }
    }
    let lang = edit
        .preferred_language
        .as_deref()
        .map(str::trim)
        .filter(|l| !l.is_empty());
    if let Some(l) = lang {
        if l.len() > 10
            || !l
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            return Err(err("invalid", "That isn't a language code."));
        }
    }
    if name.is_none() && lang.is_none() {
        return Err(err("invalid", "Nothing to change."));
    }
    let acct = auth
        .session
        .current()
        .await
        .ok_or_else(|| err("notSignedIn", "Sign in first."))?;
    let access = auth.session.access_token().await?;
    let update = AccountUpdate {
        display_name: name.map(str::to_owned),
        preferred_language: lang.map(str::to_owned),
    };
    let info = api_account::update_account(auth.session.api(), &access, &acct.account_id, &update)
        .await
        .map_err(|e| match e.epic_code() {
            Some(c) if c.contains("display_name") || c.contains("displayName") => err(
                "nameTaken",
                "Epic didn't accept that display name (taken, not allowed, or changed too recently).",
            ),
            _ => UiError::from(e),
        })?;
    tracing::info!(
        display_name = update.display_name.is_some(),
        language = update.preferred_language.is_some(),
        "account updated"
    );
    // Keep the cached profile (top bar, Settings card) in step.
    let _ = state
        .cache
        .upsert_signed_in_account(&egs_cache::AccountUpsert {
            account_id: &acct.account_id,
            display_name: &info.display_name,
            email: info.email.as_deref(),
            country: info.country.as_deref(),
        });
    if let Ok(mut d) = account.details.lock() {
        *d = None;
    }
    crate::library::notify_changed(&app, "account");
    notify(&app, "account");
    fetch_details(&auth, &account).await
}

#[tauri::command]
#[specta::specta]
pub async fn wallet_summary(
    auth: State<'_, AuthState>,
    account: State<'_, AccountState>,
    state: State<'_, AppState>,
    force: bool,
) -> Result<WalletInfo, UiError> {
    load_wallet(&auth, &account, &state, force).await
}

pub async fn load_wallet(
    auth: &AuthState,
    account: &AccountState,
    state: &AppState,
    force: bool,
) -> Result<WalletInfo, UiError> {
    if !force {
        if let Some((at, w)) = account.wallet.lock().ok().and_then(|w| w.clone()) {
            if at.elapsed() < WALLET_TTL {
                return Ok(w);
            }
        }
    }
    if !caps(account).wallet || auth.session.current().await.is_none() {
        return Ok(WalletInfo::default());
    }
    let access = auth.session.access_token().await?;
    let locale = settings_value(&state.cache, "general.language").unwrap_or_else(|| "en-US".into());
    let w = api_wallet::wallet_summary(auth.session.api(), &access, &locale).await?;
    let info = WalletInfo {
        available: true,
        balance: w.balance,
        rewards: w.rewards,
        coupons: w.coupons.map(f64::from),
    };
    if let Ok(mut slot) = account.wallet.lock() {
        *slot = Some((Instant::now(), info.clone()));
    }
    Ok(info)
}

/// After sign-in and when the web window closes: refresh the wallet, tell the UI.
pub async fn refresh_wallet(app: &AppHandle) {
    let (Some(auth), Some(account), Some(state)) = (
        app.try_state::<AuthState>(),
        app.try_state::<AccountState>(),
        app.try_state::<AppState>(),
    ) else {
        return;
    };
    match load_wallet(&auth, &account, &state, true).await {
        Ok(w) => {
            tracing::info!(
                available = w.available,
                balance = w.balance.is_some(),
                rewards = w.rewards.is_some(),
                "wallet loaded"
            );
            notify(app, "wallet");
        }
        Err(e) => tracing::warn!(error = %e.message, "wallet not loaded"),
    }
}

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RedeemResult {
    /// What the code added, e.g. game names.
    pub items: Vec<String>,
}

/// Redeem a code the user typed (sent once, on their click; never logged).
#[tauri::command]
#[specta::specta]
pub async fn redeem_code(
    app: AppHandle,
    auth: State<'_, AuthState>,
    account: State<'_, AccountState>,
    code: String,
) -> Result<RedeemResult, UiError> {
    if !caps(&account).redeem {
        return Err(err("web", "Redeem codes on Epic's page."));
    }
    let acct = auth
        .session
        .current()
        .await
        .ok_or_else(|| err("notSignedIn", "Sign in to redeem a code."))?;
    let access = auth.session.access_token().await?;
    match api_wallet::redeem_code(auth.session.api(), &access, &acct.account_id, &code).await {
        Ok(r) => {
            tracing::info!(items = r.details.len(), "code redeemed");
            crate::library::sync_soon(&app);
            refresh_wallet(&app).await;
            Ok(RedeemResult {
                items: r
                    .details
                    .into_iter()
                    .map(|d| d.entitlement_name)
                    .filter(|n| !n.is_empty())
                    .collect(),
            })
        }
        Err(Ok(e)) => {
            tracing::info!(reason = ?std::mem::discriminant(&e), "code not redeemed");
            Err(err(
                if e == RedeemError::Coupon {
                    "coupon"
                } else {
                    "redeem"
                },
                e.to_string(),
            ))
        }
        Err(Err(e)) => Err(UiError::from(e)),
    }
}

/// Dev tooling (`--dev-action account:`): the Settings data path, facts only (no values).
pub async fn dev_check(app: &AppHandle) -> Result<String, UiError> {
    let (Some(auth), Some(account), Some(state)) = (
        app.try_state::<AuthState>(),
        app.try_state::<AccountState>(),
        app.try_state::<AppState>(),
    ) else {
        return Err(UiError::internal("state"));
    };
    let d = fetch_details(&auth, &account).await?;
    let w = load_wallet(&auth, &account, &state, true).await?;
    Ok(format!(
        "details: email={} country={} language={} tfa={:?} can_change_name={} connections={:?};          wallet: available={} balance={} rewards={} coupons={}; caps={:?}",
        d.email.is_some(),
        d.country.is_some(),
        d.preferred_language.is_some(),
        d.tfa_enabled,
        d.can_change_name,
        d.connections.as_ref().map(|c| c.iter().map(|x| x.kind.clone()).collect::<Vec<_>>()),
        w.available,
        w.balance.is_some(),
        w.rewards.is_some(),
        w.coupons.is_some(),
        d.capabilities,
    ))
}

#[tauri::command]
#[specta::specta]
pub fn account_capabilities(account: State<'_, AccountState>) -> Capabilities {
    caps(&account)
}

#[cfg(test)]
mod tests {
    use super::*;
    use egs_api::oauth::Permission;

    #[test]
    fn capabilities_follow_exact_permissions() {
        let v = VerifyResponse {
            account_id: None,
            display_name: None,
            expires_in: None,
            perms: vec![
                Permission {
                    resource: "account:public:account".into(),
                    action: 6,
                },
                Permission {
                    resource: "account:public:account:externalAuths".into(),
                    action: 7,
                },
                Permission {
                    resource: "fulfillment:public:code".into(),
                    action: 1,
                },
                Permission {
                    resource: "payment:public:abc:wallet".into(),
                    action: 2,
                },
                Permission {
                    resource: "reward:public:abc:reward-accounts".into(),
                    action: 2,
                },
                Permission {
                    resource: "account:token:otherSessionsWithSameSourceForAccount".into(),
                    action: 8,
                },
            ],
        };
        let c = Capabilities::from_verify(&v, "abc");
        assert!(c.update_account && c.connections && c.redeem && c.wallet);
        assert!(
            !c.sign_out_others,
            "same-source sessions only isn't 'all devices'"
        );
        assert!(!Capabilities::from_verify(&v, "other").wallet);
    }
}
