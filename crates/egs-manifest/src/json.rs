//! JSON manifests (older titles). Numbers are "blobs": each byte as 3 decimal
//! digits, little-endian (manifests.md "JSON manifest").

use std::collections::BTreeMap;

use serde_json::Value;

use crate::model::{ChunkInfo, ChunkPart, FileManifest, Guid, Manifest, Meta};
use crate::Error;

/// `"013000000000"` → 13.
pub fn blob_to_u64(s: &str) -> Result<u64, Error> {
    if s.len() % 3 != 0 || s.len() > 24 {
        return Err(Error::Corrupt("blob number"));
    }
    let mut n: u64 = 0;
    for (i, chunk) in s.as_bytes().chunks(3).enumerate() {
        let digits = std::str::from_utf8(chunk).map_err(|_| Error::Corrupt("blob number"))?;
        let byte: u64 = digits.parse().map_err(|_| Error::Corrupt("blob number"))?;
        if byte > 255 {
            return Err(Error::Corrupt("blob byte"));
        }
        n |= byte << (8 * i);
    }
    Ok(n)
}

/// Blob bytes in order (for 20-byte SHA-1 file hashes).
pub fn blob_to_bytes(s: &str) -> Result<Vec<u8>, Error> {
    if s.len() % 3 != 0 {
        return Err(Error::Corrupt("blob bytes"));
    }
    s.as_bytes()
        .chunks(3)
        .map(|c| {
            std::str::from_utf8(c)
                .ok()
                .and_then(|d| d.parse::<u16>().ok())
                .filter(|v| *v <= 255)
                .map(|v| v as u8)
                .ok_or(Error::Corrupt("blob bytes"))
        })
        .collect()
}

/// 32 hex chars read big-endian as 4 × u32.
fn guid(s: &str) -> Result<Guid, Error> {
    if s.len() != 32 {
        return Err(Error::Corrupt("guid"));
    }
    let mut g = [0u32; 4];
    for (i, w) in g.iter_mut().enumerate() {
        *w = u32::from_str_radix(&s[i * 8..i * 8 + 8], 16).map_err(|_| Error::Corrupt("guid"))?;
    }
    Ok(g)
}

fn s(v: &Value, k: &str) -> String {
    v.get(k)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

fn blob(v: &Value, k: &str, default: u64) -> Result<u64, Error> {
    match v.get(k).and_then(Value::as_str) {
        Some(x) => blob_to_u64(x),
        None => Ok(default),
    }
}

fn crc_group(g: &Guid) -> u8 {
    let mut crc = flate2::Crc::new();
    crc.update(&crate::model::guid_le_bytes(g));
    (crc.sum() % 100) as u8
}

pub(crate) fn parse(bytes: &[u8]) -> Result<Manifest, Error> {
    let v: Value = serde_json::from_slice(bytes).map_err(|_| Error::NotAManifest)?;
    let version = blob(&v, "ManifestFileVersion", 13)? as u32;
    let meta = Meta {
        data_version: 0,
        feature_level: version,
        is_file_data: v
            .get("bIsFileData")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        app_id: blob(&v, "AppID", 0)? as u32,
        app_name: s(&v, "AppNameString"),
        build_version: s(&v, "BuildVersionString"),
        launch_exe: s(&v, "LaunchExeString"),
        launch_command: s(&v, "LaunchCommand"),
        prereq_ids: v
            .get("PrereqIds")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default(),
        prereq_name: s(&v, "PrereqName"),
        prereq_path: s(&v, "PrereqPath"),
        prereq_args: s(&v, "PrereqArgs"),
        ..Meta::default()
    };

    let empty = serde_json::Map::new();
    let obj = |k: &str| v.get(k).and_then(Value::as_object).unwrap_or(&empty);
    let (sizes, hashes, shas, groups) = (
        obj("ChunkFilesizeList"),
        obj("ChunkHashList"),
        obj("ChunkShaList"),
        obj("DataGroupList"),
    );
    let mut chunks = Vec::with_capacity(sizes.len());
    for (id, size) in sizes {
        let g = guid(id)?;
        let sha_hex = shas.get(id).and_then(Value::as_str).unwrap_or_default();
        let mut sha1 = [0u8; 20];
        if sha_hex.len() == 40 {
            for (i, b) in sha1.iter_mut().enumerate() {
                *b = u8::from_str_radix(&sha_hex[i * 2..i * 2 + 2], 16)
                    .map_err(|_| Error::Corrupt("chunk sha"))?;
            }
        }
        chunks.push(ChunkInfo {
            guid: g,
            hash: hashes
                .get(id)
                .and_then(Value::as_str)
                .map(blob_to_u64)
                .transpose()?
                .unwrap_or(0),
            sha1,
            group: match groups.get(id).and_then(Value::as_str) {
                Some(x) => blob_to_u64(x)? as u8,
                None => crc_group(&g),
            },
            window_size: 1024 * 1024,
            file_size: size.as_str().map(blob_to_u64).transpose()?.unwrap_or(0) as i64,
            secret_guid: None,
            window_size_compressed: None,
            encryption_tag: None,
        });
    }

    let mut files = Vec::new();
    for f in v
        .get("FileManifestList")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let mut sha1 = [0u8; 20];
        let hb = blob_to_bytes(
            f.get("FileHash")
                .and_then(Value::as_str)
                .unwrap_or_default(),
        )?;
        for (i, b) in hb.iter().take(20).enumerate() {
            sha1[i] = *b;
        }
        let mut flags = 0u8;
        if f.get("bIsReadOnly")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            flags |= 1;
        }
        if f.get("bIsCompressed")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            flags |= 2;
        }
        if f.get("bIsUnixExecutable")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            flags |= 4;
        }
        let mut parts = Vec::new();
        let mut offset = 0u64;
        for p in f
            .get("FileChunkParts")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let size = blob(p, "Size", 0)? as u32;
            parts.push(ChunkPart {
                guid: guid(p.get("Guid").and_then(Value::as_str).unwrap_or_default())?,
                offset: blob(p, "Offset", 0)? as u32,
                size,
                file_offset: offset,
            });
            offset += u64::from(size);
        }
        files.push(FileManifest {
            filename: s(f, "Filename"),
            symlink_target: String::new(),
            sha1,
            flags,
            install_tags: f
                .get("InstallTags")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(|x| x.as_str().map(str::to_owned))
                        .collect()
                })
                .unwrap_or_default(),
            parts,
            md5: None,
            mime_type: None,
            sha256: None,
        });
    }

    let custom_fields: BTreeMap<String, String> = v
        .get("CustomFields")
        .and_then(Value::as_object)
        .map(|m| {
            m.iter()
                .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_owned())))
                .collect()
        })
        .unwrap_or_default();

    Ok(Manifest {
        version,
        json: true,
        meta,
        chunks,
        files,
        custom_fields,
        secret_guid: None,
        encryption_tag: None,
        encrypted: None,
    })
}
