//! Library sync writes (Phase 3). One transaction per sync; returns what changed
//! so the UI re-renders only those tiles. Spec: docs/protocol/library.md.

use std::collections::{HashMap, HashSet};

use rusqlite::{params, OptionalExtension};

use crate::{now_secs, Cache, Error};

/// Catalog-derived fields; `None` on a [`GameUpsert`] keeps what's stored.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CatalogFields {
    pub title: String,
    pub developer: String,
    pub is_dlc: bool,
    pub artifact_id: Option<String>,
    pub third_party: Option<String>,
    pub main_game: Option<String>,
    pub meta_json: String,
    pub tall_url: Option<String>,
    pub wide_url: Option<String>,
}

#[derive(Debug, Clone)]
pub struct GameUpsert {
    pub app_name: String,
    pub namespace: String,
    pub catalog_item_id: String,
    /// Build version from the assets list; `None` = not installable here.
    pub latest_version: Option<String>,
    pub record_type: Option<String>,
    pub acquired_at: Option<i64>,
    /// Used when the catalog couldn't be fetched for a new item.
    pub fallback_title: String,
    pub catalog: Option<CatalogFields>,
}

/// What the cache already knows about a game (to decide what to re-fetch).
#[derive(Debug, Clone, PartialEq)]
pub struct KnownGame {
    pub latest_version: Option<String>,
    pub catalog_at: Option<i64>,
}

#[derive(Debug, Default, PartialEq)]
pub struct SyncDiff {
    pub added: Vec<String>,
    pub changed: Vec<String>,
    pub removed: Vec<String>,
    pub removed_mock: usize,
}

impl SyncDiff {
    pub fn is_empty(&self) -> bool {
        self.added.is_empty()
            && self.changed.is_empty()
            && self.removed.is_empty()
            && self.removed_mock == 0
    }
}

/// An official-launcher install to adopt.
#[derive(Debug, Clone)]
pub struct Adopt<'a> {
    pub app_name: &'a str,
    pub install_path: &'a str,
    pub version: &'a str,
    pub install_size: u64,
    pub egl_guid: &'a str,
    pub needs_verify: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PendingImage {
    pub app_name: String,
    pub kind: String,
    pub url: String,
}

/// Recompute install state from versions (only for installed games).
fn derived_state(current: &str, version: Option<&str>, latest: Option<&str>) -> String {
    match current {
        "installed" | "updateAvailable" => match (version, latest) {
            (Some(v), Some(l)) if v != l => "updateAvailable".into(),
            _ => "installed".into(),
        },
        other => other.to_owned(),
    }
}

impl Cache {
    pub fn known_games(&self, owner: &str) -> Result<HashMap<String, KnownGame>, Error> {
        let conn = self.lock()?;
        let mut stmt = conn.prepare_cached(
            "SELECT app_name, latest_version, catalog_at FROM games WHERE owner = ?1",
        )?;
        let rows = stmt.query_map(params![owner], |r| {
            Ok((
                r.get::<_, String>(0)?,
                KnownGame {
                    latest_version: r.get(1)?,
                    catalog_at: r.get(2)?,
                },
            ))
        })?;
        rows.collect::<Result<_, _>>().map_err(Error::from)
    }

    /// Apply a library sync for `owner`. With `complete`, rows no longer owned are
    /// removed (never installed or EGL-adopted ones). Mock rows are always removed.
    pub fn apply_library(
        &self,
        owner: &str,
        rows: &[GameUpsert],
        complete: bool,
    ) -> Result<SyncDiff, Error> {
        let mut conn = self.lock()?;
        let tx = conn.transaction()?;
        let now = now_secs();
        let mut diff = SyncDiff::default();

        tx.execute(
            "DELETE FROM downloads WHERE app_name IN (SELECT app_name FROM games WHERE namespace = 'mock')",
            [],
        )?;
        diff.removed_mock = tx.execute("DELETE FROM games WHERE namespace = 'mock'", [])?;

        let mut seen = HashSet::with_capacity(rows.len());
        for row in rows {
            seen.insert(row.app_name.as_str());
            type Existing = (
                String,
                String,
                Option<String>,
                bool,
                bool,
                Option<String>,
                Option<String>,
                String,
                Option<String>,
            );
            let existing: Option<Existing> = tx
                .query_row(
                    "SELECT title, developer, latest_version, installable, is_dlc, third_party, version,
                            install_state, meta_json
                     FROM games WHERE app_name = ?1",
                    params![row.app_name],
                    |r| {
                        Ok((
                            r.get(0)?,
                            r.get(1)?,
                            r.get(2)?,
                            r.get(3)?,
                            r.get(4)?,
                            r.get(5)?,
                            r.get(6)?,
                            r.get(7)?,
                            r.get(8)?,
                        ))
                    },
                )
                .optional()?;
            let installable = row.latest_version.is_some();

            match existing {
                None => {
                    let c = row.catalog.clone().unwrap_or_default();
                    let title = if c.title.is_empty() {
                        row.fallback_title.clone()
                    } else {
                        c.title.clone()
                    };
                    tx.execute(
                        "INSERT INTO games (app_name, namespace, catalog_item_id, title, developer, install_state,
                             latest_version, installable, record_type, artifact_id, third_party, main_game,
                             is_dlc, meta_json, catalog_at, acquired_at, owner, updated_at)
                         VALUES (?1, ?2, ?3, ?4, ?5, 'notInstalled', ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)",
                        params![
                            row.app_name,
                            row.namespace,
                            row.catalog_item_id,
                            title,
                            c.developer,
                            row.latest_version,
                            installable,
                            row.record_type,
                            c.artifact_id,
                            c.third_party,
                            c.main_game,
                            c.is_dlc,
                            row.catalog.as_ref().map(|c| c.meta_json.as_str()),
                            row.catalog.as_ref().map(|_| now),
                            row.acquired_at,
                            owner,
                            now
                        ],
                    )?;
                    diff.added.push(row.app_name.clone());
                }
                Some((
                    title,
                    developer,
                    latest,
                    was_installable,
                    is_dlc,
                    third_party,
                    version,
                    state,
                    meta,
                )) => {
                    let new_state =
                        derived_state(&state, version.as_deref(), row.latest_version.as_deref());
                    let c = row.catalog.as_ref();
                    let changed = latest != row.latest_version
                        || was_installable != installable
                        || new_state != state
                        || c.is_some_and(|c| {
                            c.title != title
                                || c.developer != developer
                                || c.is_dlc != is_dlc
                                || c.third_party != third_party
                                || meta.as_deref() != Some(c.meta_json.as_str())
                        });
                    if c.is_some() || changed {
                        tx.execute(
                            "UPDATE games SET namespace = ?2, catalog_item_id = ?3, latest_version = ?4,
                                 installable = ?5, record_type = COALESCE(?6, record_type), install_state = ?7,
                                 owner = ?8, acquired_at = COALESCE(?9, acquired_at), updated_at = ?10
                             WHERE app_name = ?1",
                            params![
                                row.app_name,
                                row.namespace,
                                row.catalog_item_id,
                                row.latest_version,
                                installable,
                                row.record_type,
                                new_state,
                                owner,
                                row.acquired_at,
                                now
                            ],
                        )?;
                        if let Some(c) = c {
                            tx.execute(
                                "UPDATE games SET title = ?2, developer = ?3, is_dlc = ?4, artifact_id = ?5,
                                     third_party = ?6, main_game = ?7, meta_json = ?8, catalog_at = ?9
                                 WHERE app_name = ?1",
                                params![
                                    row.app_name,
                                    c.title,
                                    c.developer,
                                    c.is_dlc,
                                    c.artifact_id,
                                    c.third_party,
                                    c.main_game,
                                    c.meta_json,
                                    now
                                ],
                            )?;
                        }
                    }
                    if changed {
                        diff.changed.push(row.app_name.clone());
                    }
                }
            }

            if let Some(c) = &row.catalog {
                for (kind, url) in [("tall", &c.tall_url), ("wide", &c.wide_url)] {
                    match url {
                        Some(url) => {
                            tx.execute(
                                "INSERT INTO game_images (app_name, kind, url) VALUES (?1, ?2, ?3)
                                 ON CONFLICT(app_name, kind) DO UPDATE SET
                                     url = excluded.url,
                                     local_path = CASE WHEN game_images.url = excluded.url THEN game_images.local_path ELSE NULL END,
                                     etag = CASE WHEN game_images.url = excluded.url THEN game_images.etag ELSE NULL END",
                                params![row.app_name, kind, url],
                            )?;
                        }
                        None => {
                            tx.execute(
                                "DELETE FROM game_images WHERE app_name = ?1 AND kind = ?2",
                                params![row.app_name, kind],
                            )?;
                        }
                    }
                }
            }
        }

        if complete {
            let mut stmt = tx.prepare(
                "SELECT app_name FROM games WHERE owner = ?1 AND install_state = 'notInstalled' AND egl_managed = 0",
            )?;
            let stale: Vec<String> = stmt
                .query_map(params![owner], |r| r.get(0))?
                .collect::<Result<Vec<String>, _>>()?
                .into_iter()
                .filter(|a| !seen.contains(a.as_str()))
                .collect();
            drop(stmt);
            for app in &stale {
                tx.execute("DELETE FROM games WHERE app_name = ?1", params![app])?;
            }
            diff.removed = stale;
        }

        tx.commit()?;
        Ok(diff)
    }

    /// Owned list unchanged: apply only new build versions; returns changed app names.
    pub fn update_versions(
        &self,
        owner: &str,
        versions: &[(String, String)],
    ) -> Result<Vec<String>, Error> {
        let mut conn = self.lock()?;
        let tx = conn.transaction()?;
        let mut changed = Vec::new();
        for (app, latest) in versions {
            let row: Option<(Option<String>, Option<String>, String)> = tx
                .query_row(
                    "SELECT latest_version, version, install_state FROM games WHERE app_name = ?1 AND owner = ?2",
                    params![app, owner],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                )
                .optional()?;
            let Some((old, version, state)) = row else {
                continue;
            };
            let new_state = derived_state(&state, version.as_deref(), Some(latest));
            if old.as_deref() != Some(latest.as_str()) || new_state != state {
                tx.execute(
                    "UPDATE games SET latest_version = ?2, installable = 1, install_state = ?3, updated_at = ?4
                     WHERE app_name = ?1",
                    params![app, latest, new_state, now_secs()],
                )?;
                changed.push(app.clone());
            }
        }
        tx.commit()?;
        Ok(changed)
    }

    /// Adopt an install made by the official launcher (read-only on its side).
    /// Only for an owned, not-yet-installed game. Returns false if nothing matched.
    pub fn adopt_install(&self, owner: &str, a: &Adopt<'_>) -> Result<bool, Error> {
        let mut conn = self.lock()?;
        let tx = conn.transaction()?;
        let latest: Option<Option<String>> = tx
            .query_row(
                "SELECT latest_version FROM games
                 WHERE app_name = ?1 AND owner = ?2 AND install_state = 'notInstalled'",
                params![a.app_name, owner],
                |r| r.get(0),
            )
            .optional()?;
        let Some(latest) = latest else {
            return Ok(false);
        };
        let state = derived_state("installed", Some(a.version), latest.as_deref());
        tx.execute(
            "UPDATE games SET install_state = ?2, version = ?3, install_path = ?4, install_size = ?5,
                 egl_managed = 1, egl_guid = ?6, needs_verify = ?7, updated_at = ?8
             WHERE app_name = ?1",
            params![a.app_name, state, a.version, a.install_path, a.install_size as i64, a.egl_guid, a.needs_verify, now_secs()],
        )?;
        tx.commit()?;
        Ok(true)
    }

    pub fn library_token(&self, owner: &str) -> Result<Option<String>, Error> {
        let conn = self.lock()?;
        Ok(conn
            .query_row(
                "SELECT library_token FROM sync_state WHERE account_id = ?1",
                params![owner],
                |r| r.get(0),
            )
            .optional()?
            .flatten())
    }

    pub fn set_sync_state(
        &self,
        owner: &str,
        token: Option<&str>,
        full: bool,
    ) -> Result<(), Error> {
        let conn = self.lock()?;
        let now = now_secs();
        conn.execute(
            "INSERT INTO sync_state (account_id, library_token, last_sync_at, last_full_at) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(account_id) DO UPDATE SET
                 library_token = COALESCE(excluded.library_token, sync_state.library_token),
                 last_sync_at = excluded.last_sync_at,
                 last_full_at = COALESCE(excluded.last_full_at, sync_state.last_full_at)",
            params![owner, token, now, full.then_some(now)],
        )?;
        Ok(())
    }

    /// Apply Epic's per-artifact playtime totals; returns changed app names.
    pub fn set_playtime(
        &self,
        owner: &str,
        totals: &[(String, u64)],
    ) -> Result<Vec<String>, Error> {
        let mut conn = self.lock()?;
        let tx = conn.transaction()?;
        let mut changed = Vec::new();
        for (artifact, secs) in totals {
            let mut stmt = tx.prepare_cached(
                "UPDATE games SET playtime_secs = ?3, updated_at = ?4
                 WHERE owner = ?1 AND artifact_id = ?2 AND playtime_secs <> ?3
                 RETURNING app_name",
            )?;
            let rows = stmt.query_map(params![owner, artifact, *secs as i64, now_secs()], |r| {
                r.get(0)
            })?;
            for r in rows {
                changed.push(r?);
            }
        }
        tx.commit()?;
        Ok(changed)
    }

    /// Images still to download (skips ones that failed in the last day).
    pub fn pending_images(&self, limit: usize) -> Result<Vec<PendingImage>, Error> {
        let conn = self.lock()?;
        let mut stmt = conn.prepare_cached(
            "SELECT i.app_name, i.kind, i.url FROM game_images i JOIN games g ON g.app_name = i.app_name
             WHERE i.local_path IS NULL
               AND (i.etag IS NULL OR i.etag NOT LIKE 'failed:%' OR CAST(substr(i.etag, 8) AS INTEGER) < ?1)
             ORDER BY g.is_dlc, g.title COLLATE NOCASE
             LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![now_secs() - 86_400, limit as i64], |r| {
            Ok(PendingImage {
                app_name: r.get(0)?,
                kind: r.get(1)?,
                url: r.get(2)?,
            })
        })?;
        rows.collect::<Result<_, _>>().map_err(Error::from)
    }

    /// Record a downloaded image (only if its URL didn't change meanwhile).
    pub fn image_saved(
        &self,
        img: &PendingImage,
        local_path: &str,
        content_type: &str,
    ) -> Result<bool, Error> {
        let conn = self.lock()?;
        let n = conn.execute(
            "UPDATE game_images SET local_path = ?4, content_type = ?5, etag = NULL
             WHERE app_name = ?1 AND kind = ?2 AND url = ?3",
            params![img.app_name, img.kind, img.url, local_path, content_type],
        )?;
        Ok(n > 0)
    }

    pub fn image_failed(&self, img: &PendingImage) -> Result<(), Error> {
        let conn = self.lock()?;
        conn.execute(
            "UPDATE game_images SET etag = 'failed:' || ?4 WHERE app_name = ?1 AND kind = ?2 AND url = ?3",
            params![img.app_name, img.kind, img.url, now_secs()],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Sync only runs when signed in, so the account row exists.
    fn cache() -> Cache {
        let c = Cache::open_in_memory().unwrap();
        c.upsert_signed_in_account(&crate::AccountUpsert {
            account_id: "acc",
            display_name: "Player",
            email: None,
            country: None,
        })
        .unwrap();
        c
    }

    fn up(app: &str, latest: Option<&str>, title: Option<&str>) -> GameUpsert {
        GameUpsert {
            app_name: app.into(),
            namespace: format!("ns-{app}"),
            catalog_item_id: format!("id-{app}"),
            latest_version: latest.map(Into::into),
            record_type: Some("APPLICATION".into()),
            acquired_at: Some(1_700_000_000),
            fallback_title: app.into(),
            catalog: title.map(|t| CatalogFields {
                title: t.into(),
                developer: "Dev".into(),
                meta_json: format!("{{\"title\":\"{t}\"}}"),
                tall_url: Some(format!("https://cdn1.epicgames.com/{app}-tall.jpg")),
                wide_url: None,
                artifact_id: Some(format!("art-{app}")),
                ..Default::default()
            }),
        }
    }

    #[test]
    fn sync_diffs_and_replaces_mock_data() {
        let c = cache();
        c.seed_mock_if_empty(10).unwrap();

        let d = c
            .apply_library(
                "acc",
                &[
                    up("A", Some("1"), Some("Alpha")),
                    up("B", None, Some("Beta")),
                ],
                true,
            )
            .unwrap();
        assert_eq!(d.removed_mock, 10);
        assert_eq!(d.added, vec!["A", "B"]);
        let snap = c.snapshot().unwrap();
        assert_eq!(snap.games.len(), 2);
        assert!(snap
            .games
            .iter()
            .any(|g| g.title == "Beta" && !g.installable));

        // Same data again: nothing changed (catalog not re-sent keeps stored metadata).
        let d = c
            .apply_library(
                "acc",
                &[up("A", Some("1"), None), up("B", None, None)],
                true,
            )
            .unwrap();
        assert!(d.is_empty(), "{d:?}");
        assert!(c
            .snapshot()
            .unwrap()
            .games
            .iter()
            .any(|g| g.title == "Alpha"));

        // New build for A, B no longer owned.
        let d = c
            .apply_library("acc", &[up("A", Some("2"), None)], true)
            .unwrap();
        assert_eq!(d.changed, vec!["A"]);
        assert_eq!(d.removed, vec!["B"]);
    }

    #[test]
    fn installed_game_gets_update_available_and_is_never_removed() {
        let c = cache();
        c.apply_library("acc", &[up("A", Some("1"), Some("Alpha"))], true)
            .unwrap();
        {
            let conn = c.lock().unwrap();
            conn.execute(
                "UPDATE games SET install_state = 'installed', version = '1' WHERE app_name = 'A'",
                [],
            )
            .unwrap();
        }
        c.apply_library("acc", &[up("A", Some("2"), None)], true)
            .unwrap();
        let g = c
            .snapshot()
            .unwrap()
            .games
            .into_iter()
            .find(|g| g.app_name == "A")
            .unwrap();
        assert_eq!(g.state, crate::InstallState::UpdateAvailable);

        // Disappears from the owned list but is installed: kept.
        let d = c.apply_library("acc", &[], true).unwrap();
        assert!(d.removed.is_empty());
    }

    #[test]
    fn images_pending_saved_and_reset_on_url_change() {
        let c = cache();
        c.apply_library("acc", &[up("A", Some("1"), Some("Alpha"))], true)
            .unwrap();
        let p = c.pending_images(10).unwrap();
        assert_eq!(p.len(), 1);
        assert!(c.image_saved(&p[0], "C:/x.jpg", "image/jpeg").unwrap());
        assert!(c.pending_images(10).unwrap().is_empty());
        assert!(c.snapshot().unwrap().games[0].has_cover);

        let mut changed = up("A", Some("1"), Some("Alpha"));
        if let Some(cat) = changed.catalog.as_mut() {
            cat.tall_url = Some("https://cdn1.epicgames.com/new.jpg".into());
        }
        c.apply_library("acc", &[changed], true).unwrap();
        assert_eq!(c.pending_images(10).unwrap().len(), 1);
        assert!(!c.snapshot().unwrap().games[0].has_cover);
    }

    #[test]
    fn adopt_marks_installed_once() {
        let c = cache();
        c.apply_library("acc", &[up("A", Some("2"), Some("Alpha"))], true)
            .unwrap();
        let a = Adopt {
            app_name: "A",
            install_path: "F:/Games/A",
            version: "1",
            install_size: 10,
            egl_guid: "G",
            needs_verify: true,
        };
        assert!(c.adopt_install("acc", &a).unwrap());
        assert!(
            !c.adopt_install("acc", &a).unwrap(),
            "second adopt is a no-op"
        );
        assert!(!c
            .adopt_install(
                "other",
                &Adopt {
                    app_name: "A",
                    ..a.clone()
                }
            )
            .unwrap());
        let g = c
            .snapshot()
            .unwrap()
            .games
            .into_iter()
            .find(|g| g.app_name == "A")
            .unwrap();
        assert_eq!(g.state, crate::InstallState::UpdateAvailable);
    }

    #[test]
    fn playtime_and_sync_state() {
        let c = cache();
        c.apply_library("acc", &[up("A", Some("1"), Some("Alpha"))], true)
            .unwrap();
        assert_eq!(
            c.set_playtime("acc", &[("art-A".into(), 3600)]).unwrap(),
            vec!["A"]
        );
        assert!(c
            .set_playtime("acc", &[("art-A".into(), 3600)])
            .unwrap()
            .is_empty());

        assert_eq!(c.library_token("acc").unwrap(), None);
        c.set_sync_state("acc", Some("st-1"), true).unwrap();
        c.set_sync_state("acc", None, false).unwrap();
        assert_eq!(c.library_token("acc").unwrap().as_deref(), Some("st-1"));
    }
}

/// Shared by other modules' tests: a cache with one signed-in account and one game.
#[cfg(test)]
pub(crate) mod tests_support {
    use super::*;

    pub fn cache_with_game(app: &str, latest: &str) -> Cache {
        let c = Cache::open_in_memory().unwrap();
        c.upsert_signed_in_account(&crate::AccountUpsert {
            account_id: "acc",
            display_name: "Player",
            email: None,
            country: None,
        })
        .unwrap();
        c.apply_library(
            "acc",
            &[GameUpsert {
                app_name: app.into(),
                namespace: "ns".into(),
                catalog_item_id: "id".into(),
                latest_version: Some(latest.into()),
                record_type: Some("APPLICATION".into()),
                acquired_at: None,
                fallback_title: app.into(),
                catalog: None,
            }],
            true,
        )
        .unwrap();
        c
    }
}
