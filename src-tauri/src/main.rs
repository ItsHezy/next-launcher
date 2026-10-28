#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod console;
mod logging;
mod paths;
mod protocol;
mod startup;

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
    mock_games: usize,
    data_dir: Option<PathBuf>,
    reset: bool,
}

fn parse_args() -> Args {
    let mut args = Args {
        bench: false,
        bench_scroll: false,
        dev_route: None,
        // Phase 1: the library is mock data. Phase 3 replaces this with sync.
        mock_games: 48,
        data_dir: None,
        reset: false,
    };
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--bench-startup" => args.bench = true,
            "--bench-scroll" => args.bench_scroll = true,
            "--reset-data" => args.reset = true,
            "--mock-games" => {
                if let Some(n) = it.next().and_then(|v| v.parse().ok()) {
                    args.mock_games = n;
                }
            }
            "--data-dir" => args.data_dir = it.next().map(PathBuf::from),
            "--dev-route" => args.dev_route = it.next(),
            _ => {}
        }
    }
    args
}

fn specta_builder() -> tauri_specta::Builder<tauri::Wry> {
    tauri_specta::Builder::<tauri::Wry>::new().commands(tauri_specta::collect_commands![
        commands::app_ready,
        commands::bench_report,
        commands::startup_timeline,
        commands::get_snapshot,
        commands::get_settings,
        commands::set_setting,
        commands::set_pinned,
        commands::window_action,
    ])
}

struct Boot {
    cache: Arc<Cache>,
    json: String,
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
    startup::mark("snapshot_read");
    let json = serde_json::to_string(&serde_json::json!({
        "snapshot": snapshot,
        "settings": settings,
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
    })
}

/// Fallback when the database can't be opened: start empty, tell the user why.
fn boot_failed(err: &anyhow::Error) -> anyhow::Result<Boot> {
    tracing::error!(error = %err, "boot failed, starting with an empty in-memory cache");
    let cache = Cache::open_in_memory()?;
    let json = serde_json::to_string(&serde_json::json!({
        "snapshot": cache.snapshot()?,
        "settings": {},
        "imgBase": protocol::base_url(),
        "bench": false,
        "benchScroll": false,
        "devRoute": null,
        "bootError": "Your local library cache couldn't be opened, so the library is empty for now. Details are in the log.",
    }))?;
    Ok(Boot {
        cache: Arc::new(cache),
        json,
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
    logging::init(&paths.logs, false);
    tracing::info!(version = env!("CARGO_PKG_VERSION"), "starting");

    // Read the cache while Tauri and WebView2 initialise.
    let boot_paths = paths.clone();
    let (mock_games, bench, bench_scroll) = (args.mock_games, args.bench, args.bench_scroll);
    let dev_route = args.dev_route.clone();
    let boot_thread =
        std::thread::spawn(move || boot(&boot_paths, mock_games, bench, bench_scroll, dev_route));

    let specta = specta_builder();
    let result = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.unminimize();
                let _ = w.show();
                let _ = w.set_focus();
            }
        }))
        .invoke_handler(specta.invoke_handler())
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
            let boot = match boot_thread.join() {
                Ok(Ok(b)) => b,
                Ok(Err(e)) => boot_failed(&e)?,
                Err(_) => boot_failed(&anyhow::anyhow!("boot thread panicked"))?,
            };
            app.manage(AppState {
                cache: boot.cache,
                paths: paths.clone(),
                bench,
                shown: AtomicBool::new(false),
            });

            WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()))
                .title("Next Launcher")
                .inner_size(1280.0, 800.0)
                .min_inner_size(960.0, 600.0)
                .center()
                .decorations(false)
                .shadow(true)
                .visible(false)
                .background_color(BG)
                .data_directory(paths.webview_ui.clone())
                .initialization_script(format!("window.__NL_BOOT__={};", boot.json))
                .build()?;
            startup::mark("window_built");

            let handle = app.handle().clone();
            std::thread::spawn(move || {
                std::thread::sleep(SHOW_TIMEOUT);
                commands::show_main(&handle, "timeout");
            });
            Ok(())
        })
        .run(tauri::generate_context!());

    if let Err(e) = result {
        tracing::error!(error = %e, "app exited with an error");
        eprintln!("Next Launcher failed to start: {e}");
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
