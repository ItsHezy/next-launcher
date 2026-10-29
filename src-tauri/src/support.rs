//! Settings helpers (Phase 7): folder picker, Support checks, logs folder, app info,
//! hidden games.

use std::path::{Path, PathBuf};
use std::time::Instant;

use egs_api::Retry;
use serde::Serialize;
use tauri::{AppHandle, Manager, State};

use crate::auth::AuthState;
use crate::commands::{settings_value, AppState, UiError};
use crate::install::DownloadsState;

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CheckResult {
    pub ok: bool,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub version: String,
    pub data_dir: String,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct HiddenGame {
    pub app_name: String,
    pub title: String,
}

fn err(code: &str, message: impl Into<String>) -> UiError {
    UiError {
        code: code.into(),
        message: message.into(),
    }
}

#[tauri::command]
#[specta::specta]
pub fn app_info(state: State<'_, AppState>) -> AppInfo {
    AppInfo {
        version: env!("CARGO_PKG_VERSION").into(),
        data_dir: state.paths.root.to_string_lossy().into_owned(),
    }
}

/// The folder new games install into (setting, else the default), as the user sees it.
fn install_root(app: &AppHandle) -> PathBuf {
    if let Some(root) = app
        .try_state::<DownloadsState>()
        .and_then(|d| d.install_root.clone())
    {
        return root;
    }
    app.try_state::<AppState>()
        .and_then(|s| settings_value(&s.cache, "downloads.installDir"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("C:\\Games"))
}

/// Can Epic be reached? Signed in: a token check; otherwise a public endpoint.
#[tauri::command]
#[specta::specta]
pub async fn check_connection(auth: State<'_, AuthState>) -> Result<CheckResult, UiError> {
    let t = Instant::now();
    let api = auth.session.api();
    let result = match auth.session.access_token().await {
        Ok(access) => egs_api::oauth::verify(api, &access, false)
            .await
            .map(|_| ()),
        Err(_) => api
            .send(
                api.http().get(format!(
                    "{}/launcher/api/public/distributionpoints",
                    api.hosts().launcher
                )),
                Retry::Never,
            )
            .await
            .map(|_| ()),
    };
    let ms = t.elapsed().as_millis();
    Ok(match result {
        Ok(()) => CheckResult {
            ok: true,
            message: format!("Epic's servers answered in {ms} ms."),
        },
        Err(e) => {
            tracing::warn!(error = %e, "connection check failed");
            CheckResult {
                ok: false,
                message: format!("Epic's servers couldn't be reached: {e}"),
            }
        }
    })
}

/// Can games be written to the install folder? Writes and deletes one small file there
/// (or in its nearest existing parent; the folder itself is never created by a check).
#[tauri::command]
#[specta::specta]
pub async fn check_install_folder(app: AppHandle) -> Result<CheckResult, UiError> {
    let root = install_root(&app);
    let mut probe = root.as_path();
    while !probe.exists() {
        match probe.parent() {
            Some(p) => probe = p,
            None => {
                return Ok(CheckResult {
                    ok: false,
                    message: format!("{} isn't on a drive that exists.", root.display()),
                })
            }
        }
    }
    let file = probe.join(".next-launcher-write-test");
    let res = std::fs::write(&file, b"ok").and_then(|_| std::fs::remove_file(&file));
    Ok(match res {
        Ok(()) => CheckResult {
            ok: true,
            message: format!("Games can be written to {}.", root.display()),
        },
        Err(e) => CheckResult {
            ok: false,
            message: format!("Can't write to {}: {e}", probe.display()),
        },
    })
}

#[tauri::command]
#[specta::specta]
pub async fn check_disk(app: AppHandle) -> Result<CheckResult, UiError> {
    let root = install_root(&app);
    Ok(match crate::disk::free_bytes(&root) {
        Some(free) => CheckResult {
            ok: free > 10 << 30,
            message: format!(
                "{:.1} GB free on the drive of {}.",
                free as f64 / 1e9,
                root.display()
            ),
        },
        None => CheckResult {
            ok: false,
            message: format!("Couldn't read free space for {}.", root.display()),
        },
    })
}

#[tauri::command]
#[specta::specta]
pub fn open_logs_folder(state: State<'_, AppState>) -> Result<(), UiError> {
    let dir = state.paths.logs.to_string_lossy().into_owned();
    egs_launch::process::open_with_shell(&dir).map_err(|e| err("open", e.to_string()))
}

#[tauri::command]
#[specta::specta]
pub fn hidden_games(state: State<'_, AppState>) -> Result<Vec<HiddenGame>, UiError> {
    Ok(state
        .cache
        .hidden_games()?
        .into_iter()
        .map(|(app_name, title)| HiddenGame { app_name, title })
        .collect())
}

#[tauri::command]
#[specta::specta]
pub fn set_hidden(
    app: AppHandle,
    state: State<'_, AppState>,
    app_name: String,
    hidden: bool,
) -> Result<(), UiError> {
    state.cache.set_hidden(&app_name, hidden)?;
    crate::library::notify_changed(&app, "hidden");
    Ok(())
}

/// Let the user pick a folder (Windows' own folder dialog). `None` if cancelled.
#[tauri::command]
#[specta::specta]
pub async fn pick_folder(app: AppHandle, title: String) -> Result<Option<String>, UiError> {
    let owner = app
        .get_webview_window("main")
        .and_then(|w| w.hwnd().ok())
        .map(|h| h.0 as isize)
        .unwrap_or(0);
    let picked = tokio::task::spawn_blocking(move || browse_for_folder(owner, &title))
        .await
        .map_err(|_| UiError::internal("folder dialog"))?;
    Ok(picked.map(|p| p.to_string_lossy().into_owned()))
}

#[cfg(windows)]
#[allow(unsafe_code)]
fn browse_for_folder(owner: isize, title: &str) -> Option<PathBuf> {
    use std::os::windows::ffi::OsStringExt;
    use windows_sys::Win32::System::Com::{
        CoInitializeEx, CoTaskMemFree, CoUninitialize, COINIT_APARTMENTTHREADED,
    };
    use windows_sys::Win32::UI::Shell::{
        SHBrowseForFolderW, SHGetPathFromIDListW, BIF_NEWDIALOGSTYLE, BIF_RETURNONLYFSDIRS,
        BROWSEINFOW,
    };

    let title_w: Vec<u16> = title.encode_utf16().chain(std::iter::once(0)).collect();
    let mut name = [0u16; 260];
    // SAFETY: COM is initialised for this (blocking, dedicated) thread and released below.
    let com = unsafe { CoInitializeEx(std::ptr::null(), COINIT_APARTMENTTHREADED as u32) };
    // SAFETY: an all-zero BROWSEINFOW is valid; the fields the call reads are set below and
    // every buffer outlives the call.
    let mut info: BROWSEINFOW = unsafe { std::mem::zeroed() };
    info.hwndOwner = owner as _;
    info.pszDisplayName = name.as_mut_ptr();
    info.lpszTitle = title_w.as_ptr();
    info.ulFlags = BIF_RETURNONLYFSDIRS | BIF_NEWDIALOGSTYLE;
    // SAFETY: `info` is fully initialised; the returned id list is freed with CoTaskMemFree.
    let pidl = unsafe { SHBrowseForFolderW(&info) };
    let mut out = None;
    if !pidl.is_null() {
        let mut path = [0u16; 1024];
        // SAFETY: `pidl` came from SHBrowseForFolderW; `path` holds MAX_PATH+ characters.
        if unsafe { SHGetPathFromIDListW(pidl, path.as_mut_ptr()) } != 0 {
            let len = path.iter().position(|&c| c == 0).unwrap_or(path.len());
            out = Some(PathBuf::from(std::ffi::OsString::from_wide(&path[..len])));
        }
        // SAFETY: freeing the id list the shell allocated for us, once.
        unsafe { CoTaskMemFree(pidl as *const _) };
    }
    if com >= 0 {
        // SAFETY: balances the successful CoInitializeEx above.
        unsafe { CoUninitialize() };
    }
    out.filter(|p: &PathBuf| Path::new(p).is_absolute())
}

#[cfg(not(windows))]
fn browse_for_folder(_owner: isize, _title: &str) -> Option<PathBuf> {
    None
}
