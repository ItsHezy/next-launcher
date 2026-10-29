//! Cloud-save packaging (docs/protocol/cloud-saves.md): save files ↔ a binary manifest
//! (feature level 18) plus 1 MiB chunks, laid out the way EGL and Legendary do, so saves
//! uploaded here restore in the official launcher and the other way round.
//! Pure: callers read and write the files.

use std::collections::{BTreeMap, HashMap};

use sha1::{Digest, Sha1};

use crate::chunk;
use crate::model::{crc_group, ChunkInfo, ChunkPart, FileManifest, Guid, Manifest, Meta};
use crate::Error;

pub const CHUNK_SIZE: usize = 1024 * 1024;

/// One save file: path relative to the save folder (`/` separators) and its bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaveFile {
    pub path: String,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct Packaged {
    /// `manifests/<YYYY.MM.DD-HH.MM.SS>.manifest`, relative to the game's storage folder.
    pub manifest_path: String,
    pub manifest: Vec<u8>,
    /// (path relative to the game's storage folder, encoded chunk)
    pub chunks: Vec<(String, Vec<u8>)>,
}

/// Build a save manifest and its chunks. `build_version` is the UTC time
/// `YYYY.MM.DD-HH.MM.SS`; `new_guid` supplies random chunk GUIDs.
pub fn package(
    files: &[SaveFile],
    app_name: &str,
    account_id: &str,
    cloud_folder: &str,
    cloud_folder_mac: Option<&str>,
    build_version: &str,
    mut new_guid: impl FnMut() -> Guid,
) -> Result<Packaged, Error> {
    let mut sorted: Vec<&SaveFile> = files.iter().collect();
    sorted.sort_by_key(|f| f.path.to_lowercase());

    let mut chunks: Vec<(Guid, Vec<u8>)> = Vec::new();
    let mut cur: Option<(Guid, Vec<u8>)> = None;
    let mut fms = Vec::new();
    for f in sorted {
        let mut parts = Vec::new();
        let mut pos = 0usize;
        while pos < f.data.len() {
            let (guid, buf) =
                cur.get_or_insert_with(|| (new_guid(), Vec::with_capacity(CHUNK_SIZE)));
            let take = (CHUNK_SIZE - buf.len()).min(f.data.len() - pos);
            parts.push(ChunkPart {
                guid: *guid,
                offset: buf.len() as u32,
                size: take as u32,
                file_offset: pos as u64,
            });
            buf.extend_from_slice(&f.data[pos..pos + take]);
            pos += take;
            if buf.len() == CHUNK_SIZE {
                if let Some(full) = cur.take() {
                    chunks.push(full);
                }
            }
        }
        fms.push(FileManifest {
            filename: f.path.clone(),
            symlink_target: String::new(),
            sha1: Sha1::digest(&f.data).into(),
            flags: 0,
            install_tags: Vec::new(),
            parts,
            md5: None,
            mime_type: None,
            sha256: None,
        });
    }
    if let Some(last) = cur.take() {
        chunks.push(last);
    }

    let mut infos = Vec::new();
    let mut encoded = Vec::new();
    for (guid, mut data) in chunks {
        data.resize(CHUNK_SIZE, 0); // zero-padded to 1 MiB before hashing (reference behaviour)
        let bytes = chunk::encode(guid, &data)?;
        infos.push(ChunkInfo {
            guid,
            hash: chunk::rolling_hash(&data),
            sha1: Sha1::digest(&data).into(),
            group: crc_group(&guid),
            window_size: CHUNK_SIZE as u32,
            file_size: bytes.len() as i64,
            secret_guid: None,
            window_size_compressed: None,
            encryption_tag: None,
        });
        encoded.push(bytes);
    }

    let mut custom_fields = BTreeMap::new();
    custom_fields.insert("CloudSaveFolder".to_owned(), cloud_folder.to_owned());
    if let Some(mac) = cloud_folder_mac.filter(|m| !m.is_empty()) {
        custom_fields.insert("CloudSaveFolder_MAC".to_owned(), mac.to_owned());
    }
    let m = Manifest {
        version: 18,
        json: false,
        meta: Meta {
            data_version: 0,
            feature_level: 18,
            app_name: format!("{app_name}{account_id}"),
            build_version: build_version.to_owned(),
            ..Meta::default()
        },
        chunks: infos,
        files: fms,
        custom_fields,
        secret_guid: None,
        encryption_tag: None,
        encrypted: None,
    };
    let chunk_paths: Vec<String> = m.chunks.iter().map(|c| m.chunk_path(c)).collect();
    Ok(Packaged {
        manifest_path: format!("manifests/{build_version}.manifest"),
        manifest: m.to_bytes()?,
        chunks: chunk_paths.into_iter().zip(encoded).collect(),
    })
}

/// Rebuild the save files from a parsed save manifest and its decoded chunk data
/// (GUID → uncompressed bytes). Fails if a chunk is missing or a file's SHA-1 is wrong.
pub fn unpack(m: &Manifest, chunks: &HashMap<Guid, Vec<u8>>) -> Result<Vec<SaveFile>, Error> {
    let mut out = Vec::with_capacity(m.files.len());
    for f in &m.files {
        let mut data = Vec::with_capacity(f.size() as usize);
        for p in &f.parts {
            let c = chunks
                .get(&p.guid)
                .ok_or(Error::Corrupt("missing save chunk"))?;
            let (start, end) = (p.offset as usize, p.offset as usize + p.size as usize);
            data.extend_from_slice(
                c.get(start..end)
                    .ok_or(Error::Corrupt("chunk part out of range"))?,
            );
        }
        if f.sha1 != [0; 20] && <[u8; 20]>::from(Sha1::digest(&data)) != f.sha1 {
            return Err(Error::HashMismatch);
        }
        out.push(SaveFile {
            path: f.filename.clone(),
            data,
        });
    }
    Ok(out)
}

/// `CloudIncludeList` / `CloudExcludeList` matching (Legendary-compatible): a pattern ending
/// in `/` is a folder prefix, otherwise a suffix match, otherwise a `*` glob.
pub fn matches(path: &str, patterns: &[String]) -> bool {
    patterns
        .iter()
        .map(|p| p.trim())
        .filter(|p| !p.is_empty())
        .any(|p| (p.ends_with('/') && path.starts_with(p)) || path.ends_with(p) || glob(p, path))
}

/// Keep a file given the include and exclude lists (comma-separated catalog values).
pub fn keep(path: &str, include: Option<&str>, exclude: Option<&str>) -> bool {
    let list = |s: Option<&str>| -> Vec<String> {
        s.map(|v| v.split(',').map(str::to_owned).collect())
            .unwrap_or_default()
    };
    let (inc, exc) = (list(include), list(exclude));
    if !inc.is_empty() && !matches(path, &inc) {
        return false;
    }
    exc.is_empty() || !matches(path, &exc)
}

/// `*` matches any run of characters (including `/`, like Python's fnmatch); `?` one character.
fn glob(pattern: &str, text: &str) -> bool {
    let (p, t): (Vec<char>, Vec<char>) = (pattern.chars().collect(), text.chars().collect());
    let (mut pi, mut ti, mut star, mut mark) = (0usize, 0usize, None::<usize>, 0usize);
    while ti < t.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == t[ti]) {
            pi += 1;
            ti += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star = Some(pi);
            mark = ti;
            pi += 1;
        } else if let Some(s) = star {
            pi = s + 1;
            mark += 1;
            ti = mark;
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == '*' {
        pi += 1;
    }
    pi == p.len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Manifest;

    fn guids() -> impl FnMut() -> Guid {
        let mut n = 0u32;
        move || {
            n += 1;
            [n, 0xA, 0xB, 0xC]
        }
    }

    #[test]
    fn package_round_trips_and_matches_the_reference_layout() {
        let big: Vec<u8> = (0..(CHUNK_SIZE + 4096)).map(|i| (i % 251) as u8).collect();
        let files = vec![
            SaveFile {
                path: "b/Slot1.sav".into(),
                data: big.clone(),
            },
            SaveFile {
                path: "A.ini".into(),
                data: b"[x]\nk=v\n".to_vec(),
            },
            SaveFile {
                path: "empty.dat".into(),
                data: vec![],
            },
        ];
        let p = package(
            &files,
            "app",
            "acc",
            "{InstallDir}/Saves/",
            Some("{InstallDir}/wtl.app/Saves/"),
            "2026.10.29-14.05.33",
            guids(),
        )
        .unwrap();
        assert_eq!(p.manifest_path, "manifests/2026.10.29-14.05.33.manifest");
        assert_eq!(p.chunks.len(), 2, "1 MiB + a few KiB → two chunks");
        assert!(p
            .chunks
            .iter()
            .all(|(path, _)| path.starts_with("ChunksV4/")));

        let m = Manifest::parse(&p.manifest).unwrap();
        assert_eq!(
            (
                m.meta.feature_level,
                m.meta.app_name.as_str(),
                m.meta.build_version.as_str()
            ),
            (18, "appacc", "2026.10.29-14.05.33")
        );
        assert_eq!(
            m.custom_fields.get("CloudSaveFolder").map(String::as_str),
            Some("{InstallDir}/Saves/")
        );
        let names: Vec<&str> = m.files.iter().map(|f| f.filename.as_str()).collect();
        assert_eq!(
            names,
            ["A.ini", "b/Slot1.sav", "empty.dat"],
            "case-insensitive order"
        );
        assert!(m.chunks.iter().all(|c| c.window_size == CHUNK_SIZE as u32));

        let mut data = HashMap::new();
        for (_, bytes) in &p.chunks {
            let c = chunk::decode(bytes, &HashMap::new()).unwrap();
            assert_eq!(c.data.len(), CHUNK_SIZE, "padded");
            data.insert(c.guid, c.data);
        }
        let back = unpack(&m, &data).unwrap();
        let get = |n: &str| back.iter().find(|f| f.path == n).map(|f| f.data.clone());
        assert_eq!(get("b/Slot1.sav"), Some(big));
        assert_eq!(get("A.ini").as_deref(), Some(&b"[x]\nk=v\n"[..]));
        assert_eq!(get("empty.dat"), Some(vec![]));
    }

    #[test]
    fn unpack_refuses_missing_chunks_and_bad_hashes() {
        let files = vec![SaveFile {
            path: "s".into(),
            data: vec![1, 2, 3],
        }];
        let p = package(
            &files,
            "a",
            "b",
            "{x}",
            None,
            "2026.01.01-00.00.00",
            guids(),
        )
        .unwrap();
        let m = Manifest::parse(&p.manifest).unwrap();
        assert!(unpack(&m, &HashMap::new()).is_err());
        let mut wrong = HashMap::new();
        wrong.insert(m.chunks[0].guid, vec![9u8; CHUNK_SIZE]);
        assert_eq!(unpack(&m, &wrong).unwrap_err(), Error::HashMismatch);
    }

    #[test]
    fn filters_follow_the_reference_rules() {
        assert!(matches("Saved/SaveGames/1.sav", &["Saved/".into()]));
        assert!(matches("Saved/SaveGames/1.sav", &[".sav".into()]));
        assert!(matches("Saved/SaveGames/1.sav", &["Saved/*.sav".into()]));
        assert!(!matches("Saved/Logs/x.log", &["Saved/SaveGames/".into()]));
        assert!(keep("a.sav", Some("*.sav, *.ini"), None));
        assert!(!keep("a.log", Some("*.sav"), None));
        assert!(!keep("Logs/a.log", None, Some("Logs/")));
        assert!(keep("anything", None, None));
    }
}
