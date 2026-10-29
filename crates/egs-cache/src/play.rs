//! Play sessions (Phase 5): last played, local playtime, and the queue of playtime
//! reports that couldn't reach Epic yet (`events_log`, never secrets).

use rusqlite::params;

use crate::{now_secs, Cache, Error};

const PLAYTIME_EVENT: &str = "playtime";

impl Cache {
    /// The game just started: it moves to the front of "Recently played".
    pub fn mark_played(&self, app_name: &str, at_secs: i64) -> Result<(), Error> {
        let conn = self.lock()?;
        conn.execute(
            "UPDATE games SET last_played = ?2, updated_at = ?3 WHERE app_name = ?1",
            params![app_name, at_secs, now_secs()],
        )?;
        Ok(())
    }

    /// A session ended: add it to the local total right away (Epic's total replaces it
    /// on the next sync, once the report has gone through).
    pub fn add_playtime(&self, app_name: &str, secs: i64) -> Result<(), Error> {
        let conn = self.lock()?;
        conn.execute(
            "UPDATE games SET playtime_secs = playtime_secs + ?2, updated_at = ?3 WHERE app_name = ?1",
            params![app_name, secs.max(0), now_secs()],
        )?;
        Ok(())
    }

    /// Keep a playtime report (JSON, no secrets) to send later.
    pub fn queue_playtime_report(&self, payload: &serde_json::Value) -> Result<(), Error> {
        let conn = self.lock()?;
        conn.execute(
            "INSERT INTO events_log (at, kind, payload) VALUES (?1, ?2, ?3)",
            params![now_secs(), PLAYTIME_EVENT, serde_json::to_string(payload)?],
        )?;
        Ok(())
    }

    /// Queued playtime reports, oldest first: `(id, payload)`.
    pub fn pending_playtime_reports(&self) -> Result<Vec<(i64, serde_json::Value)>, Error> {
        let conn = self.lock()?;
        let mut stmt =
            conn.prepare_cached("SELECT id, payload FROM events_log WHERE kind = ?1 ORDER BY id")?;
        let rows = stmt.query_map(params![PLAYTIME_EVENT], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (id, json) = row?;
            out.push((id, serde_json::from_str(&json)?));
        }
        Ok(out)
    }

    pub fn drop_event(&self, id: i64) -> Result<(), Error> {
        let conn = self.lock()?;
        conn.execute("DELETE FROM events_log WHERE id = ?1", params![id])?;
        Ok(())
    }

    /// Stable random id for this installation (playtime reports' `machineId`).
    pub fn machine_id(&self) -> Result<String, Error> {
        let conn = self.lock()?;
        let existing: Option<String> = conn
            .query_row(
                "SELECT value FROM settings WHERE key = 'internal.machineId'",
                [],
                |r| r.get(0),
            )
            .ok()
            .and_then(|v: String| serde_json::from_str(&v).ok());
        if let Some(id) = existing {
            return Ok(id);
        }
        let mut b = [0u8; 16];
        getrandom::fill(&mut b).map_err(|e| Error::Random(e.to_string()))?;
        b[6] = (b[6] & 0x0f) | 0x40; // version 4
        b[8] = (b[8] & 0x3f) | 0x80; // RFC 4122 variant
        let hex: String = b.iter().map(|x| format!("{x:02x}")).collect();
        let id = format!(
            "{}-{}-{}-{}-{}",
            &hex[..8],
            &hex[8..12],
            &hex[12..16],
            &hex[16..20],
            &hex[20..]
        );
        conn.execute(
            "INSERT INTO settings (key, value) VALUES ('internal.machineId', ?1)
             ON CONFLICT(key) DO NOTHING",
            params![serde_json::to_string(&id)?],
        )?;
        Ok(id)
    }
}

#[cfg(test)]
mod tests {
    use crate::sync::tests_support::cache_with_game;

    #[test]
    fn sessions_update_recent_and_playtime() {
        let c = cache_with_game("A", "1");
        c.mark_played("A", 1_000).unwrap();
        c.add_playtime("A", 90).unwrap();
        c.add_playtime("A", 30).unwrap();
        let snap = c.snapshot().unwrap();
        let g = snap.games.iter().find(|g| g.app_name == "A").unwrap();
        assert_eq!((g.last_played, g.playtime_secs), (Some(1_000.0), 120.0));
        assert_eq!(snap.recent, vec!["A".to_owned()]);
    }

    #[test]
    fn playtime_queue_and_machine_id() {
        let c = cache_with_game("A", "1");
        c.queue_playtime_report(&serde_json::json!({"artifactId": "x"}))
            .unwrap();
        let pending = c.pending_playtime_reports().unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].1["artifactId"], "x");
        c.drop_event(pending[0].0).unwrap();
        assert!(c.pending_playtime_reports().unwrap().is_empty());

        let id = c.machine_id().unwrap();
        assert_eq!(id.len(), 36);
        assert_eq!(&id[14..15], "4");
        assert_eq!(c.machine_id().unwrap(), id, "stable");
    }
}
