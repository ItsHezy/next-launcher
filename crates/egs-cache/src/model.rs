//! Rows sent to the UI. Field names are camelCase on the wire.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum InstallState {
    NotInstalled,
    Installed,
    UpdateAvailable,
    Installing,
}

impl InstallState {
    pub fn as_db(self) -> &'static str {
        match self {
            Self::NotInstalled => "notInstalled",
            Self::Installed => "installed",
            Self::UpdateAvailable => "updateAvailable",
            Self::Installing => "installing",
        }
    }

    pub fn from_db(s: &str) -> Self {
        match s {
            "installed" => Self::Installed,
            "updateAvailable" => Self::UpdateAvailable,
            "installing" => Self::Installing,
            _ => Self::NotInstalled,
        }
    }
}

/// One library tile. Kept small: 500+ of these go into the boot payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct GameRow {
    pub app_name: String,
    pub title: String,
    pub developer: String,
    pub state: InstallState,
    pub install_size: f64,
    /// Unix seconds.
    pub last_played: Option<f64>,
    pub playtime_secs: f64,
    pub pinned: bool,
    /// A 3:4 cover is cached on disk (served by the `img` protocol); else the UI draws a placeholder.
    pub has_cover: bool,
    /// A 16:9 image is cached on disk.
    pub has_wide: bool,
    /// Changes when cached art changes; appended to image URLs to bust the WebView cache.
    pub art_rev: u32,
    /// Stable 0–359 hue for the placeholder gradient.
    pub hue: u16,
    /// Epic offers a Windows build we may install.
    pub installable: bool,
    /// Managed by another launcher (e.g. EA app, Ubisoft Connect).
    pub third_party: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct DownloadRow {
    pub id: f64,
    pub app_name: String,
    pub title: String,
    pub state: String,
    pub bytes_done: f64,
    pub bytes_total: f64,
    pub speed_bps: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct AccountRow {
    pub account_id: String,
    pub display_name: String,
    pub email: Option<String>,
    pub country: Option<String>,
    pub signed_in: bool,
}

/// Everything the first paint needs, read in one transaction.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct LibrarySnapshot {
    /// All non-DLC, non-hidden games, sorted by title.
    pub games: Vec<GameRow>,
    /// `app_name`s of pinned games in pin order.
    pub quick_launch: Vec<String>,
    /// `app_name`s by most recent `last_played`, at most 6.
    pub recent: Vec<String>,
    pub active_download: Option<DownloadRow>,
    pub account: Option<AccountRow>,
}
