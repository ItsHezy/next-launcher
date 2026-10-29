//! Epic manifest formats: binary and JSON manifests, the `.chunk` container,
//! chunk paths, install tags. Spec: `docs/protocol/manifests.md`.
//!
//! Pure parsing over byte slices: no I/O, no network, no Tauri.

mod binary;
pub mod chunk;
mod crypto;
mod json;
mod model;
pub mod savegame;
pub mod writer;

use std::collections::HashMap;

pub use model::{
    guid_hex, guid_le_bytes, ChunkInfo, ChunkPart, EncryptedPayload, FileManifest, Guid, Manifest,
    Meta,
};

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum Error {
    #[error("not an Epic manifest")]
    NotAManifest,
    #[error("data ended early")]
    Truncated,
    #[error("corrupt data ({0})")]
    Corrupt(&'static str),
    #[error("hash mismatch")]
    HashMismatch,
    #[error("Epic didn't provide the key for this build")]
    MissingSecret,
    #[error("decryption failed")]
    Decrypt,
    #[error("unsupported: {0}")]
    Unsupported(&'static str),
}

impl Manifest {
    /// Parse a binary or JSON manifest. Encrypted fields stay hidden until [`Manifest::decrypt`].
    pub fn parse(bytes: &[u8]) -> Result<Self, Error> {
        match bytes.first() {
            Some(b'{') => json::parse(bytes),
            Some(_) => binary::parse(bytes),
            None => Err(Error::NotAManifest),
        }
    }

    /// Fill in encrypted fields with a key from Epic's `secrets` (no-op if not encrypted).
    pub fn decrypt(&mut self, secrets: &HashMap<String, String>) -> Result<(), Error> {
        crypto::decrypt_manifest(self, secrets)
    }

    pub fn is_encrypted(&self) -> bool {
        self.encrypted.is_some()
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>, Error> {
        writer::write(self)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, HashSet};

    use super::*;

    pub(crate) fn sample() -> Manifest {
        let g1: Guid = [0x1111_1111, 0x2222_2222, 0x3333_3333, 0x4444_4444];
        let g2: Guid = [0xAAAA_AAAA, 0xBBBB_BBBB, 0xCCCC_CCCC, 0xDDDD_DDDD];
        Manifest {
            version: 18,
            json: false,
            meta: Meta {
                data_version: 2,
                feature_level: 18,
                app_name: "SampleApp".into(),
                build_version: "1.0.0-CL-1-Windows".into(),
                launch_exe: "Bin/Game.exe".into(),
                launch_command: "-sample".into(),
                prereq_ids: vec!["vcredist".into()],
                prereq_name: "VC++".into(),
                prereq_path: "Redist/vc_redist.x64.exe".into(),
                prereq_args: "/quiet".into(),
                build_id: Some("buildid".into()),
                uninstall_action_path: "Bin/Uninstall.exe".into(),
                ..Meta::default()
            },
            chunks: vec![
                ChunkInfo {
                    guid: g1,
                    hash: 0x0123_4567_89AB_CDEF,
                    sha1: [1; 20],
                    group: 42,
                    window_size: 1024 * 1024,
                    file_size: 4096,
                    secret_guid: None,
                    window_size_compressed: None,
                    encryption_tag: None,
                },
                ChunkInfo {
                    guid: g2,
                    hash: 7,
                    sha1: [2; 20],
                    group: 3,
                    window_size: 1024 * 1024,
                    file_size: 100,
                    secret_guid: None,
                    window_size_compressed: None,
                    encryption_tag: None,
                },
            ],
            files: vec![
                FileManifest {
                    filename: "Bin/Game.exe".into(),
                    symlink_target: String::new(),
                    sha1: [9; 20],
                    flags: 4,
                    install_tags: vec![],
                    parts: vec![
                        ChunkPart {
                            guid: g1,
                            offset: 0,
                            size: 1000,
                            file_offset: 0,
                        },
                        ChunkPart {
                            guid: g2,
                            offset: 10,
                            size: 24,
                            file_offset: 1000,
                        },
                    ],
                    md5: Some([5; 16]),
                    mime_type: Some("application/octet-stream".into()),
                    sha256: Some([6; 32]),
                },
                FileManifest {
                    filename: "Content/Ünïcödé Voice.pak".into(),
                    symlink_target: String::new(),
                    sha1: [8; 20],
                    flags: 0,
                    install_tags: vec!["voice_de".into()],
                    parts: vec![ChunkPart {
                        guid: g1,
                        offset: 1000,
                        size: 500,
                        file_offset: 0,
                    }],
                    md5: None,
                    mime_type: None,
                    sha256: Some([7; 32]),
                },
            ],
            custom_fields: BTreeMap::from([(
                "CloudSaveFolder".to_owned(),
                "{AppData}/x".to_owned(),
            )]),
            secret_guid: None,
            encryption_tag: None,
            encrypted: None,
        }
    }

    #[test]
    fn binary_roundtrip_including_utf16_names() {
        let m = sample();
        let bytes = m.to_bytes().unwrap();
        assert_eq!(&bytes[..4], &[0x0C, 0xC0, 0xBE, 0x44]);
        let back = Manifest::parse(&bytes).unwrap();
        assert_eq!(back, m);
        assert_eq!(back.files[0].size(), 1024);
    }

    #[test]
    fn sizes_and_install_tags() {
        let m = sample();
        let none: HashSet<String> = HashSet::new();
        let de: HashSet<String> = ["voice_de".to_owned()].into();
        assert_eq!(m.install_size(&none), 1024);
        assert_eq!(m.install_size(&de), 1524);
        assert_eq!(m.download_size(&none), 4196); // both chunks needed by Game.exe
        assert_eq!(m.install_tags(), vec!["voice_de".to_owned()]);
    }

    #[test]
    fn chunk_paths_v4_and_v5() {
        let mut m = sample();
        let c = m.chunks[0].clone();
        assert_eq!(
            m.chunk_path(&c),
            "ChunksV4/42/0123456789ABCDEF_11111111222222223333333344444444.chunk"
        );
        m.meta.feature_level = 22;
        let p = m.chunk_path(&c);
        assert!(p.starts_with("ChunksV5/plain/42/"), "{p}");
        let mut c2 = c.clone();
        c2.secret_guid = Some([1, 2, 3, 4]);
        assert!(m
            .chunk_path(&c2)
            .starts_with("ChunksV5/AQAAAAIAAAADAAAABAAAAA/42/"));
    }

    #[test]
    fn corrupt_and_truncated_input_errors_cleanly() {
        let bytes = sample().to_bytes().unwrap();
        assert_eq!(Manifest::parse(&bytes[..30]).unwrap_err(), Error::Truncated);
        let mut bad = bytes.clone();
        let n = bad.len();
        bad[n - 3] ^= 0x55;
        assert!(Manifest::parse(&bad).is_err());
        assert_eq!(Manifest::parse(b"").unwrap_err(), Error::NotAManifest);
        assert_eq!(Manifest::parse(b"nope").unwrap_err(), Error::NotAManifest);
    }

    #[test]
    fn json_manifest_blobs_and_guids() {
        let json = br#"{
          "ManifestFileVersion": "013000000000",
          "bIsFileData": false,
          "AppID": "000000000000",
          "AppNameString": "OldGame",
          "BuildVersionString": "1.0",
          "LaunchExeString": "Game.exe",
          "LaunchCommand": "",
          "PrereqIds": [], "PrereqName": "", "PrereqPath": "", "PrereqArgs": "",
          "FileManifestList": [{
            "Filename": "Game.exe",
            "FileHash": "001002003004005006007008009010011012013014015016017018019020",
            "bIsUnixExecutable": true,
            "FileChunkParts": [{ "Guid": "0000000100000002000000030000000A", "Offset": "000000000000", "Size": "000004000000" }]
          }],
          "ChunkHashList": { "0000000100000002000000030000000A": "001000000000000000000000" },
          "ChunkShaList": { "0000000100000002000000030000000A": "0102030405060708090a0b0c0d0e0f1011121314" },
          "DataGroupList": { "0000000100000002000000030000000A": "007" },
          "ChunkFilesizeList": { "0000000100000002000000030000000A": "232003000000000000000000" },
          "CustomFields": { "Key": "Value" }
        }"#;
        let m = Manifest::parse(json).unwrap();
        assert!(m.json);
        assert_eq!(m.meta.feature_level, 13);
        assert_eq!(m.chunks[0].guid, [1, 2, 3, 10]);
        assert_eq!(m.chunks[0].group, 7);
        assert_eq!(m.chunks[0].file_size, 1000);
        assert_eq!(m.chunks[0].hash, 1);
        assert_eq!(m.files[0].sha1[0], 1);
        assert_eq!(m.files[0].sha1[19], 20);
        assert_eq!(m.files[0].size(), 1024);
        assert!(m.files[0].is_executable());
        assert_eq!(m.chunk_dir(), "ChunksV3");
        assert_eq!(
            m.custom_fields.get("Key").map(String::as_str),
            Some("Value")
        );
    }

    /// Reads a real manifest from disk, read-only. Run with:
    /// `NL_REAL_MANIFEST=<path> cargo test -p egs-manifest real_manifest -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn real_manifest() {
        let Some(path) = std::env::var_os("NL_REAL_MANIFEST") else {
            return;
        };
        let bytes = std::fs::read(path).unwrap();
        let t = std::time::Instant::now();
        let m = Manifest::parse(&bytes).unwrap();
        let none = HashSet::new();
        println!(
            "parsed {} bytes in {:?}: feature level {}, {} chunks, {} files, install {} MB, tags {:?}, encrypted {}",
            bytes.len(),
            t.elapsed(),
            m.meta.feature_level,
            m.chunks.len(),
            m.files.len(),
            m.install_size(&none) / 1_000_000,
            m.install_tags().len(),
            m.is_encrypted()
        );
        println!(
            "launch {:?} {:?}; prereq {:?} {:?} {:?}; uninstall action {:?}",
            m.meta.launch_exe,
            m.meta.launch_command,
            m.meta.prereq_name,
            m.meta.prereq_path,
            m.meta.prereq_args,
            m.meta.uninstall_action_path
        );
        assert!(!m.files.is_empty());
        let index = m.chunk_index();
        assert!(m
            .files
            .iter()
            .all(|f| f.parts.iter().all(|p| index.contains_key(&p.guid))));
    }
}
