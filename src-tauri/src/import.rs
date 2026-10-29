//! Use games already installed by the official launcher (docs/protocol/egl-local-files.md).
//!
//! Scanning is read-only. Adopting only *reads* the game folder and EGL's files: it
//! copies the build manifest into our own data folder and records the install path.

use serde::Serialize;
use tauri::{AppHandle, State};
use tauri_specta::Event;

use crate::auth::AuthState;
use crate::commands::{AppState, UiError};
use crate::library::LibraryChanged;

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ImportCandidate {
    pub app_name: String,
    pub title: String,
    pub folder: String,
    pub size: f64,
    pub version: String,
    /// Interrupted update or EGL asked for validation: verify before first launch.
    pub needs_verify: bool,
    /// The game is in this account's library.
    pub owned: bool,
    /// We already manage it (installed here or adopted before).
    pub already_added: bool,
    /// Complete, folder and manifest present, not a content pack.
    pub adoptable: bool,
}

fn scan() -> Result<Vec<egs_import::EglInstall>, UiError> {
    let pd = egs_import::program_data().ok_or_else(|| UiError {
        code: "import".into(),
        message: "Couldn't find the Epic Games Launcher's data folder.".into(),
    })?;
    egs_import::scan(&pd).map_err(|e| UiError {
        code: "import".into(),
        message: match e {
            egs_import::ImportError::NotFound => {
                "The Epic Games Launcher doesn't seem to be installed.".into()
            }
            other => format!("Couldn't read the Epic Games Launcher's install list ({other})."),
        },
    })
}

/// List installs the official launcher knows about. Read-only.
#[tauri::command]
#[specta::specta]
pub async fn egl_import_scan(
    state: State<'_, AppState>,
    auth: State<'_, AuthState>,
) -> Result<Vec<ImportCandidate>, UiError> {
    let installs = scan()?;
    let owner = auth.session.current().await.map(|a| a.account_id);
    let known = match &owner {
        Some(o) => state.cache.known_games(o)?,
        None => Default::default(),
    };
    let snapshot = state.cache.snapshot()?;
    let installed: std::collections::HashSet<_> = snapshot
        .games
        .iter()
        .filter(|g| !matches!(g.state, egs_cache::InstallState::NotInstalled))
        .map(|g| g.app_name.clone())
        .collect();
    tracing::info!(found = installs.len(), "scanned official launcher installs");
    Ok(installs
        .into_iter()
        .filter(|i| !i.is_content_pack)
        .map(|i| ImportCandidate {
            owned: known.contains_key(&i.app_name),
            already_added: installed.contains(&i.app_name),
            adoptable: i.adoptable(),
            needs_verify: i.needs_verify(),
            title: if i.display_name.is_empty() {
                i.app_name.clone()
            } else {
                i.display_name.clone()
            },
            folder: i.install_location.to_string_lossy().into_owned(),
            size: i.install_size as f64,
            version: i.version,
            app_name: i.app_name,
        })
        .collect())
}

/// Use an existing install. The user confirmed in the dialog that names the folder.
#[tauri::command]
#[specta::specta]
pub async fn egl_adopt(
    app: AppHandle,
    state: State<'_, AppState>,
    auth: State<'_, AuthState>,
    app_name: String,
) -> Result<(), UiError> {
    let owner = auth
        .session
        .current()
        .await
        .ok_or_else(|| UiError {
            code: "notSignedIn".into(),
            message: "Sign in first, so we can check you own this game.".into(),
        })?
        .account_id;
    let install = scan()?
        .into_iter()
        .find(|i| i.app_name == app_name && i.adoptable())
        .ok_or_else(|| UiError {
            code: "import".into(),
            message: "That install isn't complete or can't be found anymore.".into(),
        })?;

    // Keep our own copy of the build manifest (read from the game folder, never modified).
    let dest_dir = state.paths.manifests.join(&install.app_name);
    let copy = std::fs::create_dir_all(&dest_dir).and_then(|_| {
        let safe_version: String = install
            .version
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || "+-._".contains(c) {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        std::fs::copy(
            &install.manifest_path,
            dest_dir.join(format!("{safe_version}.manifest")),
        )
    });
    if let Err(e) = copy {
        tracing::warn!(error = %e, "could not copy the build manifest");
        return Err(UiError {
            code: "import".into(),
            message: "Couldn't read the game's manifest. Is the drive connected?".into(),
        });
    }

    let folder = install.install_location.to_string_lossy().into_owned();
    let adopted = state.cache.adopt_install(
        &owner,
        &egs_cache::Adopt {
            app_name: &install.app_name,
            install_path: &folder,
            version: &install.version,
            install_size: install.install_size,
            egl_guid: &install.installation_guid,
            needs_verify: install.needs_verify(),
        },
    )?;
    if !adopted {
        return Err(UiError {
            code: "import".into(),
            message: "This game isn't in your library, or it's already installed here.".into(),
        });
    }
    tracing::info!(
        needs_verify = install.needs_verify(),
        "adopted an official-launcher install"
    );
    let _ = LibraryChanged {
        reason: "import".into(),
    }
    .emit(&app);
    Ok(())
}
