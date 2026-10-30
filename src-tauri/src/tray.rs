//! Tray icon and menu (Phase 9): open, Quick Launch games, the current download with
//! pause/resume, quit. The menu is rebuilt when the pointer reaches the icon, so it's
//! current without listening to every change.

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Runtime};

use crate::commands::AppState;

const TRAY_ID: &str = "main";
const QUICK_LAUNCH_MAX: usize = 8;

/// Show and focus the main window (tray click, "Open", second app start).
pub fn show_window<R: Runtime>(app: &AppHandle<R>) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

fn build_menu(app: &AppHandle) -> tauri::Result<Menu<tauri::Wry>> {
    let menu = Menu::new(app)?;
    menu.append(&MenuItem::with_id(
        app,
        "open",
        "Open Next Launcher",
        true,
        None::<&str>,
    )?)?;

    let snapshot = app
        .try_state::<AppState>()
        .and_then(|s| s.cache.snapshot().ok());
    if let Some(snap) = &snapshot {
        let running: Vec<String> = app
            .try_state::<crate::launch::LaunchState>()
            .map(|l| l.running_names())
            .unwrap_or_default();
        let pinned: Vec<_> = snap
            .quick_launch
            .iter()
            .filter_map(|id| snap.games.iter().find(|g| &g.app_name == id))
            .filter(|g| {
                matches!(
                    g.state,
                    egs_cache::model::InstallState::Installed
                        | egs_cache::model::InstallState::UpdateAvailable
                )
            })
            .take(QUICK_LAUNCH_MAX)
            .collect();
        if !pinned.is_empty() {
            menu.append(&PredefinedMenuItem::separator(app)?)?;
            for g in pinned {
                let playing = running.contains(&g.app_name);
                let label = if playing {
                    format!("{} (playing)", g.title)
                } else {
                    format!("Play {}", g.title)
                };
                menu.append(&MenuItem::with_id(
                    app,
                    format!("play:{}", g.app_name),
                    label,
                    !playing,
                    None::<&str>,
                )?)?;
            }
        }
        if let Some(dl) = &snap.active_download {
            menu.append(&PredefinedMenuItem::separator(app)?)?;
            let pct = if dl.bytes_total > 0.0 {
                (dl.bytes_done / dl.bytes_total * 100.0).floor()
            } else {
                0.0
            };
            let paused = dl.state == "paused" || dl.state == "failed";
            let line = format!(
                "{} — {pct:.0}%{}",
                dl.title,
                if paused { " (paused)" } else { "" }
            );
            menu.append(&MenuItem::with_id(
                app,
                "dl-status",
                line,
                false,
                None::<&str>,
            )?)?;
            let (id, text) = if paused {
                (format!("resume:{}", dl.id as i64), "Resume download")
            } else {
                (format!("pause:{}", dl.id as i64), "Pause download")
            };
            menu.append(&MenuItem::with_id(app, id, text, true, None::<&str>)?)?;
        }
    }
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    menu.append(&MenuItem::with_id(
        app,
        "quit",
        "Quit Next Launcher",
        true,
        None::<&str>,
    )?)?;
    Ok(menu)
}

fn refresh(app: &AppHandle) {
    if let (Some(tray), Ok(menu)) = (app.tray_by_id(TRAY_ID), build_menu(app)) {
        let _ = tray.set_menu(Some(menu));
    }
}

fn on_menu(app: &AppHandle, id: &str) {
    match id {
        "open" => show_window(app),
        "quit" => app.exit(0),
        _ => {
            if let Some(name) = id.strip_prefix("play:") {
                let (app, name) = (app.clone(), name.to_owned());
                tauri::async_runtime::spawn(async move {
                    if let Err(e) = crate::launch::launch(&app, &name, &Default::default()).await {
                        // Questions (prerequisites, saves, offline) need the window.
                        tracing::info!(code = %e.code, "tray launch needs the app");
                        show_window(&app);
                    }
                });
            } else if let Some(n) = id.strip_prefix("pause:").and_then(|n| n.parse().ok()) {
                crate::install::pause_job(app, n);
            } else if let Some(n) = id.strip_prefix("resume:").and_then(|n| n.parse().ok()) {
                crate::install::resume_job(app, n);
            }
        }
    }
}

pub fn setup(app: &AppHandle) -> tauri::Result<()> {
    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip("Next Launcher")
        .menu(&build_menu(app)?)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| on_menu(app, event.id().as_ref()))
        .on_tray_icon_event(|tray, event| match event {
            TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } => show_window(tray.app_handle()),
            TrayIconEvent::Enter { .. } => refresh(tray.app_handle()),
            _ => {}
        });
    if let Some(icon) = app.default_window_icon().cloned() {
        builder = builder.icon(icon);
    }
    builder.build(app)?;
    Ok(())
}
