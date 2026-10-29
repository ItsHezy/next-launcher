//! Where Next Launcher keeps its data.
//!
//! Release: `%LOCALAPPDATA%\NextLauncher`. Debug builds default to `<repo>/.dev-data`
//! so development never touches the real profile. `--data-dir <path>` overrides both.

use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct Paths {
    pub root: PathBuf,
    pub db: PathBuf,
    pub logs: PathBuf,
    pub images: PathBuf,
    /// Copies of installed builds' manifests (updates, verify, uninstall).
    pub manifests: PathBuf,
    pub webview_ui: PathBuf,
    /// Epic login + signed-in Epic pages share one profile, so the login cookie
    /// also signs in account pages (ADR 0002, revised in Phase 2).
    pub webview_web: PathBuf,
}

impl Paths {
    pub fn resolve(override_dir: Option<PathBuf>) -> Self {
        // WebView2 resolves a relative profile folder from the exe's folder, SQLite from the
        // current one: make `--data-dir` absolute so both land in the same place.
        let override_dir = override_dir.map(|d| std::path::absolute(&d).unwrap_or(d));
        let root = override_dir.unwrap_or_else(default_root);
        Self {
            db: root.join("next-launcher.sqlite3"),
            logs: root.join("logs"),
            images: root.join("images"),
            manifests: root.join("manifests"),
            webview_ui: root.join("webview").join("ui"),
            webview_web: root.join("webview").join("web"),
            root,
        }
    }

    /// Present when Settings → Support → Debug logging is on (read before logging starts).
    pub fn debug_marker(&self) -> PathBuf {
        self.root.join("debug-logging")
    }

    pub fn ensure(&self) -> std::io::Result<()> {
        for dir in [
            &self.root,
            &self.logs,
            &self.images,
            &self.manifests,
            &self.webview_ui,
        ] {
            std::fs::create_dir_all(dir)?;
        }
        Ok(())
    }
}

#[cfg(debug_assertions)]
fn default_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join(".dev-data")
}

#[cfg(not(debug_assertions))]
fn default_root() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("NextLauncher")
}
