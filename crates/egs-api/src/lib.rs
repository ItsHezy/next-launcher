//! Typed HTTP client for Epic's services, used with the signed-in user's own tokens.
//!
//! Specs live in `docs/protocol/`. This crate has no Tauri dependency and no
//! token storage; `egs-auth` owns the session and hands access tokens in.

pub mod account;
pub mod downloads;
pub mod error;
pub mod http;
pub mod images;
pub mod launch;
pub mod library;
pub mod oauth;
mod secret;
pub mod store;
pub mod wallet;

pub use error::{ApiError, EpicError};
pub use http::{Client, ClientConfig, Hosts, Retry};
pub use secret::Secret;

/// `User-Agent` for API hosts (docs/protocol/README.md "Client identity").
/// `egl_version` is e.g. `20.3.6-59512232`.
pub fn launcher_user_agent(egl_version: &str) -> String {
    format!("UELauncher/{egl_version}+++Portal+Release-Live Windows/10.0.19041.1.256.64bit")
}

/// EGL version used when the official launcher isn't installed locally.
pub const DEFAULT_EGL_VERSION: &str = "20.3.6-59512232";

/// Read the installed EGL version from its `Launcher.manifest.meta`
/// (contains `EpicGamesLauncher:...:<version>+++...`). Read-only.
pub fn detect_egl_version(program_data: &std::path::Path) -> Option<String> {
    let meta = program_data.join(r"Epic\EpicGamesLauncher\Data\Launcher.manifest.meta");
    let bytes = std::fs::read(meta).ok()?;
    let text = String::from_utf8_lossy(&bytes);
    let start = text.find("EpicGamesLauncher:EpicGamesLauncher:EpicGamesLauncher:")?;
    let rest = &text[start + "EpicGamesLauncher:EpicGamesLauncher:EpicGamesLauncher:".len()..];
    let version: String = rest
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '.' || *c == '-')
        .collect();
    // Expect something like 20.3.6-59512232
    (version.contains('.') && version.contains('-') && version.len() <= 32).then_some(version)
}

#[cfg(test)]
mod tests {
    #[test]
    fn user_agent_shape() {
        assert_eq!(
            super::launcher_user_agent("20.3.6-59512232"),
            "UELauncher/20.3.6-59512232+++Portal+Release-Live Windows/10.0.19041.1.256.64bit"
        );
    }
}
