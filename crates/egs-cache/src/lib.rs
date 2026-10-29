//! SQLite cache for Next Launcher: schema + migrations, the library snapshot the
//! first paint is rendered from, settings, and the image index.
//!
//! No Tauri dependency. All state that must survive a restart lives here (or in
//! the OS keyring for secrets).

mod downloads;
mod migrations;
mod mock;
pub mod model;
mod play;
mod store;

pub use store::Owned;
mod sync;

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::{Mutex, MutexGuard};
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection, OptionalExtension};

pub use downloads::{DownloadJob, GameInstall};
pub use migrations::LATEST as SCHEMA_VERSION;
pub use model::*;
pub use sync::{Adopt, CatalogFields, GameUpsert, KnownGame, PendingImage, SyncDiff};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("stored setting is not valid JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("database schema v{found} is newer than this build supports (v{supported})")]
    SchemaTooNew { found: u32, supported: u32 },
    #[error("database lock poisoned")]
    Poisoned,
    #[error("no system randomness: {0}")]
    Random(String),
}

pub struct Cache {
    conn: Mutex<Connection>,
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// FNV-1a 32-bit.
fn hue_hash(s: &str) -> u32 {
    let mut h: u32 = 0x811C_9DC5;
    for b in s.bytes() {
        h ^= u32::from(b);
        h = h.wrapping_mul(0x0100_0193);
    }
    h
}

/// Stable placeholder hue so a tile keeps its color across runs.
fn hue_for(app_name: &str) -> u16 {
    (hue_hash(app_name) % 360) as u16
}

impl Cache {
    /// Open (or create) the database, apply pragmas and migrations.
    pub fn open(path: &Path) -> Result<Self, Error> {
        let conn = Connection::open(path)?;
        Self::init(conn)
    }

    pub fn open_in_memory() -> Result<Self, Error> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(mut conn: Connection) -> Result<Self, Error> {
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             PRAGMA foreign_keys = ON;
             PRAGMA busy_timeout = 2000;
             PRAGMA temp_store = MEMORY;",
        )?;
        migrations::run(&mut conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    fn lock(&self) -> Result<MutexGuard<'_, Connection>, Error> {
        self.conn.lock().map_err(|_| Error::Poisoned)
    }

    /// Fill an empty database with `count` invented games (Phase 1 only).
    /// Returns `true` if it seeded.
    pub fn seed_mock_if_empty(&self, count: usize) -> Result<bool, Error> {
        let mut conn = self.lock()?;
        let existing: i64 = conn.query_row("SELECT COUNT(*) FROM games", [], |r| r.get(0))?;
        if existing > 0 {
            return Ok(false);
        }
        let tx = conn.transaction()?;
        mock::seed(&tx, count, now_secs())?;
        tx.commit()?;
        Ok(true)
    }

    /// Everything the first paint needs, in one read transaction.
    pub fn snapshot(&self) -> Result<LibrarySnapshot, Error> {
        let mut conn = self.lock()?;
        let tx = conn.transaction()?;

        // The library belongs to the signed-in (or most recent) account.
        tx.execute_batch(
            "CREATE TEMP VIEW IF NOT EXISTS current_owner AS
                 SELECT account_id FROM accounts ORDER BY signed_in DESC, updated_at DESC LIMIT 1",
        )?;

        let mut games = Vec::new();
        {
            let mut stmt = tx.prepare_cached(
                "SELECT g.app_name, g.title, g.developer, g.install_state, g.install_size,
                        g.last_played, g.playtime_secs, g.pinned,
                        (SELECT local_path FROM game_images i WHERE i.app_name = g.app_name AND i.kind = 'tall' AND i.local_path IS NOT NULL),
                        (SELECT local_path FROM game_images i WHERE i.app_name = g.app_name AND i.kind = 'wide' AND i.local_path IS NOT NULL),
                        CASE WHEN g.namespace = 'mock' THEN 1 ELSE g.installable END,
                        g.third_party
                 FROM games g
                 WHERE g.is_dlc = 0 AND g.hidden = 0 AND (g.owner IS NULL OR g.owner = (SELECT account_id FROM current_owner))
                 ORDER BY g.title COLLATE NOCASE",
            )?;
            let rows = stmt.query_map([], |r| {
                let app_name: String = r.get(0)?;
                let tall: Option<String> = r.get(8)?;
                let wide: Option<String> = r.get(9)?;
                Ok(GameRow {
                    hue: hue_for(&app_name),
                    has_cover: tall.is_some(),
                    has_wide: wide.is_some(),
                    art_rev: hue_hash(&format!(
                        "{}|{}",
                        tall.as_deref().unwrap_or(""),
                        wide.as_deref().unwrap_or("")
                    )) % 1_000_000,
                    title: r.get(1)?,
                    developer: r.get(2)?,
                    state: InstallState::from_db(&r.get::<_, String>(3)?),
                    install_size: r.get::<_, i64>(4)? as f64,
                    last_played: r.get::<_, Option<i64>>(5)?.map(|v| v as f64),
                    playtime_secs: r.get::<_, i64>(6)? as f64,
                    pinned: r.get(7)?,
                    installable: r.get(10)?,
                    third_party: r.get(11)?,
                    app_name,
                })
            })?;
            for row in rows {
                games.push(row?);
            }
        }

        let quick_launch = tx
            .prepare_cached(
                "SELECT app_name FROM games WHERE pinned = 1
                     AND (owner IS NULL OR owner = (SELECT account_id FROM current_owner))
                 ORDER BY pin_order, title",
            )?
            .query_map([], |r| r.get(0))?
            .collect::<Result<Vec<String>, _>>()?;

        let recent = tx
            .prepare_cached(
                "SELECT app_name FROM games WHERE last_played IS NOT NULL AND hidden = 0 AND is_dlc = 0
                     AND (owner IS NULL OR owner = (SELECT account_id FROM current_owner))
                 ORDER BY last_played DESC LIMIT 6",
            )?
            .query_map([], |r| r.get(0))?
            .collect::<Result<Vec<String>, _>>()?;

        let active_download = tx
            .query_row(
                "SELECT d.id, d.app_name, g.title, d.state, d.bytes_done, d.bytes_total, d.speed_bps
                 FROM downloads d JOIN games g ON g.app_name = d.app_name
                 WHERE d.state IN ('running','paused','queued')
                 ORDER BY CASE d.state WHEN 'running' THEN 0 WHEN 'paused' THEN 1 ELSE 2 END, d.queue_pos
                 LIMIT 1",
                [],
                |r| {
                    Ok(DownloadRow {
                        id: r.get::<_, i64>(0)? as f64,
                        app_name: r.get(1)?,
                        title: r.get(2)?,
                        state: r.get(3)?,
                        bytes_done: r.get::<_, i64>(4)? as f64,
                        bytes_total: r.get::<_, i64>(5)? as f64,
                        speed_bps: r.get::<_, i64>(6)? as f64,
                    })
                },
            )
            .optional()?;

        let account = tx
            .query_row(
                "SELECT account_id, display_name, email, country, signed_in FROM accounts
                 ORDER BY signed_in DESC, updated_at DESC LIMIT 1",
                [],
                |r| {
                    Ok(AccountRow {
                        account_id: r.get(0)?,
                        display_name: r.get(1)?,
                        email: r.get(2)?,
                        country: r.get(3)?,
                        signed_in: r.get(4)?,
                    })
                },
            )
            .optional()?;

        tx.commit()?;
        Ok(LibrarySnapshot {
            games,
            quick_launch,
            recent,
            active_download,
            account,
        })
    }

    /// All stored settings as raw JSON values (defaults are applied by the app).
    pub fn settings(&self) -> Result<BTreeMap<String, serde_json::Value>, Error> {
        let conn = self.lock()?;
        let mut stmt = conn.prepare_cached("SELECT key, value FROM settings")?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        let mut out = BTreeMap::new();
        for row in rows {
            let (k, v) = row?;
            out.insert(k, serde_json::from_str(&v)?);
        }
        Ok(out)
    }

    pub fn set_setting(&self, key: &str, value: &serde_json::Value) -> Result<(), Error> {
        let conn = self.lock()?;
        conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, serde_json::to_string(value)?],
        )?;
        Ok(())
    }

    /// Local file and content type of a cached image, if downloaded.
    pub fn image_file(
        &self,
        app_name: &str,
        kind: &str,
    ) -> Result<Option<(String, String)>, Error> {
        let conn = self.lock()?;
        Ok(conn
            .query_row(
                "SELECT local_path, COALESCE(content_type, 'image/jpeg') FROM game_images
                 WHERE app_name = ?1 AND kind = ?2 AND local_path IS NOT NULL",
                params![app_name, kind],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?)
    }

    /// Pin or unpin a game in Quick Launch. New pins go to the end.
    pub fn set_pinned(&self, app_name: &str, pinned: bool) -> Result<(), Error> {
        let conn = self.lock()?;
        if pinned {
            conn.execute(
                "UPDATE games SET pinned = 1,
                     pin_order = (SELECT COALESCE(MAX(pin_order), -1) + 1 FROM games WHERE pinned = 1),
                     updated_at = ?2
                 WHERE app_name = ?1 AND pinned = 0",
                params![app_name, now_secs()],
            )?;
        } else {
            conn.execute(
                "UPDATE games SET pinned = 0, pin_order = NULL, updated_at = ?2 WHERE app_name = ?1",
                params![app_name, now_secs()],
            )?;
        }
        Ok(())
    }

    /// The account whose session should be restored on start, if any.
    pub fn signed_in_account(&self) -> Result<Option<String>, Error> {
        let conn = self.lock()?;
        Ok(conn
            .query_row(
                "SELECT account_id FROM accounts WHERE signed_in = 1 ORDER BY updated_at DESC LIMIT 1",
                [],
                |r| r.get(0),
            )
            .optional()?)
    }

    /// Insert or update an account; marks it the one signed-in account.
    pub fn upsert_signed_in_account(&self, a: &AccountUpsert<'_>) -> Result<(), Error> {
        let mut conn = self.lock()?;
        let tx = conn.transaction()?;
        tx.execute(
            "UPDATE accounts SET signed_in = 0 WHERE account_id <> ?1",
            params![a.account_id],
        )?;
        tx.execute(
            "INSERT INTO accounts (account_id, display_name, email, country, signed_in, updated_at)
             VALUES (?1, ?2, ?3, ?4, 1, ?5)
             ON CONFLICT(account_id) DO UPDATE SET
                 display_name = excluded.display_name,
                 email = COALESCE(excluded.email, accounts.email),
                 country = COALESCE(excluded.country, accounts.country),
                 signed_in = 1,
                 updated_at = excluded.updated_at",
            params![a.account_id, a.display_name, a.email, a.country, now_secs()],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Keep the cached account row (library stays visible) but mark it signed out.
    pub fn mark_signed_out(&self, account_id: &str) -> Result<(), Error> {
        let conn = self.lock()?;
        conn.execute(
            "UPDATE accounts SET signed_in = 0, updated_at = ?2 WHERE account_id = ?1",
            params![account_id, now_secs()],
        )?;
        Ok(())
    }
}

/// Account fields we cache (email/country are optional until account info loads).
pub struct AccountUpsert<'a> {
    pub account_id: &'a str,
    pub display_name: &'a str,
    pub email: Option<&'a str>,
    pub country: Option<&'a str>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn account_sign_in_and_out() {
        let cache = Cache::open_in_memory().unwrap();
        assert_eq!(cache.signed_in_account().unwrap(), None);
        cache
            .upsert_signed_in_account(&AccountUpsert {
                account_id: "a1",
                display_name: "One",
                email: None,
                country: None,
            })
            .unwrap();
        cache
            .upsert_signed_in_account(&AccountUpsert {
                account_id: "a1",
                display_name: "One",
                email: Some("one@example.com"),
                country: Some("US"),
            })
            .unwrap();
        assert_eq!(cache.signed_in_account().unwrap().as_deref(), Some("a1"));
        let acct = cache.snapshot().unwrap().account.unwrap();
        assert_eq!(acct.email.as_deref(), Some("one@example.com"));
        assert!(acct.signed_in);

        // A second account takes over; the first is no longer signed in.
        cache
            .upsert_signed_in_account(&AccountUpsert {
                account_id: "a2",
                display_name: "Two",
                email: None,
                country: None,
            })
            .unwrap();
        assert_eq!(cache.signed_in_account().unwrap().as_deref(), Some("a2"));
        cache.mark_signed_out("a2").unwrap();
        assert_eq!(cache.signed_in_account().unwrap(), None);
    }

    #[test]
    fn mock_snapshot_has_expected_shape() {
        let cache = Cache::open_in_memory().unwrap();
        assert!(cache.seed_mock_if_empty(48).unwrap());
        assert!(!cache.seed_mock_if_empty(48).unwrap());
        let snap = cache.snapshot().unwrap();
        assert_eq!(snap.games.len(), 48);
        assert!(snap.recent.len() <= 6);
        assert!(!snap.quick_launch.is_empty());
        assert!(snap.active_download.is_some());
        assert!(
            snap.account.is_none(),
            "mock data must not create an account"
        );
        // sorted by title
        let titles: Vec<_> = snap.games.iter().map(|g| g.title.to_lowercase()).collect();
        let mut sorted = titles.clone();
        sorted.sort();
        assert_eq!(titles, sorted);
    }

    #[test]
    fn pin_and_settings_roundtrip() {
        let cache = Cache::open_in_memory().unwrap();
        cache.seed_mock_if_empty(10).unwrap();
        let snap = cache.snapshot().unwrap();
        let target = snap
            .games
            .iter()
            .find(|g| !g.pinned)
            .map(|g| g.app_name.clone())
            .unwrap();
        cache.set_pinned(&target, true).unwrap();
        let snap = cache.snapshot().unwrap();
        assert_eq!(snap.quick_launch.last(), Some(&target));
        cache.set_pinned(&target, false).unwrap();
        assert!(!cache.snapshot().unwrap().quick_launch.contains(&target));

        cache
            .set_setting("general.accent", &serde_json::json!("#248A3D"))
            .unwrap();
        assert_eq!(
            cache.settings().unwrap().get("general.accent"),
            Some(&serde_json::json!("#248A3D"))
        );
    }

    #[test]
    fn snapshot_of_large_library_is_fast() {
        let cache = Cache::open_in_memory().unwrap();
        cache.seed_mock_if_empty(600).unwrap();
        let t = std::time::Instant::now();
        let snap = cache.snapshot().unwrap();
        let elapsed = t.elapsed();
        assert_eq!(snap.games.len(), 600);
        // Generous bound for debug builds on CI; release is ~1-2 ms.
        assert!(elapsed.as_millis() < 250, "snapshot took {elapsed:?}");
    }
}
