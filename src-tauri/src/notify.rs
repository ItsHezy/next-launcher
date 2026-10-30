//! Windows notifications (Phase 9): download finished, this week's free games.
//! Respects Settings → Notifications (per kind, sound, do-not-disturb while playing).

use tauri::{AppHandle, Manager};
use tauri_plugin_notification::NotificationExt;

use crate::commands::{setting_on, AppState};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    DownloadFinished,
    FreeGames,
    AppUpdate,
}

impl Kind {
    fn key(self) -> &'static str {
        match self {
            Self::DownloadFinished => "notifications.downloadFinished",
            Self::FreeGames => "notifications.freeGames",
            Self::AppUpdate => "general.checkAppUpdates",
        }
    }
}

/// Show a notification if the user wants this kind now. Returns whether it was shown.
pub fn send(app: &AppHandle, kind: Kind, title: &str, body: &str) -> bool {
    let Some(state) = app.try_state::<AppState>() else {
        return false;
    };
    if !setting_on(&state.cache, kind.key()) {
        return false;
    }
    let playing = app
        .try_state::<crate::launch::LaunchState>()
        .is_some_and(|l| l.any_running());
    if playing && setting_on(&state.cache, "notifications.dndWhilePlaying") {
        tracing::info!(?kind, "notification held back while playing");
        return false;
    }
    let mut n = app.notification().builder().title(title).body(body);
    if setting_on(&state.cache, "notifications.sound") {
        n = n.sound("Default"); // no sound name = a silent toast
    }
    match n.show() {
        Ok(()) => {
            tracing::info!(?kind, "notification shown");
            true
        }
        Err(e) => {
            tracing::warn!(error = %e, "notification failed");
            false
        }
    }
}

/// After sign-in: tell the user about free games that are new since we last told them.
pub async fn free_games(app: &AppHandle) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    if !setting_on(&state.cache, "notifications.freeGames") {
        return;
    }
    let Ok(now) = crate::store::free_now(app).await else {
        return;
    };
    let ids: Vec<String> = now.iter().map(|(id, _)| id.clone()).collect();
    let key = "internal.notified.freeGames";
    let seen: Vec<String> = crate::commands::settings_value(&state.cache, key)
        .and_then(|v| serde_json::from_str(&v).ok())
        .unwrap_or_default();
    let new: Vec<&str> = now
        .iter()
        .filter(|(id, _)| !seen.contains(id))
        .map(|(_, title)| title.as_str())
        .collect();
    if !new.is_empty() {
        send(
            app,
            Kind::FreeGames,
            "Free this week on Epic",
            &new.join(", "),
        );
    }
    if let Ok(json) = serde_json::to_string(&ids) {
        let _ = state
            .cache
            .set_setting(key, &serde_json::Value::String(json));
    }
}
