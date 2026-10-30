//! Launch (Phase 5): command line, ownership token, start, running state, playtime.
//! Spec: docs/protocol/launch.md. Argument building and process tracking: `egs-launch`.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use egs_api::launch::{self as api_launch, PlaySession};
use egs_launch::third_party::{self, Handoff, Store};
use egs_launch::{ExchangeCode, LaunchSpec};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};
use tauri_specta::Event;

use crate::auth::AuthState;
use crate::commands::{settings_value, AppState, UiError};
use crate::install::{self, DownloadsState};
use crate::library::LibraryChanged;

/// How often a running game is checked. Nothing is polled while no game runs.
const POLL: Duration = Duration::from_secs(2);

/// A game started or stopped.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
pub struct GameRunning {
    pub app_name: String,
    pub running: bool,
}

struct Running {
    started_ms: i64,
    ovt: Option<PathBuf>,
}

#[derive(Default)]
pub struct LaunchState {
    running: Mutex<HashMap<String, Running>>,
    /// Downloads paused because a game started; resumed when the last game exits.
    auto_paused: Mutex<HashSet<i64>>,
}

impl LaunchState {
    pub fn any_running(&self) -> bool {
        self.running.lock().map(|r| !r.is_empty()).unwrap_or(false)
    }

    pub fn running_names(&self) -> Vec<String> {
        self.running
            .lock()
            .map(|r| r.keys().cloned().collect())
            .unwrap_or_default()
    }

    pub fn is_running(&self, app_name: &str) -> bool {
        self.running
            .lock()
            .map(|r| r.contains_key(app_name))
            .unwrap_or(false)
    }
}

#[derive(Debug, Clone, Default, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LaunchOptions {
    /// Start without contacting Epic (no exchange code, no ownership token).
    pub offline: bool,
    /// Answer to the prerequisites question: `run` or `skip`.
    pub prereq: Option<String>,
    /// Answer to a cloud-save conflict: `download`, `upload` or `skip`.
    pub saves: Option<String>,
}

fn err(code: &str, message: impl Into<String>) -> UiError {
    UiError {
        code: code.into(),
        message: message.into(),
    }
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Catalog `customAttributes.<key>.value`.
fn attr(meta: &Option<serde_json::Value>, key: &str) -> Option<String> {
    meta.as_ref()?
        .get("customAttributes")?
        .get(key)?
        .get("value")?
        .as_str()
        .map(str::to_owned)
}

fn emit_changed(app: &AppHandle, reason: &str) {
    let _ = LibraryChanged {
        reason: reason.to_owned(),
    }
    .emit(app);
}

#[tauri::command]
#[specta::specta]
pub fn running_games(launch: State<'_, LaunchState>) -> Vec<String> {
    launch
        .running
        .lock()
        .map(|r| r.keys().cloned().collect())
        .unwrap_or_default()
}

#[tauri::command]
#[specta::specta]
pub async fn launch_game(
    app: AppHandle,
    app_name: String,
    options: LaunchOptions,
) -> Result<(), UiError> {
    let result = launch(&app, &app_name, &options).await;
    if let Err(e) = &result {
        tracing::info!(code = %e.code, "launch not started");
    }
    result
}

pub(crate) async fn launch(
    app: &AppHandle,
    app_name: &str,
    options: &LaunchOptions,
) -> Result<(), UiError> {
    let state = app
        .try_state::<AppState>()
        .ok_or_else(|| UiError::internal("app state"))?;
    let auth = app
        .try_state::<AuthState>()
        .ok_or_else(|| UiError::internal("auth state"))?;
    let downloads = app
        .try_state::<DownloadsState>()
        .ok_or_else(|| UiError::internal("downloads state"))?;
    let launch_state = app
        .try_state::<LaunchState>()
        .ok_or_else(|| UiError::internal("launch state"))?;

    let game = state
        .cache
        .game_install(app_name)?
        .ok_or_else(|| err("unknown", "That game isn't in your library."))?;
    let meta: Option<serde_json::Value> = game
        .meta_json
        .as_deref()
        .and_then(|j| serde_json::from_str(j).ok());
    let installed = matches!(game.state.as_str(), "installed" | "updateAvailable");

    // Account: the live session, or the cached one for an offline start.
    let session = auth.session.current().await;
    let (account_id, display_name) = match &session {
        Some(s) => (s.account_id.clone(), s.display_name.clone()),
        None => {
            let a = state
                .cache
                .snapshot()?
                .account
                .ok_or_else(|| err("notSignedIn", "Sign in to play."))?;
            (a.account_id, a.display_name)
        }
    };
    let locale = settings_value(&state.cache, "general.language").unwrap_or_else(|| "en-US".into());

    if !installed {
        let provider = attr(&meta, "ThirdPartyManagedProvider")
            .or_else(|| attr(&meta, "ThirdPartyManagedApp"));
        if let Some(store) = provider.as_deref().and_then(Store::from_attr) {
            return hand_off(
                app,
                &game,
                &meta,
                store,
                &account_id,
                &display_name,
                &locale,
            )
            .await;
        }
        return Err(err("notInstalled", "Install this game first."));
    }

    if launch_state.is_running(app_name) {
        return Err(err(
            "running",
            format!("{} is already running.", game.title),
        ));
    }
    downloads.ensure_not_moving(app_name)?;
    if let Some(id) = state.cache.active_download(app_name)? {
        let job = state.cache.next_download_by_id(id)?;
        if job.is_some_and(|j| j.state != "paused") {
            return Err(err(
                "busy",
                "This game is downloading. Wait for it to finish or pause it.",
            ));
        }
    }
    let install_dir = PathBuf::from(
        game.install_path
            .clone()
            .ok_or_else(|| err("notInstalled", "Install this game first."))?,
    );
    if egs_download::commit_interrupted(&install_dir) {
        return Err(err(
            "busy",
            format!("An update to {} was interrupted while it was being applied. It finishes when the download resumes; play after that.", game.title),
        ));
    }
    let manifest = install::installed_manifest(&state, &game).await?;

    // Prerequisites (e.g. a Visual C++ runtime) shipped in the game folder: ask once.
    let prereq_path = manifest.meta.prereq_path.trim().to_owned();
    if !prereq_path.is_empty() {
        let ids = if manifest.meta.prereq_ids.is_empty() {
            prereq_path.clone()
        } else {
            manifest.meta.prereq_ids.join(",")
        };
        let key = format!("internal.prereq.{app_name}");
        if settings_value(&state.cache, &key).as_deref() != Some(ids.as_str()) {
            let name = if manifest.meta.prereq_name.is_empty() {
                "a required component".to_owned()
            } else {
                manifest.meta.prereq_name.clone()
            };
            match options.prereq.as_deref() {
                None => {
                    return Err(err(
                        "prereq",
                        format!(
                            "{} needs {name}. Install it now? Windows may ask for permission.",
                            game.title
                        ),
                    ))
                }
                Some("run") => {
                    run_prereq(
                        &install_dir,
                        &prereq_path,
                        &manifest.meta.prereq_args,
                        &name,
                    )
                    .await?;
                    state
                        .cache
                        .set_setting(&key, &serde_json::Value::String(ids))?;
                }
                _ => {} // skipped this time; asked again next launch
            }
        }
    }

    let can_run_offline = attr(&meta, "CanRunOffline").as_deref() != Some("false");
    let needs_token = attr(&meta, "OwnershipToken").as_deref() == Some("true");
    let offline = options.offline || session.is_none();
    if offline && !can_run_offline && !options.offline {
        return Err(err(
            "needsOnline",
            format!("{} needs to be online to start. Try anyway?", game.title),
        ));
    }

    // Cloud saves first ("Sync when a game starts"); a conflict comes back as a question.
    crate::saves::before_launch(app, app_name, options.saves.as_deref()).await?;

    // Exchange code immediately before the start (single use, 5 minutes, never logged).
    let mut code = None;
    if !offline {
        match auth.session.exchange_code().await {
            Ok(c) => code = Some(ExchangeCode(c.expose().to_owned())),
            Err(e) => {
                tracing::warn!(error = %e, "no exchange code for launch");
                if !can_run_offline {
                    return Err(err(
                        "needsOnline",
                        format!("{} needs to be online to start, and Epic couldn't be reached. Try anyway?", game.title),
                    ));
                }
                return Err(err(
                    "offline",
                    format!("Couldn't reach Epic. Start {} offline?", game.title),
                ));
            }
        }
    }

    let ovt = if needs_token {
        if offline {
            return Err(err(
                "needsOnline",
                format!(
                    "Epic has to confirm you own {} before it starts, which needs a connection.",
                    game.title
                ),
            ));
        }
        Some(ownership_token(&state, &auth, &game, &account_id).await?)
    } else {
        None
    };

    // EOS deployment id for games that use Epic Online Services (best effort).
    let deployment_id = if offline {
        None
    } else {
        deployment_id(&auth, &game).await
    };

    let user_args =
        settings_value(&state.cache, &format!("game.{app_name}.args")).unwrap_or_default();
    let extra = attr(&meta, "AdditionalCommandLine").unwrap_or_default();
    let spec = LaunchSpec {
        install_dir: &install_dir,
        launch_exe: &manifest.meta.launch_exe,
        launch_command: &manifest.meta.launch_command,
        additional_command_line: &extra,
        app_name,
        namespace: &game.namespace,
        display_name: &display_name,
        account_id: &account_id,
        locale: &locale,
        exchange_code: code.as_ref(),
        ownership_token: ovt.as_deref(),
        deployment_id: deployment_id.as_deref(),
        user_args: &user_args,
    };
    let built = egs_launch::build(&spec).map_err(|e| err("launch", e.to_string()))?;
    drop(code);
    let exe_names: Vec<String> = manifest
        .files
        .iter()
        .filter(|f| f.filename.to_ascii_lowercase().ends_with(".exe"))
        .filter_map(|f| {
            Path::new(&f.filename)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
        })
        .collect();
    let exe_label = built
        .exe
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    if !built.exe.is_file() {
        return Err(err(
            "launch",
            format!(
                "{exe_label} is missing from the game folder. Verify the game's files to fix it."
            ),
        ));
    }

    let started = tokio::task::spawn_blocking(move || egs_launch::process::start(&built))
        .await
        .map_err(|_| UiError::internal("launch task"))?
        .map_err(|e| {
            if let Some(p) = &ovt {
                let _ = std::fs::remove_file(p);
            }
            err("launch", format!("Couldn't start {}: {e}", game.title))
        })?;
    let started_ms = now_ms();
    tracing::info!(
        exe = %exe_label,
        offline,
        ownership_token = needs_token,
        elevated = started.elevated,
        deployment_id = deployment_id.is_some(),
        "game started"
    );
    if let Ok(mut r) = launch_state.running.lock() {
        r.insert(app_name.to_owned(), Running { started_ms, ovt });
    }
    state.cache.mark_played(app_name, started_ms / 1000)?;
    let _ = GameRunning {
        app_name: app_name.to_owned(),
        running: true,
    }
    .emit(app);
    emit_changed(app, "play");
    pause_downloads_for_play(app);

    let (app2, name) = (app.clone(), app_name.to_owned());
    let mut tracker = egs_launch::process::Tracker::new(&install_dir, exe_names, Some(started));
    std::thread::spawn(move || {
        while tracker.poll() {
            std::thread::sleep(POLL);
        }
        tauri::async_runtime::block_on(finished(&app2, &name));
    });
    Ok(())
}

/// Hand an owned game to the store that manages it (EA app, Ubisoft Connect).
async fn hand_off(
    app: &AppHandle,
    game: &egs_cache::GameInstall,
    meta: &Option<serde_json::Value>,
    store: Store,
    account_id: &str,
    display_name: &str,
    locale: &str,
) -> Result<(), UiError> {
    let auth = app
        .try_state::<AuthState>()
        .ok_or_else(|| UiError::internal("auth state"))?;
    let code = auth.session.exchange_code().await.map_err(|_| {
        err(
            "offline",
            format!("Opening {} needs a connection to Epic.", store.name()),
        )
    })?;
    let game_id = match store {
        Store::Ubisoft => attr(meta, "GameID").unwrap_or_else(|| game.app_name.clone()),
        Store::Ea => game.app_name.clone(),
    };
    let url = third_party::url(&Handoff {
        store,
        game_id: &game_id,
        exchange_code: code.expose(),
        display_name,
        account_id,
        locale,
        additional: &attr(meta, "AdditionalCommandLine").unwrap_or_default(),
    });
    let opened = tokio::task::spawn_blocking(move || egs_launch::process::open_with_shell(&url))
        .await
        .map_err(|_| UiError::internal("hand-off task"))?;
    match opened {
        Ok(()) => {
            tracing::info!(store = store.name(), "handed game to its store");
            Ok(())
        }
        Err(_) => Err(err(
            "noLauncher",
            format!("Install {} to play {}.", store.name(), game.title),
        )),
    }
}

async fn run_prereq(dir: &Path, rel: &str, args: &str, name: &str) -> Result<(), UiError> {
    let exe = egs_download::safe_join(dir, &rel.replace('\\', "/")).map_err(|_| {
        err(
            "prereq",
            "The installer path in this game's files is not safe.",
        )
    })?;
    let cwd = exe
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| dir.to_path_buf());
    let args = args.to_owned();
    let code =
        tokio::task::spawn_blocking(move || egs_launch::process::run_and_wait(&exe, &args, &cwd))
            .await
            .map_err(|_| UiError::internal("prereq task"))?
            .map_err(|e| err("prereq", format!("{name} couldn't be installed: {e}")))?;
    // 3010 = installed, restart required; 1638 = a newer version is already installed.
    if matches!(code, 0 | 3010 | 1638) {
        tracing::info!(code, "prerequisite installed");
        Ok(())
    } else {
        Err(err(
            "prereq",
            format!("{name} couldn't be installed (code {code})."),
        ))
    }
}

async fn ownership_token(
    state: &AppState,
    auth: &AuthState,
    game: &egs_cache::GameInstall,
    account_id: &str,
) -> Result<PathBuf, UiError> {
    let access = auth.session.access_token().await?;
    let bytes = api_launch::ownership_token(
        auth.session.api(),
        &access,
        account_id,
        &game.namespace,
        &game.catalog_item_id,
    )
    .await
    .map_err(|e| {
        tracing::warn!(error = %e, "ownership token refused");
        err(
            "notOwned",
            "Epic couldn't confirm you own this game, so it can't start.",
        )
    })?;
    let dir = state.paths.root.join("tmp");
    let _ = tokio::fs::create_dir_all(&dir).await;
    let path = dir.join(format!("{}{}.ovt", game.namespace, game.catalog_item_id));
    tokio::fs::write(&path, bytes)
        .await
        .map_err(|e| err("launch", format!("Couldn't save the ownership token: {e}")))?;
    Ok(path)
}

async fn deployment_id(auth: &AuthState, game: &egs_cache::GameInstall) -> Option<String> {
    let access = auth.session.access_token().await.ok()?;
    let info = tokio::time::timeout(
        Duration::from_secs(4),
        egs_api::downloads::build_info(
            auth.session.api(),
            &access,
            &game.namespace,
            &game.catalog_item_id,
            &game.app_name,
        ),
    )
    .await
    .ok()?
    .ok()?;
    info.deployment_id
}

/// "Pause downloads while playing": pause what's running and hold the queue.
fn pause_downloads_for_play(app: &AppHandle) {
    let (Some(state), Some(downloads), Some(launch)) = (
        app.try_state::<AppState>(),
        app.try_state::<DownloadsState>(),
        app.try_state::<LaunchState>(),
    ) else {
        return;
    };
    let enabled = settings_value(&state.cache, "downloads.pauseWhilePlaying")
        .map(|v| v != "false")
        .unwrap_or(true);
    if !enabled {
        return;
    }
    downloads.set_hold(true);
    let paused = downloads.pause_running();
    if !paused.is_empty() {
        tracing::info!(jobs = paused.len(), "downloads paused while playing");
        if let Ok(mut a) = launch.auto_paused.lock() {
            a.extend(paused);
        }
    }
}

fn resume_downloads_after_play(app: &AppHandle) {
    let (Some(state), Some(downloads), Some(launch)) = (
        app.try_state::<AppState>(),
        app.try_state::<DownloadsState>(),
        app.try_state::<LaunchState>(),
    ) else {
        return;
    };
    if launch.any_running() {
        return;
    }
    downloads.set_hold(false);
    let ids: Vec<i64> = launch
        .auto_paused
        .lock()
        .map(|mut a| a.drain().collect())
        .unwrap_or_default();
    for id in &ids {
        let _ = state.cache.set_download_state(*id, "queued", None);
    }
    if !ids.is_empty() {
        tracing::info!(jobs = ids.len(), "downloads resumed after playing");
    }
    emit_changed(app, "download");
    install::kick(app);
}

/// The game and everything it started have exited.
async fn finished(app: &AppHandle, app_name: &str) {
    let Some(launch) = app.try_state::<LaunchState>() else {
        return;
    };
    let Some(run) = launch
        .running
        .lock()
        .ok()
        .and_then(|mut r| r.remove(app_name))
    else {
        return;
    };
    end_session(app, app_name, &run, now_ms()).await;
    crate::saves::after_exit(app, app_name).await;
    let _ = GameRunning {
        app_name: app_name.to_owned(),
        running: false,
    }
    .emit(app);
    emit_changed(app, "play");
    resume_downloads_after_play(app);
}

async fn end_session(app: &AppHandle, app_name: &str, run: &Running, end_ms: i64) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    if let Some(p) = &run.ovt {
        let _ = std::fs::remove_file(p);
    }
    let secs = ((end_ms - run.started_ms) / 1000).max(0);
    let _ = state.cache.add_playtime(app_name, secs);
    tracing::info!(secs, "game exited");
    let artifact = state
        .cache
        .game_install(app_name)
        .ok()
        .flatten()
        .and_then(|g| g.artifact_id);
    let (Some(artifact), Ok(machine)) = (artifact, state.cache.machine_id()) else {
        return;
    };
    let session = PlaySession::whole(&machine, &artifact, run.started_ms, end_ms);
    if report(app, &session).await.is_ok() {
        tracing::info!("playtime reported");
    } else {
        if let Ok(v) = serde_json::to_value(&session) {
            let _ = state.cache.queue_playtime_report(&v);
        }
        tracing::info!("playtime report queued for later");
    }
}

async fn report(app: &AppHandle, session: &PlaySession) -> Result<(), ()> {
    let auth = app.try_state::<AuthState>().ok_or(())?;
    let account = auth.session.current().await.ok_or(())?;
    let access = auth.session.access_token().await.map_err(|_| ())?;
    api_launch::report_playtime(auth.session.api(), &access, &account.account_id, session)
        .await
        .map_err(|e| tracing::warn!(error = %e, "playtime report failed"))
}

/// Send playtime reports kept from earlier sessions (after sign-in / restore).
pub async fn flush_playtime(app: &AppHandle) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let Ok(pending) = state.cache.pending_playtime_reports() else {
        return;
    };
    for (id, payload) in pending {
        let Ok(session) = serde_json::from_value::<PlaySession>(payload) else {
            let _ = state.cache.drop_event(id);
            continue;
        };
        if report(app, &session).await.is_err() {
            return; // try again next time
        }
        let _ = state.cache.drop_event(id);
        tracing::info!("queued playtime report sent");
    }
}

/// The app is closing while games run: keep their playtime so far and queue the reports.
pub fn on_exit(app: &AppHandle) {
    let (Some(state), Some(launch)) = (app.try_state::<AppState>(), app.try_state::<LaunchState>())
    else {
        return;
    };
    let Ok(mut running) = launch.running.lock() else {
        return;
    };
    let end = now_ms();
    for (app_name, run) in running.drain() {
        if let Some(p) = &run.ovt {
            let _ = std::fs::remove_file(p);
        }
        let _ = state
            .cache
            .add_playtime(&app_name, ((end - run.started_ms) / 1000).max(0));
        let artifact = state
            .cache
            .game_install(&app_name)
            .ok()
            .flatten()
            .and_then(|g| g.artifact_id);
        if let (Some(artifact), Ok(machine)) = (artifact, state.cache.machine_id()) {
            let s = PlaySession::whole(&machine, &artifact, run.started_ms, end);
            if let Ok(v) = serde_json::to_value(&s) {
                let _ = state.cache.queue_playtime_report(&v);
            }
        }
    }
}
