#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod account;
mod auth;
mod autostart;
mod commands;
mod console;
mod disk;
mod import;
mod install;
mod launch;
mod library;
mod logging;
mod notify;
mod paths;
mod protocol;
mod redact;
mod saves;
mod startup;
mod store;
mod support;
mod tray;
mod updater;
mod webpane;

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Duration;

use egs_cache::Cache;
use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};

use crate::commands::AppState;
use crate::paths::Paths;

/// Window background, identical to `--bg` so nothing flashes before CSS loads.
const BG: tauri::window::Color = tauri::window::Color(0x26, 0x28, 0x2A, 0xFF);
/// Show the window even if the UI never reports its first paint.
const SHOW_TIMEOUT: Duration = Duration::from_millis(3000);

struct Args {
    bench: bool,
    bench_scroll: bool,
    /// Dev tooling for screenshots: `<route>[:<section>|:profile|:wallet]`.
    dev_route: Option<String>,
    /// Development guard: refuse install/update/uninstall writes outside this folder.
    dev_install_root: Option<PathBuf>,
    /// Dev tooling: log exact sizes of all installable games after the first sync.
    dev_probe_sizes: bool,
    /// Dev tooling: `<install|verify|repair|uninstall>:<app>` after the first sync.
    dev_action: Option<String>,
    mock_games: usize,
    data_dir: Option<PathBuf>,
    reset: bool,
    /// Started by Windows at sign-in (the Run entry passes it).
    autostart: bool,
}

fn parse_args() -> Args {
    let mut args = Args {
        bench: false,
        bench_scroll: false,
        dev_route: None,
        dev_install_root: None,
        dev_probe_sizes: false,
        dev_action: None,
        // Mock data only on request (benchmarks); the real library comes from sync.
        mock_games: 0,
        data_dir: None,
        reset: false,
        autostart: false,
    };
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--bench-startup" => args.bench = true,
            "--bench-scroll" => args.bench_scroll = true,
            "--reset-data" => args.reset = true,
            "--autostart" => args.autostart = true,
            "--mock-games" => {
                if let Some(n) = it.next().and_then(|v| v.parse().ok()) {
                    args.mock_games = n;
                }
            }
            "--data-dir" => args.data_dir = it.next().map(PathBuf::from),
            "--dev-route" => args.dev_route = it.next(),
            "--dev-install-root" => args.dev_install_root = it.next().map(PathBuf::from),
            "--dev-probe-sizes" => args.dev_probe_sizes = true,
            "--dev-action" => args.dev_action = it.next(),
            _ => {}
        }
    }
    args
}

fn specta_builder() -> tauri_specta::Builder<tauri::Wry> {
    tauri_specta::Builder::<tauri::Wry>::new()
        .commands(tauri_specta::collect_commands![
            commands::app_ready,
            commands::bench_report,
            commands::startup_timeline,
            commands::get_snapshot,
            commands::get_settings,
            commands::set_setting,
            commands::set_pinned,
            commands::window_action,
            auth::auth_status,
            auth::sign_out,
            library::refresh_library,
            library::sync_status,
            import::egl_import_scan,
            import::egl_adopt,
            install::prepare_install,
            install::start_install,
            install::start_update,
            install::pause_download,
            install::resume_download,
            install::cancel_download,
            install::prepare_move,
            install::move_game,
            install::cancel_move,
            install::verify_game,
            install::repair_game,
            install::uninstall_game,
            launch::launch_game,
            launch::running_games,
            store::store_featured,
            store::store_browse,
            store::store_search,
            store::store_product,
            account::account_details,
            account::update_account,
            account::wallet_summary,
            account::redeem_code,
            account::account_capabilities,
            support::app_info,
            support::check_connection,
            support::check_install_folder,
            support::check_disk,
            support::open_logs_folder,
            support::hidden_games,
            support::set_hidden,
            support::pick_folder,
            support::read_bundled,
            updater::check_app_update,
            updater::install_app_update,
            webpane::web_pane_open,
            webpane::web_pane_bounds,
            webpane::web_pane_visible,
            webpane::web_pane_close,
            saves::cloud_save_status,
            saves::cloud_save_games,
            saves::cloud_save_sync,
        ])
        .events(tauri_specta::collect_events![
            auth::AuthChanged,
            library::LibraryChanged,
            library::SyncStatusChanged,
            install::DownloadProgress,
            install::MoveProgress,
            launch::GameRunning,
            account::AccountChanged,
            saves::SaveSync,
            updater::AppUpdateAvailable,
            import::GamesImported
        ])
}

struct Boot {
    cache: Arc<Cache>,
    json: String,
    auth: auth::AuthStatus,
}

/// What the UI should show about the account before any network call.
fn initial_auth(cache: &Cache, snapshot: &egs_cache::LibrarySnapshot) -> auth::AuthStatus {
    match cache.signed_in_account() {
        Ok(Some(_)) => auth::AuthStatus::Restoring {
            display_name: snapshot
                .account
                .as_ref()
                .map(|a| a.display_name.clone())
                .unwrap_or_default(),
        },
        _ => auth::AuthStatus::SignedOut { reason: None },
    }
}

/// API client with the launcher identity (docs/protocol/README.md "Client identity").
/// Explicit proxy from Settings → Network (applies on start; WebView2 windows use the
/// Windows proxy settings).
fn proxy_setting(cache: &Cache) -> Option<String> {
    let on = commands::settings_value(cache, "network.proxyEnabled").as_deref() == Some("true");
    let addr = commands::settings_value(cache, "network.proxyAddress")?;
    let addr = addr.trim();
    let ok = addr.starts_with("http://") || addr.starts_with("https://");
    if on && !ok {
        tracing::warn!("proxy address ignored: it must start with http:// or https://");
    }
    (on && ok).then(|| addr.to_owned())
}

fn api_client(proxy: Option<String>) -> anyhow::Result<egs_api::Client> {
    let egl_version = std::env::var_os("PROGRAMDATA")
        .and_then(|p| egs_api::detect_egl_version(std::path::Path::new(&p)))
        .unwrap_or_else(|| egs_api::DEFAULT_EGL_VERSION.to_owned());
    let ua = egs_api::launcher_user_agent(&egl_version);
    tracing::info!(egl_version = %egl_version, "API identity");
    let mut cfg = egs_api::ClientConfig::new(ua);
    if proxy.is_some() {
        tracing::info!("using the proxy from Settings");
    }
    cfg.proxy = proxy;
    Ok(egs_api::Client::new(cfg)?)
}

/// Open the cache and serialise everything the first paint needs.
fn boot(
    paths: &Paths,
    mock_games: usize,
    bench: bool,
    bench_scroll: bool,
    dev_route: Option<String>,
) -> anyhow::Result<Boot> {
    let cache = Cache::open(&paths.db)?;
    startup::mark("cache_open");
    if mock_games > 0 {
        cache.seed_mock_if_empty(mock_games)?;
    }
    let snapshot = cache.snapshot()?;
    let settings = commands::settings_map(&cache)?;
    let auth = initial_auth(&cache, &snapshot);
    startup::mark("snapshot_read");
    let json = serde_json::to_string(&serde_json::json!({
        "snapshot": snapshot,
        "settings": settings,
        "auth": auth,
        "imgBase": protocol::base_url(),
        "bench": bench,
        "benchScroll": bench_scroll,
        "devRoute": dev_route,
        "bootError": null,
    }))?;
    startup::mark("boot_json");
    Ok(Boot {
        cache: Arc::new(cache),
        json,
        auth,
    })
}

/// Fallback when the database can't be opened: start empty, tell the user why.
fn boot_failed(err: &anyhow::Error) -> anyhow::Result<Boot> {
    tracing::error!(error = %err, "boot failed, starting with an empty in-memory cache");
    let cache = Cache::open_in_memory()?;
    let auth = auth::AuthStatus::SignedOut { reason: None };
    let json = serde_json::to_string(&serde_json::json!({
        "snapshot": cache.snapshot()?,
        "settings": {},
        "auth": auth,
        "imgBase": protocol::base_url(),
        "bench": false,
        "benchScroll": false,
        "devRoute": null,
        "bootError": "Your local library cache couldn't be opened, so the library is empty for now. Details are in the log.",
    }))?;
    Ok(Boot {
        cache: Arc::new(cache),
        json,
        auth,
    })
}

fn main() {
    startup::start();
    let args = parse_args();
    if args.bench || args.bench_scroll {
        console::attach_parent();
    }

    let paths = Paths::resolve(args.data_dir.clone());
    if let Err(e) = paths.ensure() {
        eprintln!("cannot create data directory {}: {e}", paths.root.display());
    }
    if args.reset {
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{suffix}", paths.db.display()));
        }
    }
    logging::init(&paths.logs, paths.debug_marker().exists());
    tracing::info!(version = env!("CARGO_PKG_VERSION"), "starting");

    // Read the cache while Tauri and WebView2 initialise.
    let boot_paths = paths.clone();
    let (mock_games, bench, bench_scroll) = (args.mock_games, args.bench, args.bench_scroll);
    let dev_route = args.dev_route.clone();
    let boot_thread =
        std::thread::spawn(move || boot(&boot_paths, mock_games, bench, bench_scroll, dev_route));

    let specta = specta_builder();
    let invoke = specta.invoke_handler();
    // Benchmarks and screenshot runs never touch the network.
    let no_network = args.bench || args.bench_scroll || args.dev_route.is_some();
    let dev_install_root = args.dev_install_root.clone();
    let dev_probe_sizes = args.dev_probe_sizes;
    let dev_action = args.dev_action.clone();
    let autostart = args.autostart;
    if let Some(root) = &dev_install_root {
        tracing::info!(root = %root.display(), "development install guard active");
    }
    let mut builder = tauri::Builder::default();
    // One running copy per user, except development runs (install guard on): they have their
    // own data folder and must neither hand over to nor wake an installed Next Launcher.
    if dev_install_root.is_none() {
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            tray::show_window(app)
        }));
    }
    let result = builder
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .invoke_handler(invoke)
        .register_asynchronous_uri_scheme_protocol(protocol::SCHEME, |ctx, request, responder| {
            let Some(state) = ctx.app_handle().try_state::<AppState>() else {
                return;
            };
            let cache = state.cache.clone();
            let path = request.uri().path().to_owned();
            std::thread::spawn(move || responder.respond(protocol::handle(&cache, &path)));
        })
        .setup(move |app| {
            startup::mark("tauri_setup");
            specta.mount_events(app);
            let boot = match boot_thread.join() {
                Ok(Ok(b)) => b,
                Ok(Err(e)) => boot_failed(&e)?,
                Err(_) => boot_failed(&anyhow::anyhow!("boot thread panicked"))?,
            };
            let proxy = proxy_setting(&boot.cache);
            // "Start minimized" applies when Windows starts the app at sign-in.
            let start_hidden = autostart
                && commands::settings_value(&boot.cache, "general.startMinimized").as_deref()
                    == Some("true");
            app.manage(AppState {
                cache: boot.cache,
                paths: paths.clone(),
                bench,
                no_network,
                dev_probe_sizes,
                dev_action: dev_action.clone(),
                shown: AtomicBool::new(false),
                start_hidden,
            });
            app.manage(auth::AuthState::new(api_client(proxy)?, boot.auth));
            app.manage(library::LibraryState::default());
            app.manage(install::DownloadsState::new(dev_install_root.clone()));
            app.manage(webpane::PaneState::default());
            app.manage(launch::LaunchState::default());
            app.manage(account::AccountState::default());
            app.manage(saves::SavesState::default());
            install::apply_speed_limit(app.handle());

            let main = WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()))
                .title("Next Launcher")
                .inner_size(1280.0, 800.0)
                .min_inner_size(960.0, 600.0)
                .center()
                .decorations(false)
                .shadow(true)
                .visible(false)
                .background_color(BG)
                .data_directory(paths.webview_ui.clone())
                .initialization_script(format!("window.__NL_BOOT__={};", boot.json));
            // Development (`NL_DEV_UI_DEBUG_PORT`): a debugger port on the app's own view only.
            // Epic's pages run in their own browser process without one, so Epic's bot checks
            // see them as they are for users.
            #[cfg(debug_assertions)]
            let main = match std::env::var("NL_DEV_UI_DEBUG_PORT") {
                Ok(port) if port.parse::<u16>().is_ok() => main.additional_browser_args(&format!(
                    "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection \
                     --remote-debugging-port={port}"
                )),
                _ => main,
            };
            let main = main.build()?;
            startup::mark("window_built");
            // Closing the main window quits.
            let quit = app.handle().clone();
            main.on_window_event(move |e| match e {
                // "Minimize to tray on close": Alt+F4 and the taskbar's Close hide instead.
                tauri::WindowEvent::CloseRequested { api, .. }
                    if commands::close_to_tray(&quit) =>
                {
                    api.prevent_close();
                    if let Some(w) = quit.get_webview_window("main") {
                        let _ = w.hide();
                    }
                }
                tauri::WindowEvent::Destroyed => quit.exit(0),
                _ => {}
            });
            // Keep the Run entry pointing at this exe (e.g. after an update moved it).
            if commands::settings_value(&app.state::<AppState>().cache, "general.runAtStartup")
                .as_deref()
                == Some("true")
            {
                autostart::apply(app.handle());
            }
            if let Err(e) = tray::setup(app.handle()) {
                tracing::warn!(error = %e, "tray icon unavailable");
            }

            let handle = app.handle().clone();
            std::thread::spawn(move || {
                std::thread::sleep(SHOW_TIMEOUT);
                commands::show_main(&handle, "timeout");
            });
            Ok(())
        })
        .build(tauri::generate_context!());

    match result {
        Ok(app) => app.run(|handle, event| {
            if let tauri::RunEvent::Exit = event {
                launch::on_exit(handle);
            }
        }),
        Err(e) => {
            tracing::error!(error = %e, "app exited with an error");
            eprintln!("Next Launcher failed to start: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    /// Regenerates `ui/src/lib/bindings.ts`. CI fails if the result differs from git.
    #[test]
    fn export_bindings() {
        super::specta_builder()
            .export(
                specta_typescript::Typescript::default().header(
                    "// @ts-nocheck\n// Generated by tauri-specta from src-tauri. Do not edit.",
                ),
                concat!(env!("CARGO_MANIFEST_DIR"), "/../ui/src/lib/bindings.ts"),
            )
            .expect("export TypeScript bindings");
    }
}
