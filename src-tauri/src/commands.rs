//! Tauri commands. Every command is typed end to end via tauri-specta.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use egs_cache::{Cache, LibrarySnapshot};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State, WebviewWindow};

use crate::paths::Paths;
use crate::startup::{self, FrontendMarks, Mark};

pub struct AppState {
    pub cache: Arc<Cache>,
    pub paths: Paths,
    pub bench: bool,
    /// Benchmarks and screenshot runs: no session restore, no network.
    pub no_network: bool,
    /// Dev tooling (`--dev-probe-sizes`).
    pub dev_probe_sizes: bool,
    /// Dev tooling (`--dev-action <action>:<app>`).
    pub dev_action: Option<String>,
    pub shown: AtomicBool,
    /// Started at sign-in with "Start minimized": stay in the tray.
    pub start_hidden: bool,
}

/// Error shape the UI receives. `message` is written for people; details go to the log.
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct UiError {
    pub code: String,
    pub message: String,
}

impl UiError {
    pub fn internal(what: &str) -> Self {
        tracing::error!(what, "internal error");
        Self {
            code: "internal".into(),
            message: "Something went wrong inside Next Launcher. Details are in the log.".into(),
        }
    }
}

impl From<egs_api::ApiError> for UiError {
    fn from(e: egs_api::ApiError) -> Self {
        tracing::warn!(error = %e, "Epic API error");
        let (code, message) = if matches!(e, egs_api::ApiError::Challenge) {
            (
                "challenge",
                "Epic's store wants to check you're human. Opening it on Epic's site instead.",
            )
        } else if e.is_transient() {
            ("offline", "Couldn't reach Epic's servers. Retrying later.")
        } else if e.is_auth_invalid() {
            ("sessionExpired", "Your Epic session ended. Sign in again.")
        } else {
            (
                "epic",
                "Epic returned an unexpected answer. Details are in the log.",
            )
        };
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
}

impl From<egs_cache::Error> for UiError {
    fn from(e: egs_cache::Error) -> Self {
        tracing::error!(error = %e, "cache error");
        Self {
            code: "cache".into(),
            message: "Couldn't read the local library cache. Try restarting Next Launcher.".into(),
        }
    }
}

/// Show the main window once (first paint or safety timeout).
pub fn show_main(app: &AppHandle, reason: &str) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    if state.shown.swap(true, Ordering::SeqCst) {
        return;
    }
    if state.start_hidden {
        startup::mark(&format!("window_kept_in_tray ({reason})"));
        return;
    }
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.show();
        let _ = win.set_focus();
    }
    startup::mark(&format!("window_shown ({reason})"));
}

/// An on/off setting, with the same default the UI shows (ui/src/lib/state/settings.svelte.ts).
pub fn setting_on(cache: &Cache, key: &str) -> bool {
    let default = matches!(
        key,
        "general.minimizeToTray"
            | "general.checkAppUpdates"
            | "general.showUninstalled"
            | "downloads.pauseWhilePlaying"
            | "downloads.autoUpdate"
            | "notifications.freeGames"
            | "notifications.downloadFinished"
            | "notifications.sound"
            | "notifications.dndWhilePlaying"
            | "cloud.enabled"
            | "cloud.syncOnStart"
            | "cloud.syncOnExit"
    );
    settings_value(cache, key)
        .map(|v| v == "true")
        .unwrap_or(default)
}

/// "Minimize to tray on close" (on by default, like the official launcher).
pub fn close_to_tray<R: tauri::Runtime>(app: &AppHandle<R>) -> bool {
    app.try_state::<AppState>()
        .is_some_and(|s| setting_on(&s.cache, "general.minimizeToTray"))
}

/// Called by the UI after its first frame is painted. Shows the window.
#[tauri::command]
#[specta::specta]
pub fn app_ready(app: AppHandle, state: State<'_, AppState>, report: FrontendMarks) {
    startup::add_frontend(&report);
    show_main(&app, "first paint");
    // Network work starts only after the window is visible (ADR 0006).
    if !state.no_network {
        crate::auth::start_restore(&app);
        crate::library::kick_images(&app);
        crate::updater::schedule(&app);
    }
    let marks = startup::snapshot();
    tracing::info!("{}", startup::render(&marks));
    let _ = startup::write_json(&state.paths.logs.join("startup.json"), &marks);

    if state.bench {
        println!("{}", startup::render(&marks));
        // Let the visible frame settle, then exit so the bench script can loop.
        let app = app.clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(400));
            app.exit(0);
        });
    }
}

/// Benchmark results from the UI (`--bench-scroll`): written to the logs folder, then exit.
#[tauri::command]
#[specta::specta]
pub fn bench_report(app: AppHandle, state: State<'_, AppState>, name: String, data: String) {
    let file = match name.as_str() {
        "bench-scroll" => "bench-scroll.json",
        _ => return,
    };
    println!("{data}");
    if let Err(e) = std::fs::write(state.paths.logs.join(file), &data) {
        tracing::warn!(error = %e, "could not write bench report");
    }
    app.exit(0);
}

#[tauri::command]
#[specta::specta]
pub fn startup_timeline() -> Vec<Mark> {
    startup::snapshot()
}

#[tauri::command]
#[specta::specta]
pub fn get_snapshot(state: State<'_, AppState>) -> Result<LibrarySnapshot, UiError> {
    Ok(state.cache.snapshot()?)
}

/// A stored setting. Settings are flat `section.key` → scalar.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(untagged)]
pub enum SettingValue {
    Bool(bool),
    Number(f64),
    Text(String),
}

impl SettingValue {
    pub fn from_json(v: &serde_json::Value) -> Option<Self> {
        match v {
            serde_json::Value::Bool(b) => Some(Self::Bool(*b)),
            serde_json::Value::Number(n) => n.as_f64().map(Self::Number),
            serde_json::Value::String(s) => Some(Self::Text(s.clone())),
            _ => None,
        }
    }

    pub fn to_json(&self) -> serde_json::Value {
        match self {
            Self::Bool(b) => serde_json::Value::Bool(*b),
            Self::Number(n) => serde_json::json!(n),
            Self::Text(s) => serde_json::Value::String(s.clone()),
        }
    }
}

/// One stored setting as text (numbers formatted), if set.
pub fn settings_value(cache: &Cache, key: &str) -> Option<String> {
    cache.settings().ok()?.get(key).map(|v| match v {
        serde_json::Value::String(s) => s.clone(),
        other => other.to_string(),
    })
}

pub fn settings_map(cache: &Cache) -> Result<BTreeMap<String, SettingValue>, egs_cache::Error> {
    Ok(cache
        .settings()?
        .iter()
        .filter_map(|(k, v)| SettingValue::from_json(v).map(|v| (k.clone(), v)))
        .collect())
}

#[tauri::command]
#[specta::specta]
pub fn get_settings(state: State<'_, AppState>) -> Result<BTreeMap<String, SettingValue>, UiError> {
    Ok(settings_map(&state.cache)?)
}

#[tauri::command]
#[specta::specta]
pub fn set_setting(
    state: State<'_, AppState>,
    key: String,
    value: SettingValue,
    app: AppHandle,
) -> Result<(), UiError> {
    state.cache.set_setting(&key, &value.to_json())?;
    match key.as_str() {
        "downloads.limitMbps" => crate::install::apply_speed_limit(&app),
        "general.runAtStartup" => {
            crate::autostart::apply(&app);
        }
        // Read before logging starts, so it lives in a marker file (applies on restart).
        "support.debugLogging" => {
            let marker = state.paths.debug_marker();
            let _ = match value.to_json() {
                serde_json::Value::Bool(true) => std::fs::write(&marker, b"on"),
                _ => std::fs::remove_file(&marker).or(Ok(())),
            };
        }
        _ => {}
    }
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn set_pinned(
    state: State<'_, AppState>,
    app_name: String,
    pinned: bool,
) -> Result<(), UiError> {
    Ok(state.cache.set_pinned(&app_name, pinned)?)
}

#[derive(Debug, Clone, Copy, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum WindowAction {
    Minimize,
    ToggleMaximize,
    Close,
}

#[tauri::command]
#[specta::specta]
pub fn window_action(window: WebviewWindow, action: WindowAction) {
    let result = match action {
        WindowAction::Minimize => window.minimize(),
        WindowAction::ToggleMaximize => match window.is_maximized() {
            Ok(true) => window.unmaximize(),
            _ => window.maximize(),
        },
        // Phase 9: honour "minimize to tray on close".
        WindowAction::Close if close_to_tray(window.app_handle()) => window.hide(),
        WindowAction::Close => window.close(),
    };
    if let Err(e) = result {
        tracing::warn!(error = %e, ?action, "window action failed");
    }
}
