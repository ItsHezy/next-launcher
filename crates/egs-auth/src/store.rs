//! Refresh-token storage in the OS credential store (Windows Credential Manager).
//!
//! Credential Manager caps one secret at 2,560 bytes and an `eg1` refresh token
//! is a JWT that can approach that, so the payload is split across numbered
//! entries. Chunk 0 carries a header with the part count and is written last.

use egs_api::Secret;
use serde::{Deserialize, Serialize};

const SERVICE: &str = "NextLauncher";
const CHUNK: usize = 2000;

/// What survives a restart. The access token is never stored.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredSession {
    pub account_id: String,
    pub refresh_token: Secret,
    /// Unix seconds; `0` if Epic didn't say.
    pub refresh_expires_at: u64,
}

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("credential store: {0}")]
    Backend(String),
    #[error("stored session is corrupt")]
    Corrupt,
}

pub trait TokenStore: Send + Sync {
    fn load(&self, account_id: &str) -> Result<Option<StoredSession>, StoreError>;
    fn save(&self, session: &StoredSession) -> Result<(), StoreError>;
    fn delete(&self, account_id: &str) -> Result<(), StoreError>;
}

/// Production store backed by `keyring` (Windows: Credential Manager, generic credentials).
#[derive(Debug, Default)]
pub struct KeyringStore;

fn entry(account_id: &str, part: usize) -> Result<keyring::Entry, StoreError> {
    let user = if part == 0 {
        account_id.to_owned()
    } else {
        format!("{account_id}#{part}")
    };
    keyring::Entry::new(SERVICE, &user).map_err(|e| StoreError::Backend(e.to_string()))
}

fn read_part(account_id: &str, part: usize) -> Result<Option<Vec<u8>>, StoreError> {
    match entry(account_id, part)?.get_secret() {
        Ok(b) => Ok(Some(b)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(StoreError::Backend(e.to_string())),
    }
}

fn delete_part(account_id: &str, part: usize) -> Result<(), StoreError> {
    match entry(account_id, part)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(StoreError::Backend(e.to_string())),
    }
}

/// Header of chunk 0: `NL1 <parts>\n`.
fn split_header(first: &[u8]) -> Result<(usize, &[u8]), StoreError> {
    let nl = first
        .iter()
        .position(|&b| b == b'\n')
        .ok_or(StoreError::Corrupt)?;
    let header = std::str::from_utf8(&first[..nl]).map_err(|_| StoreError::Corrupt)?;
    let parts = header
        .strip_prefix("NL1 ")
        .and_then(|n| n.parse::<usize>().ok())
        .filter(|n| (1..=16).contains(n))
        .ok_or(StoreError::Corrupt)?;
    Ok((parts, &first[nl + 1..]))
}

impl TokenStore for KeyringStore {
    fn load(&self, account_id: &str) -> Result<Option<StoredSession>, StoreError> {
        let Some(first) = read_part(account_id, 0)? else {
            return Ok(None);
        };
        let (parts, body0) = split_header(&first)?;
        let mut payload = body0.to_vec();
        for p in 1..parts {
            payload.extend(read_part(account_id, p)?.ok_or(StoreError::Corrupt)?);
        }
        serde_json::from_slice(&payload)
            .map(Some)
            .map_err(|_| StoreError::Corrupt)
    }

    fn save(&self, session: &StoredSession) -> Result<(), StoreError> {
        let id = &session.account_id;
        let previous_parts = read_part(id, 0)?
            .and_then(|b| split_header(&b).ok().map(|(n, _)| n))
            .unwrap_or(1);
        let payload = serde_json::to_vec(session).map_err(|_| StoreError::Corrupt)?;
        let chunks: Vec<&[u8]> = payload.chunks(CHUNK).collect();
        let parts = chunks.len().max(1);

        for (i, chunk) in chunks.iter().enumerate().skip(1) {
            entry(id, i)?
                .set_secret(chunk)
                .map_err(|e| StoreError::Backend(e.to_string()))?;
        }
        let mut first = format!("NL1 {parts}\n").into_bytes();
        first.extend_from_slice(chunks.first().copied().unwrap_or_default());
        entry(id, 0)?
            .set_secret(&first)
            .map_err(|e| StoreError::Backend(e.to_string()))?;

        for stale in parts..previous_parts {
            delete_part(id, stale)?;
        }
        Ok(())
    }

    fn delete(&self, account_id: &str) -> Result<(), StoreError> {
        let parts = read_part(account_id, 0)?
            .and_then(|b| split_header(&b).ok().map(|(n, _)| n))
            .unwrap_or(1);
        for p in (0..parts).rev() {
            delete_part(account_id, p)?;
        }
        Ok(())
    }
}

/// In-memory store for tests.
#[derive(Debug, Default)]
pub struct MemoryStore(std::sync::Mutex<std::collections::HashMap<String, StoredSession>>);

impl TokenStore for MemoryStore {
    fn load(&self, account_id: &str) -> Result<Option<StoredSession>, StoreError> {
        Ok(self
            .0
            .lock()
            .map_err(|_| StoreError::Corrupt)?
            .get(account_id)
            .cloned())
    }
    fn save(&self, s: &StoredSession) -> Result<(), StoreError> {
        self.0
            .lock()
            .map_err(|_| StoreError::Corrupt)?
            .insert(s.account_id.clone(), s.clone());
        Ok(())
    }
    fn delete(&self, account_id: &str) -> Result<(), StoreError> {
        self.0
            .lock()
            .map_err(|_| StoreError::Corrupt)?
            .remove(account_id);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_roundtrip_and_validation() {
        let (n, body) = split_header(b"NL1 3\n{\"a\":1}").unwrap();
        assert_eq!(n, 3);
        assert_eq!(body, b"{\"a\":1}");
        assert!(split_header(b"NL1 0\nx").is_err());
        assert!(split_header(b"garbage").is_err());
    }

    /// Exercises the real Windows Credential Manager with a throwaway account id.
    #[cfg(windows)]
    #[test]
    fn keyring_roundtrip_with_chunking() {
        let store = KeyringStore;
        let id = format!("test-{}", std::process::id());
        // ~5 KB token forces three chunks; then a short one must clean up the extra parts.
        let long = StoredSession {
            account_id: id.clone(),
            refresh_token: Secret::new(format!("eg1~{}", "x".repeat(5000))),
            refresh_expires_at: 123,
        };
        store.save(&long).unwrap();
        let back = store.load(&id).unwrap().unwrap();
        assert_eq!(back.refresh_token.expose(), long.refresh_token.expose());
        assert_eq!(back.refresh_expires_at, 123);

        let short = StoredSession {
            refresh_token: Secret::new("eg1~short"),
            ..long
        };
        store.save(&short).unwrap();
        assert_eq!(
            store.load(&id).unwrap().unwrap().refresh_token.expose(),
            "eg1~short"
        );
        assert!(
            read_part(&id, 1).unwrap().is_none(),
            "stale chunk left behind"
        );

        store.delete(&id).unwrap();
        assert!(store.load(&id).unwrap().is_none());
    }
}
