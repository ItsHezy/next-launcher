//! App updates (Phase 9, ADR 0014): GitHub Releases + `tauri-plugin-updater`. Every update
//! must carry a signature from the owner's update key; the plugin refuses anything else.
//! Checks run in the background at most once a day; installing always needs the user's click.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};
use tauri_plugin_updater::{UpdaterBuilder, UpdaterExt};
use tauri_specta::Event;

use crate::commands::{setting_on, settings_value, AppState, UiError};
use crate::install::DownloadsState;

/// The public key in tauri.conf.json until the owner generates the real one.
const PLACEHOLDER_KEY: &str = "PLACEHOLDER-UNTIL-THE-OWNER-GENERATES-THE-KEY";
const CHECK_EVERY: i64 = 24 * 60 * 60;

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AppUpdate {
    pub current: String,
    /// False until the owner sets an update key (ADR 0014).
    pub configured: bool,
    pub available: bool,
    pub version: Option<String>,
    pub notes: Option<String>,
}

/// A newer Next Launcher is available (background check).
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
pub struct AppUpdateAvailable {
    pub version: String,
}

fn err(code: &str, message: impl Into<String>) -> UiError {
    UiError {
        code: code.into(),
        message: message.into(),
    }
}

/// Development builds (install guard on) may point the updater at a local test release.
fn dev_override() -> Option<(String, String)> {
    let url = std::env::var("NL_UPDATE_URL").ok()?;
    let key = std::env::var("NL_UPDATE_PUBKEY").ok()?;
    Some((url, key))
}

fn configured_key(app: &AppHandle) -> Option<String> {
    let key = app
        .config()
        .plugins
        .0
        .get("updater")?
        .get("pubkey")?
        .as_str()?
        .to_owned();
    (key != PLACEHOLDER_KEY && !key.trim().is_empty()).then_some(key)
}

fn builder(app: &AppHandle) -> Result<Option<UpdaterBuilder>, UiError> {
    let dev = app
        .try_state::<DownloadsState>()
        .is_some_and(|d| d.install_root.is_some());
    let mut b = app.updater_builder();
    match (dev, dev_override()) {
        (true, Some((url, key))) => {
            let url = url
                .parse()
                .map_err(|_| err("updater", "bad NL_UPDATE_URL"))?;
            b = b
                .endpoints(vec![url])
                .map_err(|e| err("updater", e.to_string()))?
                .pubkey(key);
        }
        _ if configured_key(app).is_none() => return Ok(None),
        _ => {}
    }
    Ok(Some(b))
}

fn build(b: UpdaterBuilder) -> Result<tauri_plugin_updater::Updater, UiError> {
    b.build().map_err(|e| err("updater", e.to_string()))
}

#[tauri::command]
#[specta::specta]
pub async fn check_app_update(app: AppHandle) -> Result<AppUpdate, UiError> {
    let current = app.package_info().version.to_string();
    let Some(b) = builder(&app)? else {
        return Ok(AppUpdate {
            current,
            configured: false,
            available: false,
            version: None,
            notes: None,
        });
    };
    let found = build(b)?.check().await.map_err(|e| {
        tracing::warn!(error = %e, "update check failed");
        err("updater", "Couldn't check for updates. Try again later.")
    })?;
    record_check(&app);
    Ok(AppUpdate {
        current,
        configured: true,
        available: found.is_some(),
        version: found.as_ref().map(|f| f.version.clone()),
        notes: found.and_then(|f| f.body),
    })
}

/// Download the update, check its signature, run the installer (which restarts the app).
#[tauri::command]
#[specta::specta]
pub async fn install_app_update(app: AppHandle) -> Result<(), UiError> {
    if app
        .try_state::<crate::launch::LaunchState>()
        .is_some_and(|l| l.any_running())
    {
        return Err(err(
            "busy",
            "Close your game first; the update restarts Next Launcher.",
        ));
    }
    let b = builder(&app)?
        .ok_or_else(|| err("notConfigured", "Updates aren't set up for this build yet."))?;
    // Runs after the download's signature checked out, right before the installer replaces us.
    let handle = app.clone();
    let b = b.on_before_exit(move || {
        tracing::info!("closing for the update");
        crate::launch::on_exit(&handle);
    });
    let update = build(b)?
        .check()
        .await
        .map_err(|e| err("updater", e.to_string()))?
        .ok_or_else(|| err("upToDate", "You already have the latest version."))?;
    tracing::info!(version = %update.version, "installing app update");
    update
        .download_and_install(
            |_, _| {},
            || tracing::info!("update downloaded and verified"),
        )
        .await
        .map_err(|e| {
            tracing::warn!(error = %e, "update failed");
            err(
                "updater",
                "The update couldn't be installed. Nothing was changed.",
            )
        })
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn record_check(app: &AppHandle) {
    if let Some(state) = app.try_state::<AppState>() {
        let _ = state
            .cache
            .set_setting("internal.appUpdateCheckedAt", &serde_json::json!(now()));
    }
}

/// Background check, a minute after start and at most once a day (setting on by default).
pub fn schedule(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(60)).await;
        let Some(state) = app.try_state::<AppState>() else {
            return;
        };
        if !setting_on(&state.cache, "general.checkAppUpdates") {
            return;
        }
        let last = settings_value(&state.cache, "internal.appUpdateCheckedAt")
            .and_then(|v| v.parse::<i64>().ok())
            .unwrap_or(0);
        if now() - last < CHECK_EVERY {
            return;
        }
        match check_app_update(app.clone()).await {
            Ok(u) if u.available => {
                let version = u.version.unwrap_or_default();
                tracing::info!(%version, "app update available");
                let _ = AppUpdateAvailable {
                    version: version.clone(),
                }
                .emit(&app);
                crate::notify::send(
                    &app,
                    crate::notify::Kind::AppUpdate,
                    &format!("Next Launcher {version} is available"),
                    "Settings → About installs it.",
                );
            }
            Ok(_) => {}
            Err(e) => tracing::info!(error = %e.message, "background update check skipped"),
        }
    });
}

/// Dev tooling (`--dev-action app-update:<status|check|download>`): the About button's check,
/// a plain check, or a check plus download with the signature verified. Never installs.
pub async fn dev_action(app: &AppHandle, what: &str) -> Result<String, UiError> {
    if what == "status" {
        let u = check_app_update(app.clone()).await?;
        return Ok(format!(
            "current {}, configured {}, available {:?}",
            u.current, u.configured, u.version
        ));
    }
    let b = builder(app)?.ok_or_else(|| err("notConfigured", "not configured"))?;
    let found = build(b)?
        .check()
        .await
        .map_err(|e| err("updater", e.to_string()))?;
    let Some(update) = found else {
        return Ok("up to date".into());
    };
    if what != "download" {
        return Ok(format!("{} available", update.version));
    }
    let bytes = update
        .download(|_, _| {}, || {})
        .await
        .map_err(|e| err("updater", e.to_string()))?;
    Ok(format!(
        "{} downloaded, {} bytes, signature verified",
        update.version,
        bytes.len()
    ))
}
