//! Installs, updates, verify/repair and uninstall (Phase 4).
//! Spec: docs/protocol/downloads-cdn.md. Engine: `egs-download`.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use egs_api::downloads::{self, BuildInfo};
use egs_api::{Client, Secret};
use egs_auth::Session;
use egs_cache::{Cache, DownloadJob};
use egs_download::{Control, EngineConfig, FetchFn, Limiter, Outcome, PlanOptions};
use egs_manifest::Manifest;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};
use tauri_specta::Event;

use crate::auth::AuthState;
use crate::commands::{settings_value, AppState, UiError};
use crate::library::LibraryChanged;

/// Live progress of the running download (throttled to ~4/s).
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
pub struct DownloadProgress {
    pub id: f64,
    pub app_name: String,
    pub title: String,
    /// `preparing` | `downloading` | `verifying` | `finishing`
    pub phase: String,
    pub bytes_done: f64,
    pub bytes_total: f64,
    pub speed_bps: f64,
}

/// Live progress of a game being moved to another folder (throttled to ~4/s).
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
pub struct MoveProgress {
    pub app_name: String,
    pub bytes_done: f64,
    pub bytes_total: f64,
}

#[derive(Default)]
pub struct DownloadsState {
    pub limiter: Arc<Limiter>,
    controls: Mutex<HashMap<i64, Control>>,
    worker: AtomicBool,
    /// Games with a move in progress, and its cancel flag. Nothing else may touch them.
    moving: Mutex<HashMap<String, Arc<AtomicBool>>>,
    /// Set while a game runs and "Pause downloads while playing" is on: no job starts.
    hold: AtomicBool,
    /// Development guard (`--dev-install-root`): refuse writes outside this folder.
    pub install_root: Option<PathBuf>,
}

/// Holds a game's "moving" slot; released on drop.
struct MoveSlot<'a> {
    downloads: &'a DownloadsState,
    app_name: String,
}

impl Drop for MoveSlot<'_> {
    fn drop(&mut self) {
        if let Ok(mut m) = self.downloads.moving.lock() {
            m.remove(&self.app_name);
        }
    }
}

impl DownloadsState {
    pub fn new(install_root: Option<PathBuf>) -> Self {
        Self {
            install_root,
            ..Self::default()
        }
    }

    pub(crate) fn guard(&self, path: &Path) -> Result<(), UiError> {
        match &self.install_root {
            Some(root) if !path.starts_with(root) => Err(UiError {
                code: "devGuard".into(),
                message: format!("Development builds only write inside {}.", root.display()),
            }),
            _ => Ok(()),
        }
    }

    /// Hold the queue (no new job starts) or release it.
    pub fn set_hold(&self, hold: bool) {
        self.hold.store(hold, Ordering::SeqCst);
    }

    /// Pause every running job; returns their ids. The worker records the state.
    pub fn pause_running(&self) -> Vec<i64> {
        let Ok(controls) = self.controls.lock() else {
            return Vec::new();
        };
        controls
            .iter()
            .map(|(id, ctl)| {
                ctl.pause();
                *id
            })
            .collect()
    }

    /// Refuse while the game is being moved.
    pub(crate) fn ensure_not_moving(&self, app_name: &str) -> Result<(), UiError> {
        let moving = self
            .moving
            .lock()
            .map_err(|_| UiError::internal("move lock"))?;
        if moving.contains_key(app_name) {
            return Err(err("busy", "Wait for the move to finish."));
        }
        Ok(())
    }

    fn begin_move(&self, app_name: &str) -> Result<(MoveSlot<'_>, Arc<AtomicBool>), UiError> {
        let mut moving = self
            .moving
            .lock()
            .map_err(|_| UiError::internal("move lock"))?;
        if moving.contains_key(app_name) {
            return Err(err("busy", "This game is already being moved."));
        }
        let cancel = Arc::new(AtomicBool::new(false));
        moving.insert(app_name.to_owned(), cancel.clone());
        Ok((
            MoveSlot {
                downloads: self,
                app_name: app_name.to_owned(),
            },
            cancel,
        ))
    }
}

fn err(code: &str, message: impl Into<String>) -> UiError {
    UiError {
        code: code.into(),
        message: message.into(),
    }
}

fn emit_changed(app: &AppHandle, reason: &str) {
    let _ = LibraryChanged {
        reason: reason.to_owned(),
    }
    .emit(app);
}

fn sanitize_folder_name(title: &str) -> String {
    let s: String = title
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || " -_.'&()".contains(c) {
                c
            } else {
                ' '
            }
        })
        .collect();
    let s = s.split_whitespace().collect::<Vec<_>>().join(" ");
    let s = s.trim_matches('.').to_owned();
    if s.is_empty() {
        "Game".into()
    } else {
        s
    }
}

fn manifest_copy_path(state: &AppState, app_name: &str, version: &str) -> PathBuf {
    let safe: String = version
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || "+-._".contains(c) {
                c
            } else {
                '_'
            }
        })
        .collect();
    state
        .paths
        .manifests
        .join(app_name)
        .join(format!("{safe}.manifest"))
}

/// Content keys of an encrypted build, saved beside our manifest copy (never logged).
/// Verify, uninstall and updates need them to read the copy's file list offline.
fn secrets_path(copy: &Path) -> PathBuf {
    copy.with_extension("secrets.json")
}

/// Our saved copy of a build's manifest, decrypted with the keys saved beside it.
async fn read_manifest_copy(
    state: &AppState,
    app_name: &str,
    version: &str,
) -> Result<Manifest, UiError> {
    let copy = manifest_copy_path(state, app_name, version);
    let bytes = tokio::fs::read(&copy).await.map_err(|_| {
        err(
            "manifest",
            "The file list for this install is missing. Repair it to restore it.",
        )
    })?;
    let mut m = Manifest::parse(&bytes)
        .map_err(|_| err("manifest", "The file list for this install is damaged."))?;
    if m.is_encrypted() {
        let secrets: HashMap<String, String> = tokio::fs::read(secrets_path(&copy))
            .await
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        m.decrypt(&secrets).map_err(|_| {
            err(
                "manifest",
                "The key for this install's file list is missing. Repair it to restore it.",
            )
        })?;
    }
    Ok(m)
}

async fn save_manifest_copy(state: &AppState, app_name: &str, info: &BuildInfo, raw: &[u8]) {
    let copy = manifest_copy_path(state, app_name, &info.build_version);
    if let Some(parent) = copy.parent() {
        let _ = tokio::fs::create_dir_all(parent).await;
    }
    let _ = tokio::fs::write(&copy, raw).await;
    if !info.secrets.is_empty() {
        if let Ok(json) = serde_json::to_vec(&info.secrets) {
            let _ = tokio::fs::write(secrets_path(&copy), json).await;
        }
    }
}

async fn remove_manifest_copy(state: &AppState, app_name: &str, version: &str) {
    let copy = manifest_copy_path(state, app_name, version);
    let _ = tokio::fs::remove_file(secrets_path(&copy)).await;
    let _ = tokio::fs::remove_file(&copy).await;
}

struct LoadedBuild {
    info: BuildInfo,
    manifest: Manifest,
    raw: Vec<u8>,
}

async fn load_build(
    session: &Session,
    game: &egs_cache::GameInstall,
) -> Result<LoadedBuild, UiError> {
    let access = session.access_token().await?;
    let api = session.api();
    let info = downloads::build_info(
        api,
        &access,
        &game.namespace,
        &game.catalog_item_id,
        &game.app_name,
    )
    .await
    .map_err(|e| {
        if e.status() == Some(403) || e.status() == Some(404) {
            err(
                "notEntitled",
                "Epic doesn't offer a download of this game for your account.",
            )
        } else {
            UiError::from(e)
        }
    })?;
    let raw = downloads::fetch_manifest(api, &info).await?;
    let mut manifest = Manifest::parse(&raw).map_err(|e| {
        tracing::warn!(error = %e, "manifest parse failed");
        err(
            "manifest",
            "Epic's file list for this game couldn't be read.",
        )
    })?;
    manifest.decrypt(&info.secrets).map_err(|e| match e {
        egs_manifest::Error::MissingSecret => err(
            "notEntitled",
            "Epic didn't provide the files for this game.",
        ),
        other => {
            tracing::warn!(error = %other, "manifest decrypt failed");
            err(
                "manifest",
                "Epic's file list for this game couldn't be read.",
            )
        }
    })?;
    Ok(LoadedBuild {
        info,
        manifest,
        raw,
    })
}

// ---- prepare ---------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Component {
    pub id: String,
    pub title: String,
    pub description: String,
    pub required: bool,
    pub size: f64,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct InstallPreview {
    pub app_name: String,
    pub title: String,
    pub build_version: String,
    /// Size with only required components.
    pub install_size: f64,
    pub download_size: f64,
    pub components: Vec<Component>,
    pub default_folder: String,
    /// Free space on the default folder's drive, if known.
    pub free_bytes: Option<f64>,
}

fn components_from(
    sd: &Option<downloads::SdMeta>,
    m: &Manifest,
) -> Vec<(downloads::SdComponent, u64)> {
    let Some(sd) = sd else { return vec![] };
    sd.data
        .iter()
        .map(|c| {
            let tags: HashSet<String> = c.tags.iter().cloned().collect();
            let size = m
                .files
                .iter()
                .filter(|f| {
                    !f.install_tags.is_empty() && f.install_tags.iter().any(|t| tags.contains(t))
                })
                .map(|f| f.size())
                .sum();
            (c.clone(), size)
        })
        .collect()
}

fn selected_tags(sd: &Option<downloads::SdMeta>, chosen: &[String]) -> HashSet<String> {
    let mut tags = HashSet::new();
    if let Some(sd) = sd {
        for c in &sd.data {
            if c.required() || chosen.contains(&c.unique_id) {
                tags.extend(c.tags.iter().cloned());
            }
        }
    }
    tags
}

fn default_folder(app: &AppHandle, downloads: &DownloadsState, title: &str) -> PathBuf {
    let base = downloads.install_root.clone().unwrap_or_else(|| {
        let configured = app
            .try_state::<AppState>()
            .and_then(|s| settings_value(&s.cache, "downloads.installDir"))
            .unwrap_or_else(|| "C:\\Games".into());
        PathBuf::from(configured)
    });
    base.join(sanitize_folder_name(title))
}

#[tauri::command]
#[specta::specta]
pub async fn prepare_install(
    app: AppHandle,
    state: State<'_, AppState>,
    auth: State<'_, AuthState>,
    downloads: State<'_, DownloadsState>,
    app_name: String,
) -> Result<InstallPreview, UiError> {
    let game = state
        .cache
        .game_install(&app_name)?
        .ok_or_else(|| err("unknown", "That game isn't in your library."))?;
    let build = load_build(&auth.session, &game).await?;
    let sd = match &build.info.sdmeta_url {
        Some(url) => downloads::fetch_sdmeta(auth.session.api(), url).await.ok(),
        None => None,
    };
    let required = selected_tags(&sd, &[]);
    let folder = default_folder(&app, &downloads, &game.title);
    tracing::info!(
        feature_level = build.manifest.meta.feature_level,
        files = build.manifest.files.len(),
        signed_urls = build.info.use_signed_url,
        components = sd.as_ref().map(|s| s.data.len()).unwrap_or(0),
        "install prepared"
    );
    Ok(InstallPreview {
        app_name: game.app_name.clone(),
        title: game.title.clone(),
        build_version: build.info.build_version.clone(),
        install_size: build.manifest.install_size(&required) as f64,
        download_size: build.manifest.download_size(&required) as f64,
        components: components_from(&sd, &build.manifest)
            .into_iter()
            .map(|(c, size)| Component {
                id: c.unique_id.clone(),
                title: c.title.clone(),
                description: c.description.clone(),
                required: c.required(),
                size: size as f64,
            })
            .collect(),
        free_bytes: crate::disk::free_bytes(&folder).map(|b| b as f64),
        default_folder: folder.to_string_lossy().into_owned(),
    })
}

// ---- start / controls ------------------------------------------------------------

#[tauri::command]
#[specta::specta]
pub async fn start_install(
    app: AppHandle,
    app_name: String,
    folder: String,
    components: Vec<String>,
) -> Result<f64, UiError> {
    queue_install(&app, &app_name, Some(folder), components)
        .await
        .map(|id| id as f64)
}

/// Shared by the Install button and `--dev-install`. `folder: None` = default folder.
pub async fn queue_install(
    app: &AppHandle,
    app_name: &str,
    folder: Option<String>,
    components: Vec<String>,
) -> Result<i64, UiError> {
    let state = app
        .try_state::<AppState>()
        .ok_or_else(|| UiError::internal("app state"))?;
    let auth = app
        .try_state::<AuthState>()
        .ok_or_else(|| UiError::internal("auth state"))?;
    let downloads = app
        .try_state::<DownloadsState>()
        .ok_or_else(|| UiError::internal("downloads state"))?;
    let app_name = app_name.to_owned();
    let owner = auth
        .session
        .current()
        .await
        .ok_or_else(|| err("notSignedIn", "Sign in to install games."))?
        .account_id;
    let game = state
        .cache
        .game_install(&app_name)?
        .ok_or_else(|| err("unknown", "That game isn't in your library."))?;
    if game.state != "notInstalled" {
        return Err(err(
            "busy",
            "This game is already installed or downloading.",
        ));
    }
    let folder = match folder {
        Some(f) => PathBuf::from(f.trim()),
        None => default_folder(app, &downloads, &game.title),
    };
    if !folder.is_absolute() {
        return Err(err(
            "folder",
            "Choose a full folder path, like C:\\Games\\My Game.",
        ));
    }
    downloads.guard(&folder)?;

    let build = load_build(&auth.session, &game).await?;
    let sd = match &build.info.sdmeta_url {
        Some(url) => downloads::fetch_sdmeta(auth.session.api(), url).await.ok(),
        None => None,
    };
    let tags = selected_tags(&sd, &components);
    let need = build.manifest.install_size(&tags);
    if let Some(free) = crate::disk::free_bytes(&folder) {
        // Keep 1 GiB headroom for the system.
        if free < need + (1 << 30) {
            return Err(err(
                "diskFull",
                format!(
                    "Not enough space on that drive: {:.1} GB needed, {:.1} GB free.",
                    need as f64 / 1e9,
                    free as f64 / 1e9
                ),
            ));
        }
    }
    let tag_list: Vec<String> = {
        let mut v: Vec<String> = tags.into_iter().collect();
        v.sort();
        v
    };
    let download_size = build
        .manifest
        .download_size(&tag_list.iter().cloned().collect());
    let id = state
        .cache
        .enqueue_download(
            &owner,
            &app_name,
            "install",
            &folder.to_string_lossy(),
            &build.info.build_version,
            &tag_list,
            download_size as i64,
        )?
        .ok_or_else(|| err("busy", "This game is already downloading."))?;
    tracing::info!(id, "install queued");
    emit_changed(app, "download");
    kick(app);
    Ok(id)
}

/// Queue an update for an installed game whose build is out of date.
#[tauri::command]
#[specta::specta]
pub async fn start_update(app: AppHandle, app_name: String) -> Result<(), UiError> {
    queue_update(&app, &app_name).await
}

/// Shared by the Update button and auto-update.
pub async fn queue_update(app: &AppHandle, app_name: &str) -> Result<(), UiError> {
    let state = app
        .try_state::<AppState>()
        .ok_or_else(|| UiError::internal("app state"))?;
    let auth = app
        .try_state::<AuthState>()
        .ok_or_else(|| UiError::internal("auth state"))?;
    let downloads = app
        .try_state::<DownloadsState>()
        .ok_or_else(|| UiError::internal("downloads state"))?;
    let app_name = app_name.to_owned();
    let owner = auth
        .session
        .current()
        .await
        .ok_or_else(|| err("notSignedIn", "Sign in to update games."))?
        .account_id;
    let game = state
        .cache
        .game_install(&app_name)?
        .ok_or_else(|| err("unknown", "Unknown game."))?;
    let path = game
        .install_path
        .clone()
        .ok_or_else(|| err("notInstalled", "This game isn't installed."))?;
    downloads.guard(Path::new(&path))?;
    downloads.ensure_not_moving(&app_name)?;
    let latest = game.latest_version.clone().unwrap_or_default();
    state
        .cache
        .enqueue_download(
            &owner,
            &app_name,
            "update",
            &path,
            &latest,
            &game.install_tags,
            0,
        )?
        .ok_or_else(|| err("busy", "This game is already downloading."))?;
    emit_changed(app, "download");
    kick(app);
    Ok(())
}

/// "Update games automatically" (on by default): after a library sync, queue updates for
/// installed games. Skipped: games shared with the official launcher (their folder is
/// only written when the user clicks Update), games switched off in Manage games,
/// running or busy games, and, with "Update only when idle", while the user is active.
pub async fn auto_update(app: &AppHandle) {
    let (Some(state), Some(launch)) = (
        app.try_state::<AppState>(),
        app.try_state::<crate::launch::LaunchState>(),
    ) else {
        return;
    };
    let on = |key: &str, default: bool| {
        settings_value(&state.cache, key)
            .map(|v| v == "true")
            .unwrap_or(default)
    };
    if !on("downloads.autoUpdate", true) {
        return;
    }
    if on("downloads.updateOnlyIdle", false) && crate::disk::idle_secs().is_some_and(|s| s < 600) {
        tracing::info!("auto-update waits until the computer is idle");
        return;
    }
    let Ok(snapshot) = state.cache.snapshot() else {
        return;
    };
    for g in snapshot
        .games
        .iter()
        .filter(|g| g.state == egs_cache::model::InstallState::UpdateAvailable)
    {
        if !on(&format!("game.{}.autoUpdate", g.app_name), true) || launch.is_running(&g.app_name) {
            continue;
        }
        let Ok(Some(install)) = state.cache.game_install(&g.app_name) else {
            continue;
        };
        if install.egl_managed
            || state
                .cache
                .active_download(&g.app_name)
                .ok()
                .flatten()
                .is_some()
        {
            continue;
        }
        match queue_update(app, &g.app_name).await {
            Ok(()) => tracing::info!("auto-update queued"),
            Err(e) => tracing::info!(code = %e.code, "auto-update skipped"),
        }
    }
}

#[tauri::command]
#[specta::specta]
pub fn pause_download(app: AppHandle, id: f64) -> Result<(), UiError> {
    pause_job(&app, id as i64);
    Ok(())
}

/// Pause a download (button, tray). The worker records the state when the engine stops.
pub fn pause_job(app: &AppHandle, id: i64) {
    let (Some(state), Some(downloads)) = (
        app.try_state::<AppState>(),
        app.try_state::<DownloadsState>(),
    ) else {
        return;
    };
    let running = downloads
        .controls
        .lock()
        .ok()
        .and_then(|c| c.get(&id).cloned());
    match running {
        Some(ctl) => ctl.pause(),
        None => {
            let _ = state.cache.set_download_state(id, "paused", None);
        }
    }
    emit_changed(app, "download");
}

#[tauri::command]
#[specta::specta]
pub fn resume_download(app: AppHandle, id: f64) -> Result<(), UiError> {
    resume_job(&app, id as i64);
    Ok(())
}

/// Resume a paused or stopped download (button, tray).
pub fn resume_job(app: &AppHandle, id: i64) {
    if let Some(state) = app.try_state::<AppState>() {
        let _ = state.cache.set_download_state(id, "queued", None);
    }
    emit_changed(app, "download");
    kick(app);
}

#[tauri::command]
#[specta::specta]
pub async fn cancel_download(
    app: AppHandle,
    state: State<'_, AppState>,
    downloads: State<'_, DownloadsState>,
    id: f64,
) -> Result<(), UiError> {
    let id = id as i64;
    let running = downloads
        .controls
        .lock()
        .ok()
        .and_then(|c| c.get(&id).cloned());
    match running {
        Some(ctl) => ctl.cancel(), // the worker cleans up when the engine stops
        None => {
            if let Some(job) = state.cache.next_download_by_id(id)? {
                cleanup_cancelled(&state, &job).await;
            }
            state.cache.drop_download(id)?;
            emit_changed(&app, "download");
        }
    }
    Ok(())
}

/// Remove what a cancelled job left: a fresh install's written files, or the staging folder.
async fn cleanup_cancelled(state: &AppState, job: &DownloadJob) {
    let dir = PathBuf::from(&job.install_path);
    match job.kind.as_str() {
        "install" => {
            if let Ok(m) = read_manifest_copy(state, &job.app_name, &job.build_version).await {
                let _ = egs_download::uninstall(&m, &dir);
            }
            remove_manifest_copy(state, &job.app_name, &job.build_version).await;
        }
        _ => {
            let _ = tokio::fs::remove_dir_all(dir.join(".nl-staging")).await;
        }
    }
}

// ---- worker ----------------------------------------------------------------------

pub fn kick(app: &AppHandle) {
    let Some(dl) = app.try_state::<DownloadsState>() else {
        return;
    };
    if dl.worker.swap(true, Ordering::SeqCst) {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        while let Some(state) = app.try_state::<AppState>() {
            if app
                .try_state::<DownloadsState>()
                .is_some_and(|d| d.hold.load(Ordering::SeqCst))
            {
                break; // a game is running; the queue resumes when it exits
            }
            let job = match state.cache.next_download() {
                Ok(Some(j)) => j,
                _ => break,
            };
            if let Err(e) = run_job(&app, &job).await {
                tracing::warn!(error = %e.message, kind = %job.kind, "download job failed");
                // Files stay for a retry; an install stays "installing" until cancelled.
                let _ = state
                    .cache
                    .set_download_state(job.id, "failed", Some(&e.message));
                emit_changed(&app, "download");
                break; // don't hammer the next job on a systemic failure (e.g. offline)
            }
        }
        if let Some(dl) = app.try_state::<DownloadsState>() {
            dl.worker.store(false, Ordering::SeqCst);
        }
    });
}

/// Signs chunk paths in batches of ≤ 50 just ahead of use (downloads-cdn.md §2).
struct Signer {
    session: Arc<Session>,
    account_id: String,
    namespace: String,
    item: String,
    app: String,
    build: String,
    label: String,
    order: Vec<String>,
    pos: HashMap<String, usize>,
    signed: Mutex<HashMap<String, String>>,
    ticket: tokio::sync::Mutex<Option<(Secret, Instant)>>,
    batch_lock: tokio::sync::Mutex<()>,
}

impl Signer {
    async fn ticket(&self) -> Result<Secret, String> {
        let mut t = self.ticket.lock().await;
        if let Some((tk, at)) = t.as_ref() {
            if at.elapsed() < Duration::from_secs(10 * 60) {
                return Ok(tk.clone());
            }
        }
        let access = self
            .session
            .access_token()
            .await
            .map_err(|e| e.to_string())?;
        let ticket = downloads::download_ticket(
            self.session.api(),
            &access,
            &self.namespace,
            &self.item,
            &self.app,
            &self.build,
            &self.label,
        )
        .await
        .map_err(|e| e.to_string())?;
        *t = Some((ticket.signed_ticket.clone(), Instant::now()));
        Ok(ticket.signed_ticket)
    }

    async fn url_for(&self, path: &str) -> Result<String, String> {
        if let Some(u) = self.signed.lock().ok().and_then(|mut s| s.remove(path)) {
            return Ok(u);
        }
        let _guard = self.batch_lock.lock().await;
        if let Some(u) = self.signed.lock().ok().and_then(|mut s| s.remove(path)) {
            return Ok(u);
        }
        let start = self.pos.get(path).copied().unwrap_or(0);
        let mut batch: Vec<String> = vec![path.to_owned()];
        {
            let signed = self.signed.lock().map_err(|_| "lock".to_owned())?;
            for p in self.order.iter().skip(start + 1) {
                if batch.len() >= 50 {
                    break;
                }
                if !signed.contains_key(p) && !batch.contains(p) {
                    batch.push(p.clone());
                }
            }
        }
        let ticket = self.ticket().await?;
        let access = self
            .session
            .access_token()
            .await
            .map_err(|e| e.to_string())?;
        let urls = downloads::signed_chunk_urls(
            self.session.api(),
            &access,
            &self.account_id,
            &ticket,
            &batch,
        )
        .await
        .map_err(|e| {
            if e.epic_code() == Some("errors.com.epicgames.artifactdelivery.forbidden") {
                // Ticket expired: drop it so the next call fetches a new one.
                if let Ok(mut t) = self.ticket.try_lock() {
                    *t = None;
                }
            }
            e.to_string()
        })?;
        let mut signed = self.signed.lock().map_err(|_| "lock".to_owned())?;
        for (k, v) in urls {
            signed.insert(k, v);
        }
        signed
            .remove(path)
            .ok_or_else(|| "Epic didn't sign a chunk URL".to_owned())
    }
}

fn make_fetcher(
    api: Client,
    manifest: Arc<Manifest>,
    info: &BuildInfo,
    signer: Option<Arc<Signer>>,
) -> FetchFn {
    let bases = Arc::new(info.base_urls.clone());
    let secrets = Arc::new(info.secrets.clone());
    Arc::new(move |idx| {
        let (api, manifest, bases, secrets, signer) = (
            api.clone(),
            manifest.clone(),
            bases.clone(),
            secrets.clone(),
            signer.clone(),
        );
        Box::pin(async move {
            let chunk = manifest.chunks.get(idx).ok_or("chunk index")?;
            let path = manifest.chunk_path(chunk);
            let mut last = String::from("no CDN");
            for attempt in 0..4u32 {
                let urls: Vec<String> = match &signer {
                    Some(s) => vec![s.url_for(&path).await?],
                    None => {
                        // Rotate the starting CDN per chunk to spread load.
                        let n = bases.len().max(1);
                        (0..n)
                            .map(|i| format!("{}/{}", bases[(idx + i) % n], path))
                            .collect()
                    }
                };
                for url in urls {
                    let resp = api.http().get(&url).send().await;
                    match resp {
                        Ok(r) if r.status().is_success() => match r.bytes().await {
                            Ok(b) => match egs_manifest::chunk::decode(&b, &secrets) {
                                Ok(c) => return Ok(c.data),
                                Err(e) => last = format!("chunk decode: {e}"),
                            },
                            Err(e) => last = e.without_url().to_string(),
                        },
                        Ok(r) => last = format!("HTTP {}", r.status().as_u16()),
                        Err(e) => last = e.without_url().to_string(),
                    }
                }
                tokio::time::sleep(Duration::from_millis(500u64 << attempt)).await;
            }
            Err(last)
        })
    })
}

struct JobProgress {
    app: AppHandle,
    cache: Arc<Cache>,
    id: i64,
    app_name: String,
    title: String,
    base_done: u64,
    total: u64,
    last: Mutex<(Instant, u64, f64, Instant)>, // (last emit, last bytes, ema speed, last persist)
}

impl egs_download::Progress for JobProgress {
    fn bytes(&self, downloaded: u64, _written: u64) {
        let Ok(mut l) = self.last.lock() else { return };
        let now = Instant::now();
        let dt = now.duration_since(l.0).as_secs_f64();
        if dt < 0.25 {
            return;
        }
        let inst = (downloaded.saturating_sub(l.1)) as f64 / dt;
        l.2 = if l.2 == 0.0 {
            inst
        } else {
            l.2 * 0.7 + inst * 0.3
        };
        l.0 = now;
        l.1 = downloaded;
        let done = self.base_done + downloaded;
        let _ = DownloadProgress {
            id: self.id as f64,
            app_name: self.app_name.clone(),
            title: self.title.clone(),
            phase: "downloading".into(),
            bytes_done: done as f64,
            bytes_total: self.total as f64,
            speed_bps: l.2,
        }
        .emit(&self.app);
        if now.duration_since(l.3) >= Duration::from_secs(2) {
            l.3 = now;
            let _ = self.cache.set_download_progress(
                self.id,
                done as i64,
                self.total as i64,
                l.2 as i64,
            );
        }
    }

    fn file_done(&self, filename: &str) {
        let _ = self.cache.download_file_done(self.id, filename);
    }
}

fn phase(app: &AppHandle, job: &DownloadJob, title: &str, phase: &str, done: u64, total: u64) {
    let _ = DownloadProgress {
        id: job.id as f64,
        app_name: job.app_name.clone(),
        title: title.to_owned(),
        phase: phase.into(),
        bytes_done: done as f64,
        bytes_total: total as f64,
        speed_bps: 0.0,
    }
    .emit(app);
}

async fn run_job(app: &AppHandle, job: &DownloadJob) -> Result<(), UiError> {
    let state = app
        .try_state::<AppState>()
        .ok_or_else(|| UiError::internal("app state"))?;
    let auth = app
        .try_state::<AuthState>()
        .ok_or_else(|| UiError::internal("auth state"))?;
    let dl = app
        .try_state::<DownloadsState>()
        .ok_or_else(|| UiError::internal("downloads state"))?;
    let session = auth.session.clone();
    let account = session
        .current()
        .await
        .ok_or_else(|| err("notSignedIn", "Sign in to continue downloads."))?;
    let game = state
        .cache
        .game_install(&job.app_name)?
        .ok_or_else(|| err("unknown", "Unknown game."))?;
    let install_dir = PathBuf::from(&job.install_path);
    dl.guard(&install_dir)?;

    state.cache.set_download_state(job.id, "running", None)?;
    emit_changed(app, "download");
    phase(
        app,
        job,
        &game.title,
        "preparing",
        0,
        job.bytes_total.max(0) as u64,
    );

    let build = load_build(&session, &game).await?;
    let new = Arc::new(build.manifest);
    let tags: HashSet<String> = job.tags.iter().cloned().collect();

    // Keep our copy of the manifest from the start (cancel cleanup + resume need it).
    save_manifest_copy(&state, &job.app_name, &build.info, &build.raw).await;

    let (old, staged, mut opts) = match job.kind.as_str() {
        "update" => {
            let old_version = game.version.clone().unwrap_or_default();
            let old = read_manifest_copy(&state, &job.app_name, &old_version)
                .await
                .ok();
            if old.is_none() {
                tracing::warn!("no manifest for the installed build; updating without patching");
            }
            (
                old,
                true,
                PlanOptions {
                    tags: tags.clone(),
                    ..PlanOptions::default()
                },
            )
        }
        "repair" => {
            phase(app, job, &game.title, "verifying", 0, 0);
            let issues = egs_download::verify(
                new.clone(),
                install_dir.clone(),
                tags.clone(),
                2,
                Arc::new(AtomicU64::new(0)),
                Arc::new(AtomicBool::new(false)),
            )
            .await
            .map_err(|e| err("verify", e.to_string()))?;
            let only: HashSet<String> = issues.into_iter().map(|i| i.filename).collect();
            tracing::info!(files = only.len(), "repairing");
            (
                None,
                true,
                PlanOptions {
                    tags: tags.clone(),
                    only: Some(only),
                    no_reuse: true,
                    ..PlanOptions::default()
                },
            )
        }
        _ => (
            None,
            false,
            PlanOptions {
                tags: tags.clone(),
                ..PlanOptions::default()
            },
        ),
    };
    opts.completed = state.cache.download_files_done(job.id)?;
    let plan = Arc::new(egs_download::plan(&new, old.as_ref(), &opts));
    let total = if job.bytes_total > 0 {
        job.bytes_total as u64
    } else {
        plan.download_size
    };
    let base_done = total.saturating_sub(plan.download_size);

    let signer = build.info.use_signed_url.then(|| {
        let order: Vec<String> = plan
            .fetches
            .iter()
            .filter_map(|f| new.chunks.get(f.chunk))
            .map(|c| new.chunk_path(c))
            .collect();
        let pos = order
            .iter()
            .enumerate()
            .map(|(i, p)| (p.clone(), i))
            .collect();
        Arc::new(Signer {
            session: session.clone(),
            account_id: account.account_id.clone(),
            namespace: game.namespace.clone(),
            item: game.catalog_item_id.clone(),
            app: game.app_name.clone(),
            build: build.info.build_version.clone(),
            label: if build.info.label_name.is_empty() {
                "Live-Windows".into()
            } else {
                build.info.label_name.clone()
            },
            order,
            pos,
            signed: Mutex::new(HashMap::new()),
            ticket: tokio::sync::Mutex::new(None),
            batch_lock: tokio::sync::Mutex::new(()),
        })
    });
    let fetch = make_fetcher(session.api().clone(), new.clone(), &build.info, signer);

    let control = Control::default();
    if let Ok(mut c) = dl.controls.lock() {
        c.insert(job.id, control.clone());
    }
    let progress = Arc::new(JobProgress {
        app: app.clone(),
        cache: state.cache.clone(),
        id: job.id,
        app_name: job.app_name.clone(),
        title: game.title.clone(),
        base_done,
        total,
        last: Mutex::new((Instant::now(), 0, 0.0, Instant::now())),
    });
    let write_dir = if staged {
        install_dir.join(".nl-staging")
    } else {
        install_dir.clone()
    };
    let cache_mb = settings_value(&state.cache, "downloads.cacheSizeMb")
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(256);
    let cfg = EngineConfig {
        workers: 16,
        old_dir: staged.then(|| install_dir.clone()),
        cache_budget: cache_mb << 20,
        limiter: dl.limiter.clone(),
    };
    tracing::info!(
        kind = %job.kind,
        files = plan.jobs.len(),
        skipped = plan.skipped,
        fetches = plan.fetches.len(),
        download_mb = plan.download_size / 1_000_000,
        reuse_mb = plan.reuse_size / 1_000_000,
        peak_cache_mb = plan.peak_cache / 1_000_000,
        signed_urls = build.info.use_signed_url,
        "download started"
    );
    let started = Instant::now();
    let result = egs_download::run(
        new.clone(),
        plan.clone(),
        write_dir.clone(),
        fetch,
        cfg,
        control,
        progress,
    )
    .await;
    if let Ok(mut c) = dl.controls.lock() {
        c.remove(&job.id);
    }

    match result {
        Ok(Outcome::Completed) => {
            if staged {
                phase(app, job, &game.title, "finishing", total, total);
                egs_download::commit_staged(&write_dir, &install_dir, &plan.delete)
                    .await
                    .map_err(|e| err("commit", e.to_string()))?;
            }
            if let Some(old_version) = game
                .version
                .as_deref()
                .filter(|v| *v != build.info.build_version)
            {
                remove_manifest_copy(&state, &job.app_name, old_version).await;
            }
            state.cache.finish_install(
                job.id,
                &job.app_name,
                &build.info.build_version,
                &job.install_path,
                new.install_size(&tags),
                &job.tags,
            )?;
            let secs = started.elapsed().as_secs_f64();
            tracing::info!(
                kind = %job.kind,
                secs = secs as u64,
                mb_per_s = (plan.download_size as f64 / 1e6 / secs.max(0.001)) as u64,
                "download finished"
            );
            emit_changed(app, "download");
            match job.kind.as_str() {
                "install" => {
                    crate::notify::send(
                        app,
                        crate::notify::Kind::DownloadFinished,
                        &format!("{} is ready to play", game.title),
                        "The download finished.",
                    );
                }
                "update" => {
                    crate::notify::send(
                        app,
                        crate::notify::Kind::DownloadFinished,
                        &format!("{} was updated", game.title),
                        "The update finished.",
                    );
                }
                _ => {}
            }
            let verify_after = settings_value(&state.cache, "downloads.verifyAfterInstall")
                .is_some_and(|v| v == "true");
            if job.kind == "install" && verify_after {
                let (app, name) = (app.clone(), job.app_name.clone());
                tauri::async_runtime::spawn(async move {
                    match verify_installed(&app, &name).await {
                        Ok(r) if !r.problems.is_empty() => {
                            tracing::warn!(
                                problems = r.problems.len(),
                                "verify after install found problems; repairing"
                            );
                            let _ = queue_repair(&app, &name).await;
                        }
                        Ok(_) => {}
                        Err(e) => tracing::warn!(error = %e.message, "verify after install failed"),
                    }
                });
            }
        }
        Ok(Outcome::Paused) => {
            state.cache.set_download_state(job.id, "paused", None)?;
            emit_changed(app, "download");
        }
        Ok(Outcome::Cancelled) => {
            cleanup_cancelled(&state, job).await;
            state.cache.drop_download(job.id)?;
            emit_changed(app, "download");
        }
        Err(e) => {
            tracing::warn!(error = %e, "download stopped");
            return Err(err(
                "download",
                match e {
                    egs_download::DlError::Fetch(_) => {
                        "Couldn't download from Epic's servers. Retry when your connection is back."
                            .to_owned()
                    }
                    egs_download::DlError::Io(m) => format!("Couldn't write the game files ({m})."),
                    egs_download::DlError::Corrupt(f) => {
                        format!("A downloaded file didn't match Epic's checksum ({f}).")
                    }
                    egs_download::DlError::Path(_) => {
                        "Epic's file list contained an unsafe path; the install was stopped."
                            .to_owned()
                    }
                },
            ));
        }
    }
    Ok(())
}

// ---- verify / repair / uninstall --------------------------------------------------

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct VerifyResult {
    pub files_checked: f64,
    pub problems: Vec<String>,
}

pub(crate) async fn installed_manifest(
    state: &AppState,
    game: &egs_cache::GameInstall,
) -> Result<Manifest, UiError> {
    let version = game
        .version
        .clone()
        .ok_or_else(|| err("notInstalled", "This game isn't installed."))?;
    read_manifest_copy(state, &game.app_name, &version).await
}

#[tauri::command]
#[specta::specta]
pub async fn verify_game(app: AppHandle, app_name: String) -> Result<VerifyResult, UiError> {
    verify_installed(&app, &app_name).await
}

pub async fn verify_installed(app: &AppHandle, app_name: &str) -> Result<VerifyResult, UiError> {
    let state = app
        .try_state::<AppState>()
        .ok_or_else(|| UiError::internal("app state"))?;
    if let Some(downloads) = app.try_state::<DownloadsState>() {
        downloads.ensure_not_moving(app_name)?;
    }
    let game = state
        .cache
        .game_install(app_name)?
        .ok_or_else(|| err("unknown", "Unknown game."))?;
    let path = PathBuf::from(
        game.install_path
            .clone()
            .ok_or_else(|| err("notInstalled", "This game isn't installed."))?,
    );
    let m = Arc::new(installed_manifest(&state, &game).await?);
    let tags: HashSet<String> = game.install_tags.iter().cloned().collect();
    let checked = m.files.iter().filter(|f| f.selected(&tags)).count();
    let started = Instant::now();
    let issues = egs_download::verify(
        m,
        path,
        tags,
        2,
        Arc::new(AtomicU64::new(0)),
        Arc::new(AtomicBool::new(false)),
    )
    .await
    .map_err(|e| err("verify", e.to_string()))?;
    state.cache.set_needs_verify(app_name, !issues.is_empty())?;
    tracing::info!(
        checked,
        problems = issues.len(),
        ms = started.elapsed().as_millis() as u64,
        "verified install"
    );
    emit_changed(app, "verify");
    Ok(VerifyResult {
        files_checked: checked as f64,
        problems: issues.into_iter().take(50).map(|i| i.filename).collect(),
    })
}

#[tauri::command]
#[specta::specta]
pub async fn repair_game(app: AppHandle, app_name: String) -> Result<(), UiError> {
    queue_repair(&app, &app_name).await
}

pub async fn queue_repair(app: &AppHandle, app_name: &str) -> Result<(), UiError> {
    let state = app
        .try_state::<AppState>()
        .ok_or_else(|| UiError::internal("app state"))?;
    let auth = app
        .try_state::<AuthState>()
        .ok_or_else(|| UiError::internal("auth state"))?;
    let downloads = app
        .try_state::<DownloadsState>()
        .ok_or_else(|| UiError::internal("downloads state"))?;
    let owner = auth
        .session
        .current()
        .await
        .ok_or_else(|| err("notSignedIn", "Sign in to repair games."))?
        .account_id;
    let game = state
        .cache
        .game_install(app_name)?
        .ok_or_else(|| err("unknown", "Unknown game."))?;
    let path = game
        .install_path
        .clone()
        .ok_or_else(|| err("notInstalled", "This game isn't installed."))?;
    downloads.guard(Path::new(&path))?;
    downloads.ensure_not_moving(app_name)?;
    let version = game.version.clone().unwrap_or_default();
    state
        .cache
        .enqueue_download(
            &owner,
            app_name,
            "repair",
            &path,
            &version,
            &game.install_tags,
            0,
        )?
        .ok_or_else(|| err("busy", "This game is already downloading."))?;
    emit_changed(app, "download");
    kick(app);
    Ok(())
}

#[derive(Debug, Clone, Copy, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct UninstallOptions {
    /// The user confirmed removing files from a folder the official launcher also manages.
    pub confirm_external: bool,
}

#[tauri::command]
#[specta::specta]
pub async fn uninstall_game(
    app: AppHandle,
    app_name: String,
    options: UninstallOptions,
) -> Result<(), UiError> {
    uninstall_installed(&app, &app_name, options.confirm_external).await
}

pub async fn uninstall_installed(
    app: &AppHandle,
    app_name: &str,
    confirm_external: bool,
) -> Result<(), UiError> {
    let state = app
        .try_state::<AppState>()
        .ok_or_else(|| UiError::internal("app state"))?;
    let downloads = app
        .try_state::<DownloadsState>()
        .ok_or_else(|| UiError::internal("downloads state"))?;
    let game = state
        .cache
        .game_install(app_name)?
        .ok_or_else(|| err("unknown", "Unknown game."))?;
    let path = PathBuf::from(
        game.install_path
            .clone()
            .ok_or_else(|| err("notInstalled", "This game isn't installed."))?,
    );
    downloads.guard(&path)?;
    downloads.ensure_not_moving(app_name)?;
    if game.egl_managed && !confirm_external {
        return Err(err(
            "confirmExternal",
            format!(
                "{} is shared with the Epic Games Launcher. Confirm to delete its files.",
                path.display()
            ),
        ));
    }
    let m = installed_manifest(&state, &game).await?;
    let removed = tokio::task::spawn_blocking(move || egs_download::uninstall(&m, &path))
        .await
        .map_err(|_| UiError::internal("uninstall task"))?
        .map_err(|e| err("uninstall", format!("Couldn't remove all files: {e}")))?;
    let _ = tokio::fs::remove_dir_all(state.paths.manifests.join(app_name)).await;
    state.cache.mark_uninstalled(app_name)?;
    tracing::info!(removed, "uninstalled");
    emit_changed(app, "uninstall");
    Ok(())
}

// ---- move -------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct MoveOptions {
    /// The user confirmed moving a folder the official launcher also manages.
    pub confirm_external: bool,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct MoveResult {
    pub path: String,
    /// Same drive: renamed in place.
    pub renamed: bool,
    /// Some old files couldn't be deleted after the copy (in use); they are safe to remove.
    pub left_behind: bool,
}

#[tauri::command]
#[specta::specta]
pub async fn move_game(
    app: AppHandle,
    app_name: String,
    folder: String,
    options: MoveOptions,
) -> Result<MoveResult, UiError> {
    move_installed(&app, &app_name, &folder, options.confirm_external).await
}

#[tauri::command]
#[specta::specta]
pub fn cancel_move(downloads: State<'_, DownloadsState>, app_name: String) -> Result<(), UiError> {
    let moving = downloads
        .moving
        .lock()
        .map_err(|_| UiError::internal("move lock"))?;
    if let Some(cancel) = moving.get(&app_name) {
        cancel.store(true, Ordering::SeqCst);
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct MovePreview {
    pub current_path: String,
    /// Everything in the folder, including files the game wrote there.
    pub bytes: f64,
    pub egl_managed: bool,
}

#[tauri::command]
#[specta::specta]
pub async fn prepare_move(
    state: State<'_, AppState>,
    app_name: String,
) -> Result<MovePreview, UiError> {
    let game = state
        .cache
        .game_install(&app_name)?
        .ok_or_else(|| err("unknown", "Unknown game."))?;
    let path = game
        .install_path
        .clone()
        .filter(|_| game.state != "installing" && game.state != "notInstalled")
        .ok_or_else(|| err("notInstalled", "This game isn't installed."))?;
    let dir = PathBuf::from(&path);
    let tree = tokio::task::spawn_blocking(move || egs_download::relocate::scan(&dir))
        .await
        .map_err(|_| UiError::internal("move scan"))?
        .map_err(|e| err("move", format!("Couldn't read the game folder: {e}")))?;
    Ok(MovePreview {
        current_path: path,
        bytes: tree.bytes() as f64,
        egl_managed: game.egl_managed,
    })
}

pub async fn move_installed(
    app: &AppHandle,
    app_name: &str,
    folder: &str,
    confirm_external: bool,
) -> Result<MoveResult, UiError> {
    use egs_download::relocate;

    let state = app
        .try_state::<AppState>()
        .ok_or_else(|| UiError::internal("app state"))?;
    let downloads = app
        .try_state::<DownloadsState>()
        .ok_or_else(|| UiError::internal("downloads state"))?;
    let game = state
        .cache
        .game_install(app_name)?
        .ok_or_else(|| err("unknown", "Unknown game."))?;
    let from = PathBuf::from(
        game.install_path
            .clone()
            .filter(|_| game.state != "installing" && game.state != "notInstalled")
            .ok_or_else(|| err("notInstalled", "This game isn't installed."))?,
    );
    let to = PathBuf::from(folder.trim());
    if !to.is_absolute() {
        return Err(err(
            "folder",
            "Choose a full folder path, like D:\\Games\\My Game.",
        ));
    }
    if relocate::is_within(&to, &from) || relocate::is_within(&from, &to) {
        return Err(err(
            "folder",
            "Choose a folder outside the game's current folder.",
        ));
    }
    downloads.guard(&from)?;
    downloads.guard(&to)?;
    if game.egl_managed && !confirm_external {
        return Err(err(
            "confirmExternal",
            format!(
                "{} is shared with the Epic Games Launcher, which won't find the game after the move. Confirm to move it.",
                from.display()
            ),
        ));
    }
    let (_slot, cancel) = downloads.begin_move(app_name)?;
    if state.cache.active_download(app_name)?.is_some() {
        return Err(err("busy", "Finish or cancel this game's download first."));
    }

    let scan_from = from.clone();
    let total = tokio::task::spawn_blocking(move || relocate::scan(&scan_from))
        .await
        .map_err(|_| UiError::internal("move scan"))?
        .map_err(|e| err("move", format!("Couldn't read the game folder: {e}")))?
        .bytes();
    if !relocate::same_volume(&from, &to) {
        if let Some(free) = crate::disk::free_bytes(&to) {
            if free < total + (1 << 30) {
                return Err(err(
                    "diskFull",
                    format!(
                        "Not enough space on that drive: {:.1} GB needed, {:.1} GB free.",
                        total as f64 / 1e9,
                        free as f64 / 1e9
                    ),
                ));
            }
        }
    }

    let progress = Arc::new(AtomicU64::new(0));
    let ticker = {
        let (app, progress, name) = (app.clone(), progress.clone(), app_name.to_owned());
        tauri::async_runtime::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_millis(250)).await;
                let _ = MoveProgress {
                    app_name: name.clone(),
                    bytes_done: progress.load(Ordering::Relaxed) as f64,
                    bytes_total: total as f64,
                }
                .emit(&app);
            }
        })
    };
    let started = Instant::now();
    let result = {
        let (from, to, progress) = (from.clone(), to.clone(), progress.clone());
        tokio::task::spawn_blocking(move || {
            relocate::move_dir(&from, &to, false, &progress, &cancel)
        })
        .await
    };
    ticker.abort();
    let still_there = format!("The game is still in {}.", from.display());
    let moved = match result.map_err(|_| UiError::internal("move task"))? {
        Ok(Some(m)) => m,
        Ok(None) => return Err(err("cancelled", format!("Move cancelled. {still_there}"))),
        Err(egs_download::DlError::Io(e)) if e.contains("os error 5") || e.contains("os error 32") => {
            return Err(err(
                "inUse",
                format!("A file in the game folder is in use. Close the game and anything using its folder, then try again. {still_there}"),
            ))
        }
        Err(e) => return Err(err("move", format!("Couldn't move the game: {e}. {still_there}"))),
    };
    let new_path = to.to_string_lossy().into_owned();
    state.cache.set_install_moved(app_name, &new_path)?;
    tracing::info!(
        files = moved.files,
        mb = moved.bytes / 1_000_000,
        renamed = moved.renamed,
        left_behind = moved.left_behind,
        ms = started.elapsed().as_millis() as u64,
        "moved install"
    );
    emit_changed(app, "move");
    Ok(MoveResult {
        path: new_path,
        renamed: moved.renamed,
        left_behind: moved.left_behind,
    })
}

/// Dev tooling (`--dev-action <install|verify|repair|uninstall>:<app>` or `move:<app>=<folder>`):
/// same code paths as the buttons.
pub async fn dev_action(app: AppHandle, spec: String) {
    let Some((action, app_name)) = spec.split_once(':') else {
        return;
    };
    let result = match action {
        "install" => queue_install(&app, app_name, None, vec![])
            .await
            .map(|id| format!("queued {id}")),
        "verify" => verify_installed(&app, app_name).await.map(|r| {
            format!(
                "checked {} files, {} problems {:?}",
                r.files_checked,
                r.problems.len(),
                r.problems
            )
        }),
        "repair" => queue_repair(&app, app_name)
            .await
            .map(|_| "repair queued".to_owned()),
        "uninstall" => uninstall_installed(&app, app_name, false)
            .await
            .map(|_| "uninstalled".to_owned()),
        "store" => crate::store::dev_check(&app, app_name).await,
        "saves" | "saves-up" | "saves-down" | "saves-sync" => {
            crate::saves::dev_action(&app, action, app_name).await
        }
        "account" => crate::account::dev_check(&app).await,
        "app-update" => crate::updater::dev_action(&app, app_name).await,
        "page" => match serde_json::from_str::<crate::auth::EpicPage>(&format!("\"{app_name}\"")) {
            Ok(page) => {
                let state = app.state::<AppState>();
                crate::auth::open_epic_page(app.clone(), state, page)
                    .await
                    .map(|_| "opened".to_owned())
            }
            Err(_) => Err(err("dev", "unknown page")),
        },
        "open" => crate::store::dev_open(&app, app_name).await,
        "launch" => crate::launch::launch(&app, app_name, &Default::default())
            .await
            .map(|_| "launched".to_owned()),
        "move" => match app_name.split_once('=') {
            Some((name, folder)) => move_installed(&app, name, folder, false)
                .await
                .map(|r| format!("moved to {} (renamed: {})", r.path, r.renamed)),
            None => Err(err("dev", "move:<app>=<folder>")),
        },
        _ => Err(err("dev", "unknown dev action")),
    };
    match result {
        Ok(msg) => tracing::info!(%action, %msg, "dev action done"),
        Err(e) => tracing::warn!(%action, error = %e.message, "dev action failed"),
    }
}

/// Apply the "Download limit" setting (MB/s, 0 = unlimited).
pub fn apply_speed_limit(app: &AppHandle) {
    let (Some(state), Some(dl)) = (
        app.try_state::<AppState>(),
        app.try_state::<DownloadsState>(),
    ) else {
        return;
    };
    let mbps = settings_value(&state.cache, "downloads.limitMbps")
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(0.0);
    dl.limiter.set_rate((mbps * 1_000_000.0) as u64);
}

/// Dev tooling (`--dev-probe-sizes`): exact sizes of every installable game, from
/// Epic's manifests only (no game data). Written to logs/size-probe.json.
pub async fn probe_sizes(app: AppHandle) {
    let (Some(state), Some(auth)) = (app.try_state::<AppState>(), app.try_state::<AuthState>())
    else {
        return;
    };
    let Ok(snapshot) = state.cache.snapshot() else {
        return;
    };
    let candidates: Vec<String> = snapshot
        .games
        .iter()
        .filter(|g| g.installable && g.third_party.is_none())
        .map(|g| g.app_name.clone())
        .collect();
    let sem = Arc::new(tokio::sync::Semaphore::new(2));
    let mut set = tokio::task::JoinSet::new();
    for app_name in candidates {
        let (session, cache, sem) = (auth.session.clone(), state.cache.clone(), sem.clone());
        set.spawn(async move {
            let _p = sem.acquire_owned().await;
            let game = cache.game_install(&app_name).ok().flatten()?;
            let build = load_build(&session, &game).await.ok()?;
            let sd = match &build.info.sdmeta_url {
                Some(url) => downloads::fetch_sdmeta(session.api(), url).await.ok(),
                None => None,
            };
            let tags = selected_tags(&sd, &[]);
            Some(serde_json::json!({
                "appName": game.app_name,
                "title": game.title,
                "installMB": build.manifest.install_size(&tags) / 1_000_000,
                "downloadMB": build.manifest.download_size(&tags) / 1_000_000,
                "files": build.manifest.files.len(),
                "featureLevel": build.manifest.meta.feature_level,
                "signedUrls": build.info.use_signed_url,
                "encrypted": !build.info.secrets.is_empty(),
                "components": sd.map(|s| s.data.len()).unwrap_or(0),
                "launchExe": build.manifest.meta.launch_exe,
            }))
        });
    }
    let mut rows = Vec::new();
    while let Some(r) = set.join_next().await {
        if let Ok(Some(v)) = r {
            rows.push(v);
        }
    }
    rows.sort_by_key(|v| v["downloadMB"].as_u64().unwrap_or(u64::MAX));
    let out = state.paths.logs.join("size-probe.json");
    let _ = std::fs::write(&out, serde_json::to_vec_pretty(&rows).unwrap_or_default());
    tracing::info!(games = rows.len(), "size probe written");
}
