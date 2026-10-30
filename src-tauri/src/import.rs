//! Use games already installed by the official launcher (docs/protocol/egl-local-files.md).
//!
//! Scanning is read-only. Adopting only *reads* the game folder and EGL's files: it
//! copies the build manifest into our own data folder and records the install path.
//! After every library sync, owned and complete installs are adopted automatically
//! (setting "downloads.autoImport", ADR 0015); the dialog shows the same list by hand.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};
use tauri_specta::Event;

use crate::auth::AuthState;
use crate::commands::{setting_on, AppState, UiError};
use crate::library::LibraryChanged;

/// EGL installs (by `InstallationGuid`) the user uninstalled here: never adopted again
/// automatically, even though EGL's records and `.egstore` outlive our uninstall.
const REMOVED_KEY: &str = "internal.import.removed";

/// Games from the official launcher were added to the library (automatic import).
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
pub struct GamesImported {
    pub titles: Vec<String>,
}

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
            title: title(&i),
            folder: i.install_location.to_string_lossy().into_owned(),
            size: i.install_size as f64,
            version: i.version,
            app_name: i.app_name,
        })
        .collect())
}

fn title(i: &egs_import::EglInstall) -> String {
    if i.display_name.is_empty() {
        i.app_name.clone()
    } else {
        i.display_name.clone()
    }
}

/// Copy the build manifest into our store and record the install. `Ok(false)`: not in this
/// account's library, or already installed here.
fn adopt(state: &AppState, owner: &str, install: &egs_import::EglInstall) -> Result<bool, UiError> {
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
        owner,
        &egs_cache::Adopt {
            app_name: &install.app_name,
            install_path: &folder,
            version: &install.version,
            install_size: install.install_size,
            egl_guid: &install.installation_guid,
            needs_verify: install.needs_verify(),
        },
    )?;
    if adopted {
        tracing::info!(
            needs_verify = install.needs_verify(),
            "adopted an official-launcher install"
        );
    }
    Ok(adopted)
}

/// The installs automatic import takes: complete and usable, owned by this account, not
/// installed here yet, and not uninstalled here before.
fn to_import<'a, V>(
    installs: &'a [egs_import::EglInstall],
    owned: &HashMap<String, V>,
    installed: &HashSet<String>,
    removed: &HashSet<String>,
) -> Vec<&'a egs_import::EglInstall> {
    installs
        .iter()
        .filter(|i| {
            i.adoptable()
                && owned.contains_key(&i.app_name)
                && !installed.contains(&i.app_name)
                && !removed.contains(&i.installation_guid)
        })
        .collect()
}

fn removed_guids(state: &AppState) -> HashSet<String> {
    state
        .cache
        .settings()
        .ok()
        .and_then(|s| s.get(REMOVED_KEY).cloned())
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_default()
}

/// After uninstalling an adopted game here: don't bring it back automatically.
pub fn remember_removed(state: &AppState, egl_guid: &str) {
    let mut guids = removed_guids(state);
    if guids.insert(egl_guid.to_owned()) {
        let mut list: Vec<_> = guids.into_iter().collect();
        list.sort();
        if let Err(e) = state
            .cache
            .set_setting(REMOVED_KEY, &serde_json::json!(list))
        {
            tracing::warn!(error = %e, "could not record the removed install");
        }
    }
}

/// After a library sync: adopt every install the official launcher has that qualifies
/// (see `to_import`). Reads only; never writes to the game folders or EGL's files.
pub async fn auto_import(app: &AppHandle) {
    let (Some(state), Some(auth)) = (app.try_state::<AppState>(), app.try_state::<AuthState>())
    else {
        return;
    };
    if !setting_on(&state.cache, "downloads.autoImport") {
        return;
    }
    let Some(owner) = auth.session.current().await.map(|a| a.account_id) else {
        return;
    };
    let installs = match scan() {
        Ok(i) => i,
        // No official launcher on this PC is the normal case.
        Err(e) => {
            tracing::debug!(error = %e.message, "no official launcher installs to import");
            return;
        }
    };
    let Ok(owned) = state.cache.known_games(&owner) else {
        return;
    };
    let Ok(snapshot) = state.cache.snapshot() else {
        return;
    };
    let installed: HashSet<_> = snapshot
        .games
        .iter()
        .filter(|g| !matches!(g.state, egs_cache::InstallState::NotInstalled))
        .map(|g| g.app_name.clone())
        .collect();
    let removed = removed_guids(&state);
    let mut titles = Vec::new();
    for install in to_import(&installs, &owned, &installed, &removed) {
        match adopt(&state, &owner, install) {
            Ok(true) => titles.push(title(install)),
            Ok(false) => {}
            Err(e) => tracing::warn!(error = %e.message, "automatic import skipped a game"),
        }
    }
    tracing::info!(
        found = installs.len(),
        added = titles.len(),
        "automatic import from the official launcher"
    );
    if !titles.is_empty() {
        let _ = LibraryChanged {
            reason: "import".into(),
        }
        .emit(app);
        let _ = GamesImported { titles }.emit(app);
    }
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

    if !adopt(&state, &owner, &install)? {
        return Err(UiError {
            code: "import".into(),
            message: "This game isn't in your library, or it's already installed here.".into(),
        });
    }
    let _ = LibraryChanged {
        reason: "import".into(),
    }
    .emit(&app);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn install(app: &str, guid: &str) -> egs_import::EglInstall {
        egs_import::EglInstall {
            app_name: app.into(),
            display_name: app.into(),
            install_location: PathBuf::from("X:/Games").join(app),
            namespace: String::new(),
            catalog_item_id: String::new(),
            version: "1.0".into(),
            installation_guid: guid.into(),
            install_size: 1,
            is_content_pack: false,
            main_game_app_name: None,
            incomplete: false,
            needs_validation: false,
            manifest_path: PathBuf::new(),
            manifest_ok: true,
            pending_update: false,
            folder_exists: true,
            executable_found: true,
            launch_executable: "Game.exe".into(),
            launch_command: String::new(),
            can_run_offline: true,
            requires_ownership_token: false,
            install_tags: vec![],
        }
    }

    #[test]
    fn imports_only_owned_complete_new_installs() {
        let mut pack = install("Pack", "G5");
        pack.is_content_pack = true;
        let mut broken = install("Broken", "G6");
        broken.incomplete = true;
        let mut no_exe = install("NoExe", "G7");
        no_exe.executable_found = false;
        let installs = vec![
            install("Owned", "G1"),
            install("NotOwned", "G2"),
            install("AlreadyHere", "G3"),
            install("RemovedHere", "G4"),
            pack,
            broken,
            no_exe,
        ];
        let owned: HashMap<String, ()> = [
            "Owned",
            "AlreadyHere",
            "RemovedHere",
            "Pack",
            "Broken",
            "NoExe",
        ]
        .iter()
        .map(|a| (a.to_string(), ()))
        .collect();
        let installed = HashSet::from(["AlreadyHere".to_string()]);
        let removed = HashSet::from(["G4".to_string()]);
        let picked: Vec<_> = to_import(&installs, &owned, &installed, &removed)
            .iter()
            .map(|i| i.app_name.as_str())
            .collect();
        assert_eq!(picked, ["Owned"]);
    }
}
