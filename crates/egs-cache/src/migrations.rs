//! Versioned schema migrations, tracked with `PRAGMA user_version`.
//!
//! Each entry upgrades the schema by exactly one version and runs in its own
//! transaction. When the database is current, `run` costs one pragma read.

use rusqlite::Connection;

use crate::Error;

/// Ordered migrations; index `i` upgrades `user_version` from `i` to `i + 1`.
const MIGRATIONS: &[&str] = &[
    // v1: initial schema (brief section 7).
    r#"
    CREATE TABLE accounts (
        account_id     TEXT PRIMARY KEY,
        display_name   TEXT NOT NULL,
        email          TEXT,
        country        TEXT,
        avatar_url     TEXT,
        signed_in      INTEGER NOT NULL DEFAULT 0,
        updated_at     INTEGER NOT NULL
    );

    CREATE TABLE games (
        app_name        TEXT PRIMARY KEY,
        namespace       TEXT NOT NULL,
        catalog_item_id TEXT NOT NULL,
        title           TEXT NOT NULL,
        developer       TEXT NOT NULL DEFAULT '',
        install_state   TEXT NOT NULL DEFAULT 'notInstalled'
                        CHECK (install_state IN ('notInstalled','installed','updateAvailable','installing')),
        version         TEXT,
        latest_version  TEXT,
        install_path    TEXT,
        install_size    INTEGER NOT NULL DEFAULT 0,
        last_played     INTEGER,
        playtime_secs   INTEGER NOT NULL DEFAULT 0,
        pinned          INTEGER NOT NULL DEFAULT 0,
        pin_order       INTEGER,
        cloud_saves     INTEGER NOT NULL DEFAULT 0,
        is_dlc          INTEGER NOT NULL DEFAULT 0,
        hidden          INTEGER NOT NULL DEFAULT 0,
        acquired_at     INTEGER,
        updated_at      INTEGER NOT NULL
    );
    CREATE INDEX games_last_played ON games(last_played DESC) WHERE last_played IS NOT NULL;
    CREATE INDEX games_pinned ON games(pin_order) WHERE pinned = 1;

    CREATE TABLE game_images (
        app_name     TEXT NOT NULL REFERENCES games(app_name) ON DELETE CASCADE,
        kind         TEXT NOT NULL,          -- 'tall' | 'wide' | 'thumb'
        url          TEXT NOT NULL,
        local_path   TEXT,
        content_type TEXT,
        etag         TEXT,
        PRIMARY KEY (app_name, kind)
    );

    CREATE TABLE downloads (
        id           INTEGER PRIMARY KEY AUTOINCREMENT,
        app_name     TEXT NOT NULL REFERENCES games(app_name) ON DELETE CASCADE,
        kind         TEXT NOT NULL CHECK (kind IN ('install','update','repair')),
        state        TEXT NOT NULL CHECK (state IN ('queued','running','paused','done','failed','cancelled')),
        queue_pos    INTEGER NOT NULL DEFAULT 0,
        bytes_done   INTEGER NOT NULL DEFAULT 0,
        bytes_total  INTEGER NOT NULL DEFAULT 0,
        chunks_done  INTEGER NOT NULL DEFAULT 0,
        chunks_total INTEGER NOT NULL DEFAULT 0,
        speed_bps    INTEGER NOT NULL DEFAULT 0,
        manifest_hash TEXT,
        error        TEXT,
        created_at   INTEGER NOT NULL,
        updated_at   INTEGER NOT NULL
    );
    CREATE INDEX downloads_active ON downloads(queue_pos) WHERE state IN ('queued','running','paused');

    CREATE TABLE settings (
        key   TEXT PRIMARY KEY,
        value TEXT NOT NULL                  -- JSON
    );

    CREATE TABLE store_cache (
        query_hash TEXT PRIMARY KEY,
        body       TEXT NOT NULL,            -- JSON
        expires_at INTEGER NOT NULL
    );

    CREATE TABLE events_log (
        id      INTEGER PRIMARY KEY AUTOINCREMENT,
        at      INTEGER NOT NULL,
        kind    TEXT NOT NULL,
        payload TEXT NOT NULL                -- JSON, never secrets
    );
    "#,
    // v2: real library sync (Phase 3).
    r#"
    ALTER TABLE games ADD COLUMN owner          TEXT;             -- account id the row belongs to
    ALTER TABLE games ADD COLUMN installable    INTEGER NOT NULL DEFAULT 0; -- Epic offers a Windows build
    ALTER TABLE games ADD COLUMN record_type    TEXT;
    ALTER TABLE games ADD COLUMN artifact_id    TEXT;             -- releaseInfo appId (playtime)
    ALTER TABLE games ADD COLUMN third_party    TEXT;             -- e.g. "Origin", "UbisoftConnect"
    ALTER TABLE games ADD COLUMN main_game      TEXT;             -- catalog id of the base game (DLC)
    ALTER TABLE games ADD COLUMN meta_json      TEXT;             -- raw catalog item
    ALTER TABLE games ADD COLUMN catalog_at     INTEGER;          -- when meta_json was fetched
    ALTER TABLE games ADD COLUMN egl_managed    INTEGER NOT NULL DEFAULT 0; -- adopted from EGL
    ALTER TABLE games ADD COLUMN egl_guid       TEXT;
    ALTER TABLE games ADD COLUMN needs_verify   INTEGER NOT NULL DEFAULT 0; -- verify before first use
    CREATE INDEX games_owner ON games(owner);
    CREATE INDEX games_artifact ON games(artifact_id) WHERE artifact_id IS NOT NULL;

    CREATE TABLE sync_state (
        account_id     TEXT PRIMARY KEY,
        library_token  TEXT,
        last_sync_at   INTEGER,
        last_full_at   INTEGER
    );
    "#,
];

/// Latest schema version this build understands.
pub const LATEST: u32 = MIGRATIONS.len() as u32;

/// Bring the schema up to [`LATEST`]. Refuses databases from a newer build.
pub fn run(conn: &mut Connection) -> Result<u32, Error> {
    let current: u32 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
    if current > LATEST {
        return Err(Error::SchemaTooNew {
            found: current,
            supported: LATEST,
        });
    }
    for (idx, sql) in MIGRATIONS.iter().enumerate().skip(current as usize) {
        let target = idx as u32 + 1;
        let tx = conn.transaction()?;
        tx.execute_batch(sql)?;
        tx.pragma_update(None, "user_version", target)?;
        tx.commit()?;
        tracing::info!(version = target, "applied schema migration");
    }
    Ok(LATEST)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrates_fresh_db_and_is_idempotent() {
        let mut conn = Connection::open_in_memory().unwrap();
        assert_eq!(run(&mut conn).unwrap(), LATEST);
        assert_eq!(run(&mut conn).unwrap(), LATEST);
        let v: u32 = conn
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .unwrap();
        assert_eq!(v, LATEST);
    }

    #[test]
    fn rejects_newer_schema() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "user_version", LATEST + 1)
            .unwrap();
        assert!(matches!(run(&mut conn), Err(Error::SchemaTooNew { .. })));
    }
}
