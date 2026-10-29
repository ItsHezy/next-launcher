//! Cloud saves (Phase 8). Spec: docs/protocol/cloud-saves.md, ADR 0012.
//! Storage calls: `egs-api::saves`; packaging: `egs-manifest::savegame`.
//!
//! Safety: a download never runs without first copying the local files it replaces to
//! `<data>/save-backups/<app>/<time>/` (last 3 kept); conflicts are never resolved silently
//! unless the user chose "prefer this PC" or "prefer cloud" in Settings.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use egs_api::saves as api;
use egs_manifest::savegame::{self, SaveFile};
use egs_manifest::Manifest;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};
use tauri_specta::Event;

use crate::auth::AuthState;
use crate::commands::{settings_value, AppState, UiError};
use crate::install::DownloadsState;

const BACKUPS_KEPT: usize = 3;

#[derive(Default)]
pub struct SavesState {
    busy: Mutex<HashSet<String>>,
}

/// Where a game keeps its saves and which files count (catalog attributes).
#[derive(Debug, Clone)]
pub struct SaveSpec {
    pub folder: PathBuf,
    pub cloud_folder: String,
    pub cloud_folder_mac: Option<String>,
    pub include: Option<String>,
    pub exclude: Option<String>,
}

/// What we last agreed with the cloud (setting `internal.saves.<app>`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Record {
    pub manifest: String,
    pub local_mtime: i64,
    pub at: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum Decision {
    /// No saves anywhere yet.
    Nothing,
    InSync,
    Upload,
    Download,
    Conflict,
}

/// The sync decision table (cloud-saves.md "Sync decision").
pub fn decide(
    local: Option<i64>,
    remote: Option<(&str, i64)>,
    record: Option<&Record>,
) -> Decision {
    match (local, remote) {
        (None, None) => Decision::Nothing,
        (Some(_), None) => Decision::Upload,
        (None, Some(_)) => Decision::Download,
        (Some(l), Some((name, r))) => match record {
            Some(rec) => {
                // 2 s of slack: some file systems store coarser times than we read.
                let local_changed = l > rec.local_mtime + 2;
                let remote_changed = name != rec.manifest;
                match (local_changed, remote_changed) {
                    (false, false) => Decision::InSync,
                    (true, false) => Decision::Upload,
                    (false, true) => Decision::Download,
                    (true, true) => Decision::Conflict,
                }
            }
            // First sync on this PC: equal within a minute is the same save; else ask.
            None if (l - r).abs() < 60 => Decision::InSync,
            None => Decision::Conflict,
        },
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SaveStatus {
    pub app_name: String,
    pub title: String,
    pub supported: bool,
    /// Cloud saves are on for this game (global switch and per-game switch).
    pub enabled: bool,
    pub local_path: Option<String>,
    pub local_files: f64,
    /// Newest local save file, Unix seconds.
    pub local_time: Option<f64>,
    /// Newest cloud save, Unix seconds.
    pub cloud_time: Option<f64>,
    pub last_sync: Option<f64>,
    pub decision: Decision,
}

/// A sync started, finished or needs the user (events for the UI).
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
pub struct SaveSync {
    pub app_name: String,
    /// `syncing` | `done` | `conflict` | `failed`
    pub state: String,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum SyncAction {
    /// Follow the decision table (conflicts follow the setting).
    Auto,
    /// Upload this PC's saves (the user chose it).
    Upload,
    /// Replace this PC's saves with the cloud's (backed up first).
    Download,
}

fn err(code: &str, message: impl Into<String>) -> UiError {
    UiError {
        code: code.into(),
        message: message.into(),
    }
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// `2026.10.29-14.05.33` (UTC) for a manifest name.
fn build_version(unix: i64) -> String {
    let iso = egs_api::launch::iso8601(unix * 1000); // 2026-10-29T14:05:33.000Z
    format!(
        "{}.{}.{}-{}.{}.{}",
        &iso[0..4],
        &iso[5..7],
        &iso[8..10],
        &iso[11..13],
        &iso[14..16],
        &iso[17..19]
    )
}

/// Unix seconds from `2026.10.29-14.05.33.manifest`.
fn manifest_time(name: &str) -> Option<i64> {
    let s = name.trim_end_matches(".manifest");
    let b = s.as_bytes();
    if b.len() != 19 || b[4] != b'.' || b[7] != b'.' || b[10] != b'-' {
        return None;
    }
    crate::library::parse_iso8601(&format!(
        "{}-{}-{}T{}:{}:{}Z",
        &s[0..4],
        &s[5..7],
        &s[8..10],
        &s[11..13],
        &s[14..16],
        &s[17..19]
    ))
}

fn attr(meta: &serde_json::Value, key: &str) -> Option<String> {
    meta.get("customAttributes")?
        .get(key)?
        .get("value")?
        .as_str()
        .map(str::to_owned)
        .filter(|v| !v.trim().is_empty())
}

/// Resolve `{InstallDir}/Saves/`-style templates (variables are case-insensitive).
pub fn resolve(template: &str, install_dir: &Path, account_id: &str) -> Option<PathBuf> {
    let profile = std::env::var_os("USERPROFILE").map(PathBuf::from)?;
    let local = std::env::var_os("LOCALAPPDATA").map(PathBuf::from)?;
    let mut out = String::new();
    let mut rest = template.replace('\\', "/");
    while let Some(start) = rest.find('{') {
        let end = rest[start..].find('}')? + start;
        out.push_str(&rest[..start]);
        let value = match rest[start + 1..end].to_ascii_lowercase().as_str() {
            "installdir" => install_dir.to_string_lossy().into_owned(),
            "epicid" => account_id.to_owned(),
            "appdata" => local.to_string_lossy().into_owned(),
            "userdir" => profile.join("Documents").to_string_lossy().into_owned(),
            "userprofile" => profile.to_string_lossy().into_owned(),
            "usersavedgames" => profile.join("Saved Games").to_string_lossy().into_owned(),
            _ => return None, // unknown variable: don't guess a folder
        };
        out.push_str(&value);
        rest = rest[end + 1..].to_owned();
    }
    out.push_str(&rest);
    let p = PathBuf::from(out.replace('/', "\\"));
    p.is_absolute().then_some(p)
}

/// Local save files that pass the game's filters: (relative path, modified Unix secs).
fn scan(spec: &SaveSpec) -> Vec<(String, i64)> {
    let mut out = Vec::new();
    let mut stack = vec![spec.folder.clone()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for e in entries.flatten() {
            let Ok(ft) = e.file_type() else { continue };
            let path = e.path();
            if ft.is_dir() {
                stack.push(path);
            } else if ft.is_file() {
                let Ok(rel) = path.strip_prefix(&spec.folder) else {
                    continue;
                };
                let rel = rel.to_string_lossy().replace('\\', "/");
                if rel.ends_with(".nl-tmp")
                    || !savegame::keep(&rel, spec.include.as_deref(), spec.exclude.as_deref())
                {
                    continue;
                }
                let mtime = e
                    .metadata()
                    .ok()
                    .and_then(|m| m.modified().ok())
                    .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                    .map(|d| d.as_secs() as i64)
                    .unwrap_or(0);
                out.push((rel, mtime));
            }
        }
    }
    out.sort();
    out
}

struct Ctx {
    app_name: String,
    title: String,
    account_id: String,
    spec: Option<SaveSpec>,
    enabled: bool,
}

async fn context(app: &AppHandle, app_name: &str) -> Result<Ctx, UiError> {
    let state = app
        .try_state::<AppState>()
        .ok_or_else(|| UiError::internal("app state"))?;
    let auth = app
        .try_state::<AuthState>()
        .ok_or_else(|| UiError::internal("auth state"))?;
    let account_id = auth
        .session
        .current()
        .await
        .ok_or_else(|| err("notSignedIn", "Sign in to sync saves."))?
        .account_id;
    let game = state
        .cache
        .game_install(app_name)?
        .ok_or_else(|| err("unknown", "Unknown game."))?;
    let meta: Option<serde_json::Value> = game
        .meta_json
        .as_deref()
        .and_then(|j| serde_json::from_str(j).ok());
    let on = |key: &str, default: bool| {
        settings_value(&state.cache, key)
            .map(|v| v == "true")
            .unwrap_or(default)
    };
    let enabled = on("cloud.enabled", true) && on(&format!("game.{app_name}.cloud"), true);
    let spec = match (&meta, &game.install_path) {
        (Some(m), Some(dir)) => attr(m, "CloudSaveFolder").and_then(|cf| {
            resolve(&cf, Path::new(dir), &account_id).map(|folder| SaveSpec {
                folder,
                cloud_folder: cf,
                cloud_folder_mac: attr(m, "CloudSaveFolder_MAC"),
                include: attr(m, "CloudIncludeList"),
                exclude: attr(m, "CloudExcludeList"),
            })
        }),
        _ => None,
    };
    Ok(Ctx {
        app_name: app_name.to_owned(),
        title: game.title,
        account_id,
        spec,
        enabled,
    })
}

fn record(state: &AppState, app_name: &str) -> Option<Record> {
    settings_value(&state.cache, &format!("internal.saves.{app_name}"))
        .and_then(|v| serde_json::from_str(&v).ok())
}

fn set_record(state: &AppState, app_name: &str, r: &Record) {
    if let Ok(json) = serde_json::to_string(r) {
        let _ = state.cache.set_setting(
            &format!("internal.saves.{app_name}"),
            &serde_json::Value::String(json),
        );
    }
}

async fn status_of(
    app: &AppHandle,
    ctx: &Ctx,
) -> Result<(SaveStatus, Option<api::SaveFiles>), UiError> {
    let state = app
        .try_state::<AppState>()
        .ok_or_else(|| UiError::internal("app state"))?;
    let mut status = SaveStatus {
        app_name: ctx.app_name.clone(),
        title: ctx.title.clone(),
        supported: ctx.spec.is_some(),
        enabled: ctx.enabled,
        local_path: ctx
            .spec
            .as_ref()
            .map(|s| s.folder.to_string_lossy().into_owned()),
        local_files: 0.0,
        local_time: None,
        cloud_time: None,
        last_sync: None,
        decision: Decision::Nothing,
    };
    let Some(spec) = &ctx.spec else {
        return Ok((status, None));
    };
    let files = {
        let spec = spec.clone();
        tokio::task::spawn_blocking(move || scan(&spec))
            .await
            .map_err(|_| UiError::internal("save scan"))?
    };
    let local = files.iter().map(|(_, t)| *t).max();
    let auth = app
        .try_state::<AuthState>()
        .ok_or_else(|| UiError::internal("auth state"))?;
    let access = auth.session.access_token().await?;
    let listing = api::list(auth.session.api(), &access, &ctx.account_id, &ctx.app_name).await?;
    let newest = listing.manifests().last().map(|(n, _)| (*n).to_owned());
    let remote = newest
        .as_deref()
        .and_then(|n| manifest_time(n).map(|t| (n, t)));
    let rec = record(&state, &ctx.app_name);
    status.local_files = files.len() as f64;
    status.local_time = local.map(|t| t as f64);
    status.cloud_time = remote.map(|(_, t)| t as f64);
    status.last_sync = rec.as_ref().map(|r| r.at as f64);
    status.decision = decide(local, remote, rec.as_ref());
    Ok((status, Some(listing)))
}

#[tauri::command]
#[specta::specta]
pub async fn cloud_save_status(app: AppHandle, app_name: String) -> Result<SaveStatus, UiError> {
    let ctx = context(&app, &app_name).await?;
    Ok(status_of(&app, &ctx).await?.0)
}

/// Installed games whose catalog entry supports cloud saves (no network).
#[tauri::command]
#[specta::specta]
pub async fn cloud_save_games(state: State<'_, AppState>) -> Result<Vec<SaveStatus>, UiError> {
    let snapshot = state.cache.snapshot()?;
    let mut out = Vec::new();
    for g in snapshot.games.iter().filter(|g| {
        matches!(
            g.state,
            egs_cache::model::InstallState::Installed
                | egs_cache::model::InstallState::UpdateAvailable
        )
    }) {
        let Ok(Some(install)) = state.cache.game_install(&g.app_name) else {
            continue;
        };
        let supported = install
            .meta_json
            .as_deref()
            .and_then(|j| serde_json::from_str::<serde_json::Value>(j).ok())
            .is_some_and(|m| attr(&m, "CloudSaveFolder").is_some());
        if supported {
            out.push(SaveStatus {
                app_name: g.app_name.clone(),
                title: g.title.clone(),
                supported: true,
                enabled: true,
                local_path: None,
                local_files: 0.0,
                local_time: None,
                cloud_time: None,
                last_sync: record(&state, &g.app_name).map(|r| r.at as f64),
                decision: Decision::Nothing,
            });
        }
    }
    Ok(out)
}

/// Sync one game. Returns the status afterwards; `Conflict` is returned (not resolved) when
/// the setting says "ask" and the action is `Auto`.
#[tauri::command]
#[specta::specta]
pub async fn cloud_save_sync(
    app: AppHandle,
    app_name: String,
    action: SyncAction,
) -> Result<SaveStatus, UiError> {
    sync(&app, &app_name, action).await
}

pub async fn sync(
    app: &AppHandle,
    app_name: &str,
    action: SyncAction,
) -> Result<SaveStatus, UiError> {
    let saves = app
        .try_state::<SavesState>()
        .ok_or_else(|| UiError::internal("saves state"))?;
    {
        let mut busy = saves
            .busy
            .lock()
            .map_err(|_| UiError::internal("saves lock"))?;
        if !busy.insert(app_name.to_owned()) {
            return Err(err("busy", "Saves for this game are already syncing."));
        }
    }
    let result = sync_inner(app, app_name, action).await;
    if let Ok(mut busy) = saves.busy.lock() {
        busy.remove(app_name);
    }
    let (state, message) = match &result {
        Ok(s) if s.decision == Decision::Conflict => {
            ("conflict", "Cloud and local saves both changed.".to_owned())
        }
        Ok(_) => ("done", "Saves are in sync.".to_owned()),
        Err(e) => ("failed", e.message.clone()),
    };
    let _ = SaveSync {
        app_name: app_name.to_owned(),
        state: state.into(),
        message,
    }
    .emit(app);
    result
}

async fn sync_inner(
    app: &AppHandle,
    app_name: &str,
    action: SyncAction,
) -> Result<SaveStatus, UiError> {
    let state = app
        .try_state::<AppState>()
        .ok_or_else(|| UiError::internal("app state"))?;
    let ctx = context(app, app_name).await?;
    let spec = ctx
        .spec
        .clone()
        .ok_or_else(|| err("unsupported", "This game doesn't use cloud saves."))?;
    if let Some(dl) = app.try_state::<DownloadsState>() {
        dl.guard(&spec.folder)?;
    }
    let _ = SaveSync {
        app_name: app_name.to_owned(),
        state: "syncing".into(),
        message: format!("Syncing saves for {}…", ctx.title),
    }
    .emit(app);
    let (status, listing) = status_of(app, &ctx).await?;
    let listing = listing.unwrap_or_default();
    let todo = match action {
        SyncAction::Upload => Decision::Upload,
        SyncAction::Download => Decision::Download,
        SyncAction::Auto if status.decision == Decision::Conflict => {
            match settings_value(&state.cache, "cloud.conflict").as_deref() {
                Some("local") => Decision::Upload,
                Some("cloud") => Decision::Download,
                _ => return Ok(status), // ask: the UI decides
            }
        }
        SyncAction::Auto => status.decision,
    };
    match todo {
        Decision::Upload => upload(app, &ctx, &spec).await?,
        Decision::Download => download(app, &ctx, &spec, &listing).await?,
        Decision::InSync => {
            // First agreement on this PC: remember it so later changes are attributed correctly.
            if record(&state, app_name).is_none() {
                if let (Some(name), Some(local)) = (
                    listing.manifests().last().map(|(n, _)| (*n).to_owned()),
                    status.local_time,
                ) {
                    set_record(
                        &state,
                        app_name,
                        &Record {
                            manifest: name,
                            local_mtime: local as i64,
                            at: now_secs(),
                        },
                    );
                }
            }
        }
        Decision::Nothing | Decision::Conflict => {}
    }
    Ok(status_of(app, &ctx).await?.0)
}

async fn upload(app: &AppHandle, ctx: &Ctx, spec: &SaveSpec) -> Result<(), UiError> {
    let state = app
        .try_state::<AppState>()
        .ok_or_else(|| UiError::internal("app state"))?;
    let auth = app
        .try_state::<AuthState>()
        .ok_or_else(|| UiError::internal("auth state"))?;
    let read_spec = spec.clone();
    let (files, newest) = tokio::task::spawn_blocking(move || {
        let listed = scan(&read_spec);
        let newest = listed.iter().map(|(_, t)| *t).max().unwrap_or(0);
        let files: Result<Vec<SaveFile>, std::io::Error> = listed
            .into_iter()
            .map(|(rel, _)| {
                std::fs::read(read_spec.folder.join(rel.replace('/', "\\")))
                    .map(|data| SaveFile { path: rel, data })
            })
            .collect();
        (files, newest)
    })
    .await
    .map_err(|_| UiError::internal("save read"))?;
    let files = files.map_err(|e| err("saves", format!("Couldn't read the save files: {e}")))?;
    if files.is_empty() {
        return Ok(());
    }
    let version = build_version(now_secs());
    let package = savegame::package(
        &files,
        &ctx.app_name,
        &ctx.account_id,
        &spec.cloud_folder,
        spec.cloud_folder_mac.as_deref(),
        &version,
        random_guid,
    )
    .map_err(|e| err("saves", format!("Couldn't pack the saves: {e}")))?;
    let access = auth.session.access_token().await?;
    let api = auth.session.api();
    let mut paths: Vec<String> = package.chunks.iter().map(|(p, _)| p.clone()).collect();
    paths.push(package.manifest_path.clone());
    let links = api::links(api, &access, &ctx.account_id, &ctx.app_name, &paths).await?;
    let link = |rel: &str| {
        links
            .files
            .iter()
            .find(|(k, _)| k.ends_with(rel))
            .and_then(|(_, f)| f.write_link.clone())
            .ok_or_else(|| err("saves", "Epic didn't return an upload link."))
    };
    // Chunks first, the manifest last: a manifest is never visible before its data.
    for (path, bytes) in &package.chunks {
        api::store(api, &link(path)?, bytes.clone()).await?;
    }
    api::store(
        api,
        &link(&package.manifest_path)?,
        package.manifest.clone(),
    )
    .await?;
    let name = format!("{version}.manifest");
    set_record(
        &state,
        &ctx.app_name,
        &Record {
            manifest: name,
            local_mtime: newest,
            at: now_secs(),
        },
    );
    tracing::info!(
        files = files.len(),
        chunks = package.chunks.len(),
        "saves uploaded"
    );
    Ok(())
}

async fn download(
    app: &AppHandle,
    ctx: &Ctx,
    spec: &SaveSpec,
    listing: &api::SaveFiles,
) -> Result<(), UiError> {
    let state = app
        .try_state::<AppState>()
        .ok_or_else(|| UiError::internal("app state"))?;
    let auth = app
        .try_state::<AuthState>()
        .ok_or_else(|| UiError::internal("auth state"))?;
    let api = auth.session.api();
    let (name, file) = listing
        .manifests()
        .last()
        .map(|(n, f)| ((*n).to_owned(), (*f).clone()))
        .ok_or_else(|| err("saves", "There are no cloud saves for this game."))?;
    let when = manifest_time(&name)
        .ok_or_else(|| err("saves", "The cloud save has an unexpected name."))?;
    let prefix = listing
        .prefix_for(&ctx.app_name)
        .ok_or_else(|| err("saves", "Epic's save listing has an unexpected layout."))?;
    let manifest_bytes = api::fetch(
        api,
        file.read_link
            .as_deref()
            .ok_or_else(|| err("saves", "No download link for the save."))?,
    )
    .await?;
    let m = Manifest::parse(&manifest_bytes)
        .map_err(|_| err("saves", "The cloud save's file list is damaged."))?;
    let mut chunks = HashMap::new();
    for c in &m.chunks {
        let path = format!("{prefix}{}", m.chunk_path(c));
        let link = listing
            .files
            .get(&path)
            .and_then(|f| f.read_link.clone())
            .ok_or_else(|| err("saves", "Part of the cloud save is missing on Epic's side."))?;
        let bytes = api::fetch(api, &link).await?;
        let decoded = egs_manifest::chunk::decode(&bytes, &HashMap::new())
            .map_err(|_| err("saves", "Part of the cloud save is damaged."))?;
        chunks.insert(c.guid, decoded.data);
    }
    let files = savegame::unpack(&m, &chunks).map_err(|_| {
        err(
            "saves",
            "The cloud save didn't check out; nothing was changed.",
        )
    })?;
    // Only files the game's filters allow are written (never anything else in the folder).
    let files: Vec<SaveFile> = files
        .into_iter()
        .filter(|f| savegame::keep(&f.path, spec.include.as_deref(), spec.exclude.as_deref()))
        .collect();

    let backups = state.paths.root.join("save-backups").join(&ctx.app_name);
    let folder = spec.folder.clone();
    let count = files.len();
    tokio::task::spawn_blocking(move || write_files(&folder, &backups, files, when))
        .await
        .map_err(|_| UiError::internal("save write"))?
        .map_err(|e| err("saves", format!("Couldn't write the save files: {e}. Your previous saves are in the backup folder.")))?;
    // After writing, the local files carry the cloud save's time.
    set_record(
        &state,
        &ctx.app_name,
        &Record {
            manifest: name,
            local_mtime: when,
            at: now_secs(),
        },
    );
    tracing::info!(files = count, "saves downloaded");
    Ok(())
}

/// Back up the files about to be replaced, then write each one atomically with the save's time.
fn write_files(
    folder: &Path,
    backups: &Path,
    files: Vec<SaveFile>,
    when: i64,
) -> std::io::Result<()> {
    let stamp = build_version(now_secs());
    let backup = backups.join(&stamp);
    for f in &files {
        let target = safe_target(folder, &f.path)?;
        if target.is_file() {
            let dest = backup.join(f.path.replace('/', "\\"));
            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::copy(&target, &dest)?;
        }
    }
    prune_backups(backups);
    let mtime = UNIX_EPOCH + std::time::Duration::from_secs(when.max(0) as u64);
    for f in files {
        let target = safe_target(folder, &f.path)?;
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let tmp = PathBuf::from(format!("{}.nl-tmp", target.display()));
        {
            let mut out = std::fs::File::create(&tmp)?;
            std::io::Write::write_all(&mut out, &f.data)?;
            out.set_modified(mtime)?;
            out.sync_all()?;
        }
        std::fs::rename(&tmp, &target)?;
    }
    Ok(())
}

/// Join a manifest path under the save folder, refusing `..` and absolute paths.
fn safe_target(folder: &Path, rel: &str) -> std::io::Result<PathBuf> {
    egs_download::safe_join(folder, rel).map_err(|_| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("unsafe path {rel}"),
        )
    })
}

fn prune_backups(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut names: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    names.sort();
    while names.len() > BACKUPS_KEPT {
        let old = names.remove(0);
        let _ = std::fs::remove_dir_all(old);
    }
}

fn random_guid() -> egs_manifest::Guid {
    let mut b = [0u8; 16];
    if getrandom::fill(&mut b).is_err() {
        // Fall back to time-based bytes; uniqueness per upload is all that matters.
        let t = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        b = t.to_le_bytes();
    }
    [
        u32::from_le_bytes([b[0], b[1], b[2], b[3]]),
        u32::from_le_bytes([b[4], b[5], b[6], b[7]]),
        u32::from_le_bytes([b[8], b[9], b[10], b[11]]),
        u32::from_le_bytes([b[12], b[13], b[14], b[15]]),
    ]
}

/// Before a launch ("Sync when a game starts"). Conflicts with "ask" come back as an error
/// the launch prompt turns into a question; network trouble never blocks playing.
pub async fn before_launch(
    app: &AppHandle,
    app_name: &str,
    answer: Option<&str>,
) -> Result<(), UiError> {
    let Ok(ctx) = context(app, app_name).await else {
        return Ok(());
    };
    let Some(state) = app.try_state::<AppState>() else {
        return Ok(());
    };
    let on_start = settings_value(&state.cache, "cloud.syncOnStart")
        .map(|v| v == "true")
        .unwrap_or(true);
    if ctx.spec.is_none() || !ctx.enabled || !on_start {
        return Ok(());
    }
    let action = match answer {
        Some("skip") => return Ok(()),
        Some("upload") => SyncAction::Upload,
        Some("download") => SyncAction::Download,
        _ => SyncAction::Auto,
    };
    match sync(app, app_name, action).await {
        Ok(s) if s.decision == Decision::Conflict && action == SyncAction::Auto => {
            let fmt = |t: Option<f64>| {
                t.map(|t| egs_api::launch::iso8601(t as i64 * 1000))
                    .unwrap_or_default()
            };
            Err(err(
                "saveConflict",
                format!("{}|{}|{}", ctx.title, fmt(s.local_time), fmt(s.cloud_time)),
            ))
        }
        Ok(_) => Ok(()),
        Err(e) => {
            tracing::warn!(error = %e.message, "save sync before launch failed; starting anyway");
            Ok(())
        }
    }
}

/// After the game exits ("Sync when a game exits"). A conflict is announced to the UI.
pub async fn after_exit(app: &AppHandle, app_name: &str) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let on_exit = settings_value(&state.cache, "cloud.syncOnExit")
        .map(|v| v == "true")
        .unwrap_or(true);
    if !on_exit {
        return;
    }
    match context(app, app_name).await {
        Ok(ctx) if ctx.spec.is_some() && ctx.enabled => {
            if let Err(e) = sync(app, app_name, SyncAction::Auto).await {
                tracing::warn!(error = %e.message, "save sync after exit failed");
            }
        }
        _ => {}
    }
}

/// Dev tooling (`--dev-action saves:<app>` status, `saves-up:` / `saves-down:` / `saves-sync:`).
pub async fn dev_action(app: &AppHandle, action: &str, app_name: &str) -> Result<String, UiError> {
    let s = match action {
        "saves" => cloud_save_status(app.clone(), app_name.to_owned()).await?,
        "saves-up" => sync(app, app_name, SyncAction::Upload).await?,
        "saves-down" => sync(app, app_name, SyncAction::Download).await?,
        _ => sync(app, app_name, SyncAction::Auto).await?,
    };
    Ok(format!(
        "supported={} enabled={} local_files={} local_time={:?} cloud_time={:?} last_sync={:?} decision={:?}",
        s.supported, s.enabled, s.local_files, s.local_time, s.cloud_time, s.last_sync, s.decision
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(manifest: &str, local: i64) -> Record {
        Record {
            manifest: manifest.into(),
            local_mtime: local,
            at: 0,
        }
    }

    #[test]
    fn decision_table() {
        use Decision::*;
        let r = Some(("2026.10.29-10.00.00.manifest", 1000));
        assert_eq!(decide(None, None, None), Nothing);
        assert_eq!(decide(Some(5), None, None), Upload);
        assert_eq!(decide(None, r, None), Download);
        let synced = rec("2026.10.29-10.00.00.manifest", 1000);
        assert_eq!(decide(Some(1001), r, Some(&synced)), InSync, "within slack");
        assert_eq!(decide(Some(2000), r, Some(&synced)), Upload);
        let older = rec("2026.10.28-10.00.00.manifest", 1000);
        assert_eq!(decide(Some(1000), r, Some(&older)), Download);
        assert_eq!(decide(Some(2000), r, Some(&older)), Conflict);
        assert_eq!(
            decide(Some(1030), r, None),
            InSync,
            "first sync, same minute"
        );
        assert_eq!(
            decide(Some(5000), r, None),
            Conflict,
            "first sync, differs: ask"
        );
    }

    #[test]
    fn manifest_names_and_times() {
        let t = 1_793_285_175; // 2026-10-29T14:46:15Z
        assert_eq!(build_version(t), "2026.10.29-14.46.15");
        assert_eq!(manifest_time("2026.10.29-14.46.15.manifest"), Some(t));
        assert_eq!(manifest_time("nope.manifest"), None);
    }

    #[test]
    fn templates_resolve_and_refuse_unknowns() {
        let dir = Path::new("C:\\Games\\Wtl");
        assert_eq!(
            resolve("{InstallDir}/Saves/", dir, "acc"),
            Some(PathBuf::from("C:\\Games\\Wtl\\Saves\\"))
        );
        let p = resolve("{AppData}/Game/{EpicId}/", dir, "acc").unwrap();
        assert!(p.to_string_lossy().ends_with("\\Game\\acc\\"));
        assert_eq!(resolve("{Mystery}/x", dir, "acc"), None);
    }

    #[test]
    fn download_backs_up_and_writes_atomically() {
        let tmp = tempfile::tempdir().unwrap();
        let folder = tmp.path().join("Saves");
        let backups = tmp.path().join("backups");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("slot.sav"), b"old").unwrap();
        std::fs::write(folder.join("keep.txt"), b"untouched").unwrap();
        let files = vec![
            SaveFile {
                path: "slot.sav".into(),
                data: b"new".to_vec(),
            },
            SaveFile {
                path: "sub/extra.sav".into(),
                data: b"x".to_vec(),
            },
        ];
        write_files(&folder, &backups, files, 1_700_000_000).unwrap();
        assert_eq!(std::fs::read(folder.join("slot.sav")).unwrap(), b"new");
        assert_eq!(std::fs::read(folder.join("sub/extra.sav")).unwrap(), b"x");
        assert_eq!(
            std::fs::read(folder.join("keep.txt")).unwrap(),
            b"untouched"
        );
        let backup = std::fs::read_dir(&backups)
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        assert_eq!(std::fs::read(backup.join("slot.sav")).unwrap(), b"old");
        let mtime = std::fs::metadata(folder.join("slot.sav"))
            .unwrap()
            .modified()
            .unwrap();
        assert_eq!(
            mtime.duration_since(UNIX_EPOCH).unwrap().as_secs(),
            1_700_000_000
        );
        assert!(write_files(
            &folder,
            &backups,
            vec![SaveFile {
                path: "../evil".into(),
                data: vec![]
            }],
            0
        )
        .is_err());
    }
}
