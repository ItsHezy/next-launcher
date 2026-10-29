//! Manifest data model (docs/protocol/manifests.md).

use std::collections::{BTreeMap, HashMap, HashSet};

/// 128-bit chunk id as four little-endian u32 words, as stored in binary manifests.
pub type Guid = [u32; 4];

/// `%08X` ×4: the key format of `secrets` maps and old-style chunk paths.
pub fn guid_hex(g: &Guid) -> String {
    format!("{:08X}{:08X}{:08X}{:08X}", g[0], g[1], g[2], g[3])
}

pub fn guid_le_bytes(g: &Guid) -> [u8; 16] {
    let mut out = [0u8; 16];
    for (i, w) in g.iter().enumerate() {
        out[i * 4..i * 4 + 4].copy_from_slice(&w.to_le_bytes());
    }
    out
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Meta {
    pub data_version: u8,
    pub feature_level: u32,
    pub is_file_data: bool,
    pub app_id: u32,
    pub app_name: String,
    pub build_version: String,
    pub launch_exe: String,
    pub launch_command: String,
    pub prereq_ids: Vec<String>,
    pub prereq_name: String,
    pub prereq_path: String,
    pub prereq_args: String,
    pub build_id: Option<String>,
    pub uninstall_action_path: String,
    pub uninstall_action_args: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChunkInfo {
    pub guid: Guid,
    /// 64-bit rolling hash (part of old-style paths).
    pub hash: u64,
    /// SHA-1 of the uncompressed chunk data.
    pub sha1: [u8; 20],
    /// 0..99, part of the CDN path.
    pub group: u8,
    /// Uncompressed size.
    pub window_size: u32,
    /// Bytes to download.
    pub file_size: i64,
    /// Feature level ≥ 22.
    pub secret_guid: Option<Guid>,
    pub window_size_compressed: Option<u32>,
    pub encryption_tag: Option<[u8; 16]>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChunkPart {
    pub guid: Guid,
    /// Offset inside the chunk's uncompressed data.
    pub offset: u32,
    pub size: u32,
    /// Offset inside the file (running sum of previous parts).
    pub file_offset: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileManifest {
    /// Relative path with `/` separators.
    pub filename: String,
    pub symlink_target: String,
    /// SHA-1 of the whole file.
    pub sha1: [u8; 20],
    /// bit0 read-only, bit1 compressed, bit2 executable.
    pub flags: u8,
    pub install_tags: Vec<String>,
    pub parts: Vec<ChunkPart>,
    pub md5: Option<[u8; 16]>,
    pub mime_type: Option<String>,
    pub sha256: Option<[u8; 32]>,
}

impl FileManifest {
    pub fn size(&self) -> u64 {
        self.parts.iter().map(|p| u64::from(p.size)).sum()
    }

    pub fn is_executable(&self) -> bool {
        self.flags & 0x4 != 0
    }

    /// Installed with the given tag selection? Untagged files always are.
    pub fn selected(&self, tags: &HashSet<String>) -> bool {
        self.install_tags.is_empty() || self.install_tags.iter().any(|t| tags.contains(t))
    }
}

/// Encrypted string payload (feature level ≥ 24), decrypted with an Epic-supplied secret.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncryptedPayload {
    pub compressed: bool,
    pub iv: Vec<u8>,
    pub ciphertext: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Manifest {
    /// Header serialisation version (binary) or `ManifestFileVersion` (JSON).
    pub version: u32,
    /// The source was JSON.
    pub json: bool,
    pub meta: Meta,
    pub chunks: Vec<ChunkInfo>,
    pub files: Vec<FileManifest>,
    pub custom_fields: BTreeMap<String, String>,
    pub secret_guid: Option<Guid>,
    pub encryption_tag: Option<[u8; 16]>,
    /// Present until [`Manifest::decrypt`] succeeds.
    pub encrypted: Option<EncryptedPayload>,
}

impl Manifest {
    /// Chunk directory name by feature level.
    pub fn chunk_dir(&self) -> &'static str {
        match self.meta.feature_level {
            l if l >= 22 => "ChunksV5",
            l if l >= 15 => "ChunksV4",
            l if l >= 6 => "ChunksV3",
            l if l >= 3 => "ChunksV2",
            _ => "Chunks",
        }
    }

    /// Path of a chunk relative to a CDN base URL (`…/CloudDir`).
    pub fn chunk_path(&self, c: &ChunkInfo) -> String {
        use base64::engine::general_purpose::URL_SAFE_NO_PAD;
        use base64::Engine;
        let dir = self.chunk_dir();
        if self.meta.feature_level >= 22 {
            let secret = match c.secret_guid {
                Some(s) if s != [0; 4] => URL_SAFE_NO_PAD.encode(guid_le_bytes(&s)),
                _ => "plain".to_owned(),
            };
            format!(
                "{dir}/{secret}/{:02}/{}_{}.chunk",
                c.group,
                URL_SAFE_NO_PAD.encode(c.hash.to_le_bytes()),
                URL_SAFE_NO_PAD.encode(guid_le_bytes(&c.guid))
            )
        } else {
            format!(
                "{dir}/{:02}/{:016X}_{}.chunk",
                c.group,
                c.hash,
                guid_hex(&c.guid)
            )
        }
    }

    pub fn chunk_index(&self) -> HashMap<Guid, usize> {
        self.chunks
            .iter()
            .enumerate()
            .map(|(i, c)| (c.guid, i))
            .collect()
    }

    /// Sum of file sizes for the selected tags.
    pub fn install_size(&self, tags: &HashSet<String>) -> u64 {
        self.files
            .iter()
            .filter(|f| f.selected(tags))
            .map(FileManifest::size)
            .sum()
    }

    /// Bytes to download for a fresh install with the selected tags.
    pub fn download_size(&self, tags: &HashSet<String>) -> u64 {
        let index = self.chunk_index();
        let mut needed = HashSet::new();
        for f in self.files.iter().filter(|f| f.selected(tags)) {
            for p in &f.parts {
                needed.insert(p.guid);
            }
        }
        needed
            .iter()
            .filter_map(|g| index.get(g))
            .map(|&i| self.chunks[i].file_size.max(0) as u64)
            .sum()
    }

    /// All install tags used by files.
    pub fn install_tags(&self) -> Vec<String> {
        let mut tags: Vec<String> = self
            .files
            .iter()
            .flat_map(|f| f.install_tags.iter().cloned())
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        tags.sort();
        tags
    }

    pub fn file(&self, name: &str) -> Option<&FileManifest> {
        self.files.iter().find(|f| f.filename == name)
    }

    /// The build id: stored (data version ≥ 1) or derived from meta (manifests.md "Updates").
    pub fn build_id(&self) -> String {
        if let Some(id) = &self.meta.build_id {
            return id.clone();
        }
        use base64::engine::general_purpose::URL_SAFE_NO_PAD;
        use base64::Engine;
        use sha1::{Digest, Sha1};
        let mut h = Sha1::new();
        h.update(self.meta.app_id.to_le_bytes());
        h.update(self.meta.app_name.as_bytes());
        h.update(self.meta.build_version.as_bytes());
        h.update(self.meta.launch_exe.as_bytes());
        h.update(self.meta.launch_command.as_bytes());
        URL_SAFE_NO_PAD.encode(h.finalize())
    }
}

/// Chunk group when a manifest doesn't store one: `crc32(guid as 16 LE bytes) % 100`.
pub fn crc_group(g: &Guid) -> u8 {
    let mut crc = flate2::Crc::new();
    crc.update(&guid_le_bytes(g));
    (crc.sum() % 100) as u8
}
