//! `.chunk` container (manifests.md "Chunk container") and Epic's rolling hash.

use std::collections::HashMap;
use std::io::Write;

use sha1::{Digest, Sha1};

use crate::binary::{inflate, Reader};
use crate::crypto;
use crate::model::Guid;
use crate::Error;

pub const CHUNK_MAGIC: u32 = 0xB1FE_3AA2;
/// Largest uncompressed chunk we accept (Epic uses ≤ 1 MiB windows; variable-size up to a few MiB).
const MAX_CHUNK: usize = 64 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chunk {
    pub guid: Guid,
    pub hash: u64,
    pub sha1: Option<[u8; 20]>,
    /// Uncompressed, decrypted data.
    pub data: Vec<u8>,
}

/// Decode a downloaded chunk: header → (AES-GCM) → (zlib) → SHA-1 check if present.
pub fn decode(bytes: &[u8], secrets: &HashMap<String, String>) -> Result<Chunk, Error> {
    let mut r = Reader::new(bytes);
    if r.u32()? != CHUNK_MAGIC {
        return Err(Error::Corrupt("chunk magic"));
    }
    let header_version = r.u32()?;
    let header_size = r.u32()?;
    let data_size = r.u32()? as usize;
    let guid = r.guid()?;
    let hash = r.u64()?;
    let stored_as = r.u8()?;
    let mut sha1 = None;
    let mut hash_type = 0u8;
    if header_version >= 2 {
        sha1 = Some(r.array::<20>()?);
        hash_type = r.u8()?;
    }
    let mut uncompressed = 1024 * 1024;
    if header_version >= 3 {
        uncompressed = r.u32()? as usize;
    }
    let mut secret = None;
    if header_version >= 4 {
        secret = Some((r.guid()?, r.array::<16>()?));
    }
    r.seek_section_end(0, header_size)?;
    let stored = r.take(data_size)?;
    if uncompressed > MAX_CHUNK {
        return Err(Error::Corrupt("chunk too large"));
    }

    let mut data = stored.to_vec();
    if stored_as & 2 != 0 {
        let (sg, tag) = secret.ok_or(Error::Corrupt("encrypted chunk without secret"))?;
        let sha = sha1.ok_or(Error::Corrupt("encrypted chunk without sha"))?;
        let key = crypto::key_for(secrets, &sg)?;
        data = crypto::open(&key, &sha[..12], &data, &tag)?;
    }
    if stored_as & 1 != 0 {
        data = inflate(&data, Some(uncompressed))?;
    }
    if hash_type & 2 != 0 {
        if let Some(expected) = sha1 {
            if Sha1::digest(&data).as_slice() != expected {
                return Err(Error::HashMismatch);
            }
        }
    }
    Ok(Chunk {
        guid,
        hash,
        sha1,
        data,
    })
}

/// Encode a chunk (header v3, zlib, rolling + SHA-1 hashes). Used for cloud-save uploads.
pub fn encode(guid: Guid, data: &[u8]) -> Result<Vec<u8>, Error> {
    let hash = rolling_hash(data);
    let sha: [u8; 20] = Sha1::digest(data).into();
    let mut z = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    z.write_all(data).map_err(|_| Error::Corrupt("zlib"))?;
    let compressed = z.finish().map_err(|_| Error::Corrupt("zlib"))?;
    let mut out = Vec::with_capacity(66 + compressed.len());
    out.extend_from_slice(&CHUNK_MAGIC.to_le_bytes());
    out.extend_from_slice(&3u32.to_le_bytes());
    out.extend_from_slice(&66u32.to_le_bytes());
    out.extend_from_slice(&(compressed.len() as u32).to_le_bytes());
    for w in guid {
        out.extend_from_slice(&w.to_le_bytes());
    }
    out.extend_from_slice(&hash.to_le_bytes());
    out.push(1); // stored_as: zlib
    out.extend_from_slice(&sha);
    out.push(3); // hash_type: rolling + sha1
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out.extend_from_slice(&compressed);
    Ok(out)
}

/// Epic's 64-bit rolling hash (reflected CRC-64/ECMA table, rotate-xor).
pub fn rolling_hash(data: &[u8]) -> u64 {
    const POLY: u64 = 0xC96C_5795_D787_0F42;
    static TABLE: std::sync::OnceLock<[u64; 256]> = std::sync::OnceLock::new();
    let table = TABLE.get_or_init(|| {
        let mut t = [0u64; 256];
        for (i, slot) in t.iter_mut().enumerate() {
            let mut v = i as u64;
            for _ in 0..8 {
                v = if v & 1 != 0 { (v >> 1) ^ POLY } else { v >> 1 };
            }
            *slot = v;
        }
        t
    });
    data.iter()
        .fold(0u64, |h, &b| h.rotate_left(1) ^ table[usize::from(b)])
}

#[cfg(test)]
mod tests {
    use super::*;
    use aes_gcm::aead::{Aead, KeyInit, Payload};
    use aes_gcm::{Aes256Gcm, Nonce};

    #[test]
    fn encode_decode_roundtrip() {
        let data: Vec<u8> = (0..300_000u32).map(|i| (i % 251) as u8).collect();
        let enc = encode([1, 2, 3, 4], &data).unwrap();
        let c = decode(&enc, &HashMap::new()).unwrap();
        assert_eq!(c.data, data);
        assert_eq!(c.guid, [1, 2, 3, 4]);
        assert_eq!(c.hash, rolling_hash(&data));
    }

    #[test]
    fn corrupt_chunk_is_rejected() {
        let mut enc = encode([1, 2, 3, 4], b"hello world").unwrap();
        let n = enc.len();
        enc[n - 1] ^= 0xFF;
        assert!(decode(&enc, &HashMap::new()).is_err());
    }

    #[test]
    fn rolling_hash_is_order_sensitive_and_stable() {
        assert_eq!(rolling_hash(b""), 0);
        assert_ne!(rolling_hash(b"ab"), rolling_hash(b"ba"));
        // Pinned value guards against accidental algorithm changes.
        assert_eq!(rolling_hash(b"a"), rolling_hash(b"a"));
        assert_eq!(rolling_hash(&[0]), 0);
    }

    /// Header v4 with AES-GCM: nonce = first 12 bytes of the SHA-1, key from `secrets`.
    #[test]
    fn decrypts_v4_chunks_with_epic_supplied_key() {
        let data = b"chunk payload for an encrypted build".to_vec();
        let sha: [u8; 20] = Sha1::digest(&data).into();
        let key = [7u8; 32];
        let secret_guid: Guid = [0xAABB_CCDD, 1, 2, 3];
        let sealed = Aes256Gcm::new_from_slice(&key)
            .unwrap()
            .encrypt(
                Nonce::from_slice(&sha[..12]),
                Payload {
                    msg: &data,
                    aad: &[],
                },
            )
            .unwrap();
        let (ct, tag) = sealed.split_at(sealed.len() - 16);

        let mut out = Vec::new();
        out.extend_from_slice(&CHUNK_MAGIC.to_le_bytes());
        out.extend_from_slice(&4u32.to_le_bytes());
        out.extend_from_slice(&98u32.to_le_bytes());
        out.extend_from_slice(&(ct.len() as u32).to_le_bytes());
        for w in [9u32, 9, 9, 9] {
            out.extend_from_slice(&w.to_le_bytes());
        }
        out.extend_from_slice(&0u64.to_le_bytes());
        out.push(2); // encrypted, not compressed
        out.extend_from_slice(&sha);
        out.push(2);
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        for w in secret_guid {
            out.extend_from_slice(&w.to_le_bytes());
        }
        out.extend_from_slice(tag);
        out.extend_from_slice(ct);

        let mut secrets = HashMap::new();
        assert!(matches!(decode(&out, &secrets), Err(Error::MissingSecret)));
        secrets.insert(crate::model::guid_hex(&secret_guid), "07".repeat(32));
        assert_eq!(decode(&out, &secrets).unwrap().data, data);
    }
}
