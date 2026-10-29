//! Store response cache (`store_cache`): JSON by key with an expiry (store.md "Caching").

use std::collections::HashSet;

use rusqlite::{params, OptionalExtension};

use crate::{now_secs, Cache, Error};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Owned {
    pub items: HashSet<String>,
    pub base_namespaces: HashSet<String>,
}

impl Cache {
    /// A cached store response that hasn't expired.
    pub fn store_get(&self, key: &str) -> Result<Option<String>, Error> {
        let conn = self.lock()?;
        Ok(conn
            .query_row(
                "SELECT body FROM store_cache WHERE query_hash = ?1 AND expires_at > ?2",
                params![key, now_secs()],
                |r| r.get(0),
            )
            .optional()?)
    }

    pub fn store_put(&self, key: &str, body: &str, ttl_secs: i64) -> Result<(), Error> {
        let conn = self.lock()?;
        let now = now_secs();
        conn.execute(
            "INSERT INTO store_cache (query_hash, body, expires_at) VALUES (?1, ?2, ?3)
             ON CONFLICT(query_hash) DO UPDATE SET body = excluded.body, expires_at = excluded.expires_at",
            params![key, body, now + ttl_secs.max(1)],
        )?;
        // Keep the table small: drop entries that expired a day ago.
        conn.execute(
            "DELETE FROM store_cache WHERE expires_at < ?1",
            params![now - 86_400],
        )?;
        Ok(())
    }
}

impl Cache {
    /// What the signed-in account owns, for store "In library" badges: catalog item ids,
    /// and namespaces where it owns a base game (the store sells base games under a
    /// different item id than the launcher's library uses).
    pub fn owned_items(&self) -> Result<Owned, Error> {
        let conn = self.lock()?;
        let mut stmt = conn.prepare_cached(
            "SELECT catalog_item_id, namespace, is_dlc FROM games WHERE owner =
               (SELECT account_id FROM accounts WHERE signed_in = 1 ORDER BY updated_at DESC LIMIT 1)",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, bool>(2)?,
            ))
        })?;
        let mut out = Owned::default();
        for r in rows {
            let (item, ns, dlc) = r?;
            out.items.insert(item);
            if !dlc {
                out.base_namespaces.insert(ns);
            }
        }
        Ok(out)
    }

    /// Country of the signed-in account (store prices and availability).
    pub fn signed_in_country(&self) -> Result<Option<String>, Error> {
        let conn = self.lock()?;
        Ok(conn
            .query_row(
                "SELECT country FROM accounts WHERE signed_in = 1 AND country IS NOT NULL
                 ORDER BY updated_at DESC LIMIT 1",
                [],
                |r| r.get(0),
            )
            .optional()?)
    }
}

#[cfg(test)]
mod tests {
    use crate::Cache;

    #[test]
    fn store_cache_expires() {
        let c = Cache::open_in_memory().unwrap();
        c.store_put("k", "{\"a\":1}", 60).unwrap();
        assert_eq!(c.store_get("k").unwrap().as_deref(), Some("{\"a\":1}"));
        c.store_put("k", "{}", -5).unwrap(); // clamped to 1 s, still readable now
        assert_eq!(c.store_get("k").unwrap().as_deref(), Some("{}"));
        assert_eq!(c.store_get("missing").unwrap(), None);
    }

    #[test]
    fn owned_items_belong_to_the_signed_in_account() {
        let c = crate::sync::tests_support::cache_with_game("A", "1");
        let owned = c.owned_items().unwrap();
        assert_eq!(owned.items.into_iter().collect::<Vec<_>>(), ["id"]);
        assert_eq!(
            owned.base_namespaces.into_iter().collect::<Vec<_>>(),
            ["ns"]
        );
        assert_eq!(c.signed_in_country().unwrap(), None);
    }
}
