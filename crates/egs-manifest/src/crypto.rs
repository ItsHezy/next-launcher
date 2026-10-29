//! AES-GCM for encrypted manifests and chunks (feature level ≥ 22/24).
//!
//! Keys come only from Epic's assets API for the signed-in user's own entitlement
//! (`elements[0].secrets`, docs/protocol/downloads-cdn.md). Without a key we stop.

use std::collections::HashMap;

use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes128Gcm, Aes256Gcm, Nonce};

use crate::binary::{inflate, Reader};
use crate::model::{guid_hex, Guid, Manifest};
use crate::Error;

fn hex(s: &str) -> Result<Vec<u8>, Error> {
    if s.len() % 2 != 0 {
        return Err(Error::Corrupt("secret hex"));
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).map_err(|_| Error::Corrupt("secret hex")))
        .collect()
}

/// Look up the key for `guid` in Epic's `secrets` map (keys are `%08X`×4).
pub fn key_for(secrets: &HashMap<String, String>, guid: &Guid) -> Result<Vec<u8>, Error> {
    let id = guid_hex(guid);
    let value = secrets
        .get(&id)
        .or_else(|| {
            secrets
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(&id))
                .map(|(_, v)| v)
        })
        .ok_or(Error::MissingSecret)?;
    hex(value)
}

/// Decrypt `ciphertext` with a detached `tag`.
pub fn open(key: &[u8], nonce: &[u8], ciphertext: &[u8], tag: &[u8; 16]) -> Result<Vec<u8>, Error> {
    if nonce.len() != 12 {
        return Err(Error::Corrupt("nonce length"));
    }
    let mut msg = Vec::with_capacity(ciphertext.len() + 16);
    msg.extend_from_slice(ciphertext);
    msg.extend_from_slice(tag);
    let payload = Payload {
        msg: &msg,
        aad: &[],
    };
    let nonce = Nonce::from_slice(nonce);
    let out = match key.len() {
        32 => Aes256Gcm::new_from_slice(key)
            .map_err(|_| Error::Decrypt)?
            .decrypt(nonce, payload),
        16 => Aes128Gcm::new_from_slice(key)
            .map_err(|_| Error::Decrypt)?
            .decrypt(nonce, payload),
        _ => return Err(Error::Corrupt("key length")),
    };
    out.map_err(|_| Error::Decrypt)
}

/// Fill in the strings hidden in an encrypted manifest (manifests.md §5).
pub fn decrypt_manifest(m: &mut Manifest, secrets: &HashMap<String, String>) -> Result<(), Error> {
    let Some(enc) = m.encrypted.take() else {
        return Ok(());
    };
    let guid = m.secret_guid.ok_or(Error::Corrupt("no secret guid"))?;
    let tag = m
        .encryption_tag
        .ok_or(Error::Corrupt("no encryption tag"))?;
    let key = match key_for(secrets, &guid) {
        Ok(k) => k,
        Err(e) => {
            m.encrypted = Some(enc);
            return Err(e);
        }
    };
    let plain = open(&key, &enc.iv, &enc.ciphertext, &tag)?;
    let plain = if enc.compressed {
        inflate(&plain, None)?
    } else {
        plain
    };

    let mut r = Reader::new(&plain);
    m.meta.launch_exe = r.fstring()?;
    m.meta.launch_command = r.fstring()?;
    let n = r.count()?;
    m.meta.prereq_ids = (0..n).map(|_| r.fstring()).collect::<Result<_, _>>()?;
    m.meta.prereq_name = r.fstring()?;
    m.meta.prereq_path = r.fstring()?;
    m.meta.prereq_args = r.fstring()?;
    m.meta.uninstall_action_path = r.fstring()?;
    m.meta.uninstall_action_args = r.fstring()?;
    for f in m.files.iter_mut() {
        f.filename = r.fstring()?;
        f.symlink_target = r.fstring()?;
    }
    m.secret_guid = None;
    m.encryption_tag = None;
    Ok(())
}
