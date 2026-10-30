//! Read-only detection of installs made by the official Epic Games Launcher (EGL).
//!
//! Spec: docs/protocol/egl-local-files.md. Nothing in this crate writes, moves or
//! deletes anything: it only reads `.item` manifests and checks the game folder.

use std::path::{Path, PathBuf};

use serde::Deserialize;

#[derive(Debug, thiserror::Error)]
pub enum ImportError {
    #[error("the Epic Games Launcher's data folder wasn't found")]
    NotFound,
    #[error("couldn't read {0}")]
    Io(String),
}

/// EGL's per-installation `.item` (only the fields we use).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct ItemFile {
    #[serde(default)]
    format_version: u32,
    app_name: String,
    #[serde(default)]
    display_name: String,
    install_location: String,
    #[serde(default)]
    catalog_namespace: String,
    #[serde(default)]
    catalog_item_id: String,
    #[serde(default)]
    app_version_string: String,
    #[serde(default)]
    installation_guid: String,
    #[serde(default)]
    install_size: u64,
    #[serde(default)]
    complete_manifest_path: String,
    #[serde(default)]
    manifest_location: String,
    #[serde(default)]
    staging_location: String,
    // EGL's flags start with a lowercase `b`, which PascalCase can't express.
    #[serde(default, rename = "bIsIncompleteInstall")]
    b_is_incomplete_install: bool,
    // EGL's flags start with a lowercase `b`, which PascalCase can't express.
    #[serde(default, rename = "bNeedsValidation")]
    b_needs_validation: bool,
    #[serde(default)]
    app_categories: Vec<String>,
    #[serde(default)]
    main_game_app_name: String,
    #[serde(default)]
    launch_executable: String,
    #[serde(default)]
    launch_command: String,
    // EGL's flags start with a lowercase `b`, which PascalCase can't express.
    #[serde(default, rename = "bCanRunOffline")]
    b_can_run_offline: bool,
    #[serde(default)]
    ownership_token: String,
    #[serde(default)]
    install_tags: Vec<String>,
}

/// One install EGL knows about.
#[derive(Debug, Clone, PartialEq)]
pub struct EglInstall {
    pub app_name: String,
    pub display_name: String,
    pub install_location: PathBuf,
    pub namespace: String,
    pub catalog_item_id: String,
    pub version: String,
    pub installation_guid: String,
    pub install_size: u64,
    /// Content pack attached to another game (`AppCategories` contains `hidden`).
    pub is_content_pack: bool,
    pub main_game_app_name: Option<String>,
    pub incomplete: bool,
    pub needs_validation: bool,
    pub manifest_path: PathBuf,
    /// The binary/JSON manifest exists and has a valid header.
    pub manifest_ok: bool,
    /// `.egstore\bps` or `.egstore\Pending` has files: an interrupted update.
    pub pending_update: bool,
    pub folder_exists: bool,
    /// `LaunchExecutable` is present in the folder (or the item names none).
    pub executable_found: bool,
    pub launch_executable: String,
    pub launch_command: String,
    pub can_run_offline: bool,
    pub requires_ownership_token: bool,
    pub install_tags: Vec<String>,
}

impl EglInstall {
    /// Safe to offer as "use this install": complete, folder, manifest and executable present, a
    /// real game. (Our uninstall deletes a game's files but leaves EGL's records and `.egstore`.)
    pub fn adoptable(&self) -> bool {
        self.folder_exists
            && self.manifest_ok
            && self.executable_found
            && !self.incomplete
            && !self.is_content_pack
    }

    /// Adopted installs are verified before their first launch/update when this is true.
    pub fn needs_verify(&self) -> bool {
        self.needs_validation || self.pending_update
    }
}

/// `%PROGRAMDATA%` (EGL's data lives under `Epic\EpicGamesLauncher\Data`).
pub fn program_data() -> Option<PathBuf> {
    std::env::var_os("PROGRAMDATA").map(PathBuf::from)
}

fn manifests_dir(program_data: &Path) -> PathBuf {
    program_data
        .join("Epic")
        .join("EpicGamesLauncher")
        .join("Data")
        .join("Manifests")
}

/// Binary manifests start with `0C C0 BE 44`; JSON manifests with `{`.
pub fn manifest_header_ok(path: &Path) -> bool {
    use std::io::Read;
    let Ok(mut f) = std::fs::File::open(path) else {
        return false;
    };
    let mut head = [0u8; 4];
    match f.read(&mut head) {
        Ok(4) => head == [0x0C, 0xC0, 0xBE, 0x44] || head[0] == b'{',
        Ok(n) => n > 0 && head[0] == b'{',
        Err(_) => false,
    }
}

fn dir_has_files(dir: &Path) -> bool {
    std::fs::read_dir(dir)
        .map(|mut it| it.next().is_some())
        .unwrap_or(false)
}

fn to_install(item: ItemFile) -> EglInstall {
    let location = PathBuf::from(&item.install_location);
    let egstore = location.join(".egstore");
    let manifest_path = if item.complete_manifest_path.is_empty() {
        PathBuf::from(&item.manifest_location).join(format!("{}.manifest", item.installation_guid))
    } else {
        PathBuf::from(&item.complete_manifest_path)
    };
    let staging = if item.staging_location.is_empty() {
        egstore.join("bps")
    } else {
        PathBuf::from(&item.staging_location)
    };
    EglInstall {
        executable_found: item.launch_executable.is_empty()
            || location.join(&item.launch_executable).is_file(),
        manifest_ok: manifest_header_ok(&manifest_path),
        pending_update: dir_has_files(&staging) || dir_has_files(&egstore.join("Pending")),
        folder_exists: location.is_dir(),
        is_content_pack: item
            .app_categories
            .iter()
            .any(|c| c.eq_ignore_ascii_case("hidden")),
        main_game_app_name: Some(item.main_game_app_name).filter(|s| !s.is_empty()),
        requires_ownership_token: item.ownership_token.eq_ignore_ascii_case("true"),
        app_name: item.app_name,
        display_name: item.display_name,
        install_location: location,
        namespace: item.catalog_namespace,
        catalog_item_id: item.catalog_item_id,
        version: item.app_version_string,
        installation_guid: item.installation_guid,
        install_size: item.install_size,
        incomplete: item.b_is_incomplete_install,
        needs_validation: item.b_needs_validation,
        manifest_path,
        launch_executable: item.launch_executable,
        launch_command: item.launch_command,
        can_run_offline: item.b_can_run_offline,
        install_tags: item.install_tags,
    }
}

/// Read every `.item` under EGL's data folder. Unreadable files are skipped with a warning.
pub fn scan(program_data: &Path) -> Result<Vec<EglInstall>, ImportError> {
    let dir = manifests_dir(program_data);
    let entries = std::fs::read_dir(&dir).map_err(|_| ImportError::NotFound)?;
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("item") {
            continue;
        }
        let bytes = match std::fs::read(&path) {
            Ok(b) => b,
            Err(e) => {
                tracing::warn!(error = %e, "unreadable EGL item file");
                continue;
            }
        };
        match serde_json::from_slice::<ItemFile>(&bytes) {
            Ok(item) if item.format_version == 0 => out.push(to_install(item)),
            Ok(item) => tracing::warn!(
                format = item.format_version,
                "unknown EGL item format; skipped"
            ),
            Err(e) => tracing::warn!(error = %e, "unparseable EGL item file"),
        }
    }
    out.sort_by(|a, b| {
        a.display_name
            .to_lowercase()
            .cmp(&b.display_name.to_lowercase())
    });
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// Synthetic layout mirroring docs/protocol/egl-local-files.md (no real user data).
    fn fixture(root: &Path, game: &Path, hidden: bool, incomplete: bool, pending: bool) {
        let manifests = manifests_dir(root);
        fs::create_dir_all(&manifests).unwrap();
        let egstore = game.join(".egstore");
        fs::create_dir_all(egstore.join("Pending")).unwrap();
        let guid = if hidden {
            "11112222333344445555666677778888"
        } else {
            "AAAABBBBCCCCDDDDEEEEFFFF00001111"
        };
        fs::write(
            egstore.join(format!("{guid}.manifest")),
            [0x0C, 0xC0, 0xBE, 0x44, 41, 0, 0, 0],
        )
        .unwrap();
        if pending {
            fs::write(egstore.join("Pending").join("x.manifest"), b"x").unwrap();
        }
        fs::create_dir_all(game.join("Bin")).unwrap();
        fs::write(game.join("Bin").join("Game.exe"), b"MZ").unwrap();
        let item = serde_json::json!({
            "FormatVersion": 0,
            "AppName": if hidden { "ContentPack" } else { "SampleGame" },
            "DisplayName": if hidden { "Sample Content" } else { "Sample Game" },
            "InstallLocation": game.to_string_lossy(),
            "CatalogNamespace": "sample",
            "CatalogItemId": "0123456789abcdef0123456789abcdef",
            "AppVersionString": "1.2.3-CL-100-Windows",
            "InstallationGuid": guid,
            "InstallSize": 123456789u64,
            "ManifestLocation": egstore.to_string_lossy(),
            "CompleteManifestPath": egstore.join(format!("{guid}.manifest")).to_string_lossy(),
            "StagingLocation": egstore.join("bps").to_string_lossy(),
            "bIsIncompleteInstall": incomplete,
            "bNeedsValidation": false,
            "AppCategories": if hidden { vec!["hidden"] } else { vec!["games", "applications"] },
            "MainGameAppName": if hidden { "SampleGame" } else { "" },
            "LaunchExecutable": "Bin/Game.exe",
            "LaunchCommand": "",
            "bCanRunOffline": true,
            "OwnershipToken": "false",
            "InstallTags": [],
            "SomeFutureField": 1
        });
        fs::write(
            manifests.join(format!("{guid}.item")),
            serde_json::to_vec(&item).unwrap(),
        )
        .unwrap();
    }

    #[test]
    fn scans_and_classifies_installs() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("ProgramData");
        let game = tmp.path().join("Games").join("Sample");
        fixture(&root, &game, false, false, true);
        fixture(&root, &game, true, false, false);

        let found = scan(&root).unwrap();
        assert_eq!(found.len(), 2);
        let g = found.iter().find(|i| i.app_name == "SampleGame").unwrap();
        assert!(g.adoptable());
        assert!(
            g.needs_verify(),
            "pending update folder should require verify"
        );
        assert_eq!(g.version, "1.2.3-CL-100-Windows");
        assert!(g.can_run_offline && !g.requires_ownership_token);

        let pack = found.iter().find(|i| i.app_name == "ContentPack").unwrap();
        assert!(pack.is_content_pack && !pack.adoptable());
        assert_eq!(pack.main_game_app_name.as_deref(), Some("SampleGame"));
    }

    #[test]
    fn incomplete_or_missing_installs_are_not_adoptable() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("ProgramData");
        let game = tmp.path().join("Games").join("Sample");
        fixture(&root, &game, false, true, false);
        let g = scan(&root).unwrap().pop().unwrap();
        assert!(!g.adoptable());

        fs::remove_dir_all(&game).unwrap();
        let g = scan(&root).unwrap().pop().unwrap();
        assert!(!g.folder_exists && !g.manifest_ok && !g.adoptable());
    }

    #[test]
    fn install_without_its_executable_is_not_adoptable() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("ProgramData");
        let game = tmp.path().join("Games").join("Sample");
        fixture(&root, &game, false, false, false);
        fs::remove_file(game.join("Bin").join("Game.exe")).unwrap();
        let g = scan(&root).unwrap().pop().unwrap();
        assert!(g.folder_exists && g.manifest_ok && !g.executable_found && !g.adoptable());
    }

    #[test]
    fn missing_egl_is_not_found() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(matches!(scan(tmp.path()), Err(ImportError::NotFound)));
    }
}
