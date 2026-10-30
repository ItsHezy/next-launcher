//! Download queue and install records (Phase 4). Survives restarts: a job that
//! was `running` when the app closed is picked up again as resumable.

use std::collections::HashSet;

use rusqlite::{params, OptionalExtension};

use crate::{now_secs, Cache, Error};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadJob {
    pub id: i64,
    pub owner: String,
    pub app_name: String,
    pub kind: String,
    pub state: String,
    pub install_path: String,
    pub build_version: String,
    pub tags: Vec<String>,
    pub bytes_total: i64,
}

/// What we know about one game, for install/verify/uninstall/launch decisions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GameInstall {
    pub app_name: String,
    pub namespace: String,
    pub catalog_item_id: String,
    pub title: String,
    pub state: String,
    pub version: Option<String>,
    pub latest_version: Option<String>,
    pub install_path: Option<String>,
    pub install_tags: Vec<String>,
    pub egl_managed: bool,
    /// The official launcher's `InstallationGuid` of an adopted install.
    pub egl_guid: Option<String>,
    pub needs_verify: bool,
    pub owner: Option<String>,
    pub meta_json: Option<String>,
    /// `releaseInfo` app id (playtime reports).
    pub artifact_id: Option<String>,
}

fn tags_from(json: Option<String>) -> Vec<String> {
    json.and_then(|j| serde_json::from_str(&j).ok())
        .unwrap_or_default()
}

impl Cache {
    pub fn game_install(&self, app_name: &str) -> Result<Option<GameInstall>, Error> {
        let conn = self.lock()?;
        Ok(conn
            .query_row(
                "SELECT app_name, namespace, catalog_item_id, title, install_state, version, latest_version,
                        install_path, install_tags, egl_managed, needs_verify, owner, meta_json, artifact_id,
                        egl_guid
                 FROM games WHERE app_name = ?1",
                params![app_name],
                |r| {
                    Ok(GameInstall {
                        app_name: r.get(0)?,
                        namespace: r.get(1)?,
                        catalog_item_id: r.get(2)?,
                        title: r.get(3)?,
                        state: r.get(4)?,
                        version: r.get(5)?,
                        latest_version: r.get(6)?,
                        install_path: r.get(7)?,
                        install_tags: tags_from(r.get(8)?),
                        egl_managed: r.get(9)?,
                        needs_verify: r.get(10)?,
                        owner: r.get(11)?,
                        meta_json: r.get(12)?,
                        artifact_id: r.get(13)?,
                        egl_guid: r.get(14)?,
                    })
                },
            )
            .optional()?)
    }

    /// Queue a download. Fails (returns `None`) if one is already active for the app.
    #[allow(clippy::too_many_arguments)]
    pub fn enqueue_download(
        &self,
        owner: &str,
        app_name: &str,
        kind: &str,
        install_path: &str,
        build_version: &str,
        tags: &[String],
        bytes_total: i64,
    ) -> Result<Option<i64>, Error> {
        let mut conn = self.lock()?;
        let tx = conn.transaction()?;
        let busy: Option<i64> = tx
            .query_row(
                "SELECT id FROM downloads WHERE app_name = ?1 AND state IN ('queued','running','paused')",
                params![app_name],
                |r| r.get(0),
            )
            .optional()?;
        if busy.is_some() {
            return Ok(None);
        }
        let pos: i64 = tx.query_row(
            "SELECT COALESCE(MAX(queue_pos), 0) + 1 FROM downloads",
            [],
            |r| r.get(0),
        )?;
        let now = now_secs();
        tx.execute(
            "INSERT INTO downloads (app_name, kind, state, queue_pos, bytes_done, bytes_total, owner, install_path,
                 build_version, tags, created_at, updated_at)
             VALUES (?1, ?2, 'queued', ?3, 0, ?4, ?5, ?6, ?7, ?8, ?9, ?9)",
            params![
                app_name,
                kind,
                pos,
                bytes_total,
                owner,
                install_path,
                build_version,
                serde_json::to_string(tags)?,
                now
            ],
        )?;
        let id = tx.last_insert_rowid();
        if kind == "install" {
            tx.execute(
                "UPDATE games SET install_state = 'installing', install_path = ?2, updated_at = ?3 WHERE app_name = ?1",
                params![app_name, install_path, now],
            )?;
        }
        tx.commit()?;
        Ok(Some(id))
    }

    /// The next job to run: one interrupted while running, else the first queued.
    pub fn next_download(&self) -> Result<Option<DownloadJob>, Error> {
        let conn = self.lock()?;
        Ok(conn
            .query_row(
                "SELECT id, COALESCE(owner, ''), app_name, kind, state, COALESCE(install_path, ''),
                        COALESCE(build_version, ''), tags, bytes_total
                 FROM downloads WHERE state IN ('running', 'queued')
                 ORDER BY CASE state WHEN 'running' THEN 0 ELSE 1 END, queue_pos
                 LIMIT 1",
                [],
                |r| {
                    Ok(DownloadJob {
                        id: r.get(0)?,
                        owner: r.get(1)?,
                        app_name: r.get(2)?,
                        kind: r.get(3)?,
                        state: r.get(4)?,
                        install_path: r.get(5)?,
                        build_version: r.get(6)?,
                        tags: tags_from(r.get(7)?),
                        bytes_total: r.get(8)?,
                    })
                },
            )
            .optional()?)
    }

    pub fn next_download_by_id(&self, id: i64) -> Result<Option<DownloadJob>, Error> {
        let conn = self.lock()?;
        Ok(conn
            .query_row(
                "SELECT id, COALESCE(owner, ''), app_name, kind, state, COALESCE(install_path, ''),
                        COALESCE(build_version, ''), tags, bytes_total
                 FROM downloads WHERE id = ?1",
                params![id],
                |r| {
                    Ok(DownloadJob {
                        id: r.get(0)?,
                        owner: r.get(1)?,
                        app_name: r.get(2)?,
                        kind: r.get(3)?,
                        state: r.get(4)?,
                        install_path: r.get(5)?,
                        build_version: r.get(6)?,
                        tags: tags_from(r.get(7)?),
                        bytes_total: r.get(8)?,
                    })
                },
            )
            .optional()?)
    }

    pub fn set_download_state(
        &self,
        id: i64,
        state: &str,
        error: Option<&str>,
    ) -> Result<(), Error> {
        let conn = self.lock()?;
        conn.execute(
            "UPDATE downloads SET state = ?2, error = ?3, speed_bps = CASE WHEN ?2 = 'running' THEN speed_bps ELSE 0 END,
                 updated_at = ?4 WHERE id = ?1",
            params![id, state, error, now_secs()],
        )?;
        Ok(())
    }

    pub fn set_download_progress(
        &self,
        id: i64,
        done: i64,
        total: i64,
        speed: i64,
    ) -> Result<(), Error> {
        let conn = self.lock()?;
        conn.execute(
            "UPDATE downloads SET bytes_done = ?2, bytes_total = ?3, speed_bps = ?4, updated_at = ?5 WHERE id = ?1",
            params![id, done, total, speed, now_secs()],
        )?;
        Ok(())
    }

    pub fn download_file_done(&self, id: i64, filename: &str) -> Result<(), Error> {
        let conn = self.lock()?;
        conn.execute(
            "INSERT OR IGNORE INTO download_files (download_id, filename) VALUES (?1, ?2)",
            params![id, filename],
        )?;
        Ok(())
    }

    pub fn download_files_done(&self, id: i64) -> Result<HashSet<String>, Error> {
        let conn = self.lock()?;
        let mut stmt =
            conn.prepare_cached("SELECT filename FROM download_files WHERE download_id = ?1")?;
        let rows = stmt.query_map(params![id], |r| r.get(0))?;
        rows.collect::<Result<_, _>>().map_err(Error::from)
    }

    /// Job finished: record the install and drop the queue entry.
    pub fn finish_install(
        &self,
        id: i64,
        app_name: &str,
        version: &str,
        install_path: &str,
        install_size: u64,
        tags: &[String],
    ) -> Result<(), Error> {
        let mut conn = self.lock()?;
        let tx = conn.transaction()?;
        let latest: Option<String> = tx
            .query_row(
                "SELECT latest_version FROM games WHERE app_name = ?1",
                params![app_name],
                |r| r.get(0),
            )
            .optional()?
            .flatten();
        let state = if latest.as_deref().is_some_and(|l| l != version) {
            "updateAvailable"
        } else {
            "installed"
        };
        tx.execute(
            "UPDATE games SET install_state = ?2, version = ?3, install_path = ?4, install_size = ?5,
                 install_tags = ?6, needs_verify = 0, updated_at = ?7 WHERE app_name = ?1",
            params![app_name, state, version, install_path, install_size as i64, serde_json::to_string(tags)?, now_secs()],
        )?;
        tx.execute("DELETE FROM downloads WHERE id = ?1", params![id])?;
        tx.commit()?;
        Ok(())
    }

    /// Job removed (cancelled/failed): a fresh install goes back to "not installed".
    pub fn drop_download(&self, id: i64) -> Result<(), Error> {
        let mut conn = self.lock()?;
        let tx = conn.transaction()?;
        let row: Option<(String, String)> = tx
            .query_row(
                "SELECT app_name, kind FROM downloads WHERE id = ?1",
                params![id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        if let Some((app, kind)) = row {
            if kind == "install" {
                tx.execute(
                    "UPDATE games SET install_state = 'notInstalled', install_path = NULL, updated_at = ?2
                     WHERE app_name = ?1 AND install_state = 'installing'",
                    params![app, now_secs()],
                )?;
            }
            tx.execute("DELETE FROM downloads WHERE id = ?1", params![id])?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn mark_uninstalled(&self, app_name: &str) -> Result<(), Error> {
        let conn = self.lock()?;
        conn.execute(
            "UPDATE games SET install_state = 'notInstalled', version = NULL, install_path = NULL, install_size = 0,
                 install_tags = NULL, egl_managed = 0, egl_guid = NULL, needs_verify = 0, updated_at = ?2
             WHERE app_name = ?1",
            params![app_name, now_secs()],
        )?;
        Ok(())
    }

    pub fn set_needs_verify(&self, app_name: &str, needs: bool) -> Result<(), Error> {
        let conn = self.lock()?;
        conn.execute(
            "UPDATE games SET needs_verify = ?2, updated_at = ?3 WHERE app_name = ?1",
            params![app_name, needs, now_secs()],
        )?;
        Ok(())
    }

    /// Id of the queued, running or paused job for this game, if any.
    pub fn active_download(&self, app_name: &str) -> Result<Option<i64>, Error> {
        let conn = self.lock()?;
        Ok(conn
            .query_row(
                "SELECT id FROM downloads WHERE app_name = ?1 AND state IN ('queued','running','paused')",
                params![app_name],
                |r| r.get(0),
            )
            .optional()?)
    }

    /// The install now lives at `path`. A folder moved away from the official
    /// launcher's location is no longer shared with it.
    pub fn set_install_moved(&self, app_name: &str, path: &str) -> Result<(), Error> {
        let conn = self.lock()?;
        conn.execute(
            "UPDATE games SET install_path = ?2, egl_managed = 0, egl_guid = NULL, updated_at = ?3
             WHERE app_name = ?1",
            params![app_name, path, now_secs()],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::sync::tests_support::cache_with_game;

    #[test]
    fn queue_lifecycle_and_resume_records() {
        let c = cache_with_game("A", "2");
        let id = c
            .enqueue_download(
                "acc",
                "A",
                "install",
                "C:/Games/A",
                "2",
                &["core".into()],
                1000,
            )
            .unwrap()
            .unwrap();
        assert!(
            c.enqueue_download("acc", "A", "install", "x", "2", &[], 1)
                .unwrap()
                .is_none(),
            "one active job per game"
        );
        assert_eq!(c.game_install("A").unwrap().unwrap().state, "installing");

        assert_eq!(c.active_download("A").unwrap(), Some(id));
        let job = c.next_download().unwrap().unwrap();
        assert_eq!((job.id, job.tags.clone()), (id, vec!["core".to_owned()]));
        c.set_download_state(id, "running", None).unwrap();
        c.download_file_done(id, "a.pak").unwrap();
        c.download_file_done(id, "a.pak").unwrap();
        assert_eq!(c.download_files_done(id).unwrap().len(), 1);
        // App restarted while running: still picked up first.
        assert_eq!(c.next_download().unwrap().unwrap().state, "running");

        c.finish_install(id, "A", "2", "C:/Games/A", 500, &["core".into()])
            .unwrap();
        let g = c.game_install("A").unwrap().unwrap();
        assert_eq!(
            (g.state.as_str(), g.version.as_deref(), g.install_tags),
            ("installed", Some("2"), vec!["core".to_owned()])
        );
        assert!(c.next_download().unwrap().is_none());
        assert_eq!(c.active_download("A").unwrap(), None);

        c.set_install_moved("A", "D:/Games/A").unwrap();
        let g = c.game_install("A").unwrap().unwrap();
        assert_eq!(
            (g.install_path.as_deref(), g.egl_managed),
            (Some("D:/Games/A"), false)
        );

        c.mark_uninstalled("A").unwrap();
        assert_eq!(c.game_install("A").unwrap().unwrap().state, "notInstalled");
    }

    #[test]
    fn cancelled_install_reverts_state() {
        let c = cache_with_game("A", "2");
        let id = c
            .enqueue_download("acc", "A", "install", "C:/Games/A", "2", &[], 10)
            .unwrap()
            .unwrap();
        c.drop_download(id).unwrap();
        let g = c.game_install("A").unwrap().unwrap();
        assert_eq!(g.state, "notInstalled");
        assert!(g.install_path.is_none());
        assert!(c.next_download().unwrap().is_none());
    }
}
