//! Binary manifest reader (docs/protocol/manifests.md "Binary manifest").

use std::collections::BTreeMap;
use std::io::Read;

use sha1::{Digest, Sha1};

use crate::model::{ChunkInfo, ChunkPart, EncryptedPayload, FileManifest, Guid, Manifest, Meta};
use crate::Error;

pub const MAGIC: u32 = 0x44BE_C00C;
/// Refuse absurd sizes from corrupt input before allocating.
const MAX_BODY: usize = 1 << 30;
const MAX_COUNT: u32 = 10_000_000;

pub(crate) struct Reader<'a> {
    b: &'a [u8],
    pub pos: usize,
}

impl<'a> Reader<'a> {
    pub fn new(b: &'a [u8]) -> Self {
        Self { b, pos: 0 }
    }
    pub fn take(&mut self, n: usize) -> Result<&'a [u8], Error> {
        let end = self.pos.checked_add(n).ok_or(Error::Truncated)?;
        let s = self.b.get(self.pos..end).ok_or(Error::Truncated)?;
        self.pos = end;
        Ok(s)
    }
    pub fn u8(&mut self) -> Result<u8, Error> {
        Ok(self.take(1)?[0])
    }
    pub fn u32(&mut self) -> Result<u32, Error> {
        let mut a = [0u8; 4];
        a.copy_from_slice(self.take(4)?);
        Ok(u32::from_le_bytes(a))
    }
    pub fn i32(&mut self) -> Result<i32, Error> {
        Ok(self.u32()? as i32)
    }
    pub fn u64(&mut self) -> Result<u64, Error> {
        let mut a = [0u8; 8];
        a.copy_from_slice(self.take(8)?);
        Ok(u64::from_le_bytes(a))
    }
    pub fn array<const N: usize>(&mut self) -> Result<[u8; N], Error> {
        let mut a = [0u8; N];
        a.copy_from_slice(self.take(N)?);
        Ok(a)
    }
    pub fn guid(&mut self) -> Result<Guid, Error> {
        Ok([self.u32()?, self.u32()?, self.u32()?, self.u32()?])
    }
    pub fn count(&mut self) -> Result<u32, Error> {
        let n = self.u32()?;
        if n > MAX_COUNT {
            return Err(Error::Corrupt("element count"));
        }
        Ok(n)
    }
    /// Unreal `FString`: i32 length; positive = ASCII/UTF-8 with NUL, negative = UTF-16LE with NUL.
    pub fn fstring(&mut self) -> Result<String, Error> {
        let len = self.i32()?;
        match len {
            0 => Ok(String::new()),
            l if l > 0 => {
                let bytes = self.take(l as usize)?;
                let body = &bytes[..bytes.len() - 1];
                Ok(String::from_utf8_lossy(body).into_owned())
            }
            l => {
                let chars = l.unsigned_abs() as usize;
                let bytes = self.take(chars.checked_mul(2).ok_or(Error::Corrupt("fstring"))?)?;
                let units: Vec<u16> = bytes
                    .chunks_exact(2)
                    .map(|c| u16::from_le_bytes([c[0], c[1]]))
                    .take(chars - 1)
                    .collect();
                Ok(String::from_utf16_lossy(&units))
            }
        }
    }
    /// Move to `start + size` (sections may hold fields we don't know).
    pub fn seek_section_end(&mut self, start: usize, size: u32) -> Result<(), Error> {
        let end = start.checked_add(size as usize).ok_or(Error::Truncated)?;
        if end > self.b.len() || end < self.pos {
            return Err(Error::Corrupt("section size"));
        }
        self.pos = end;
        Ok(())
    }
    pub fn rest(&self) -> &'a [u8] {
        &self.b[self.pos..]
    }
}

pub(crate) fn inflate(data: &[u8], expected: Option<usize>) -> Result<Vec<u8>, Error> {
    let mut out = Vec::with_capacity(expected.unwrap_or(data.len() * 3).min(MAX_BODY));
    flate2::read::ZlibDecoder::new(data)
        .take(MAX_BODY as u64 + 1)
        .read_to_end(&mut out)
        .map_err(|_| Error::Corrupt("zlib data"))?;
    if out.len() > MAX_BODY {
        return Err(Error::Corrupt("body too large"));
    }
    Ok(out)
}

fn read_meta(r: &mut Reader<'_>) -> Result<Meta, Error> {
    let start = r.pos;
    let size = r.u32()?;
    let data_version = r.u8()?;
    let mut m = Meta {
        data_version,
        feature_level: r.u32()?,
        is_file_data: r.u8()? == 1,
        app_id: r.u32()?,
        app_name: r.fstring()?,
        build_version: r.fstring()?,
        launch_exe: r.fstring()?,
        launch_command: r.fstring()?,
        ..Meta::default()
    };
    let n = r.count()?;
    for _ in 0..n {
        m.prereq_ids.push(r.fstring()?);
    }
    m.prereq_name = r.fstring()?;
    m.prereq_path = r.fstring()?;
    m.prereq_args = r.fstring()?;
    if data_version >= 1 {
        m.build_id = Some(r.fstring()?).filter(|s| !s.is_empty());
    }
    if data_version >= 2 {
        m.uninstall_action_path = r.fstring()?;
        m.uninstall_action_args = r.fstring()?;
    }
    r.seek_section_end(start, size)?;
    Ok(m)
}

fn read_chunks(r: &mut Reader<'_>, feature_level: u32) -> Result<Vec<ChunkInfo>, Error> {
    let start = r.pos;
    let size = r.u32()?;
    let _version = r.u8()?;
    let n = r.count()? as usize;
    let mut guids = Vec::with_capacity(n);
    for _ in 0..n {
        guids.push(r.guid()?);
    }
    let mut hashes = Vec::with_capacity(n);
    for _ in 0..n {
        hashes.push(r.u64()?);
    }
    let mut shas = Vec::with_capacity(n);
    for _ in 0..n {
        shas.push(r.array::<20>()?);
    }
    let mut groups = Vec::with_capacity(n);
    for _ in 0..n {
        groups.push(r.u8()?);
    }
    let mut windows = Vec::with_capacity(n);
    for _ in 0..n {
        windows.push(r.u32()?);
    }
    let mut sizes = Vec::with_capacity(n);
    for _ in 0..n {
        sizes.push(r.u64()? as i64);
    }
    let (mut secrets, mut wcomp, mut tags) = (Vec::new(), Vec::new(), Vec::new());
    if feature_level >= 22 {
        for _ in 0..n {
            secrets.push(r.guid()?);
        }
        for _ in 0..n {
            wcomp.push(r.u32()?);
        }
        for _ in 0..n {
            tags.push(r.array::<16>()?);
        }
    }
    r.seek_section_end(start, size)?;
    Ok((0..n)
        .map(|i| ChunkInfo {
            guid: guids[i],
            hash: hashes[i],
            sha1: shas[i],
            group: groups[i],
            window_size: windows[i],
            file_size: sizes[i],
            secret_guid: secrets.get(i).copied(),
            window_size_compressed: wcomp.get(i).copied(),
            encryption_tag: tags.get(i).copied(),
        })
        .collect())
}

fn read_files(r: &mut Reader<'_>) -> Result<Vec<FileManifest>, Error> {
    let start = r.pos;
    let size = r.u32()?;
    let version = r.u8()?;
    let n = r.count()? as usize;
    let mut files: Vec<FileManifest> = Vec::with_capacity(n);
    for _ in 0..n {
        files.push(FileManifest {
            filename: r.fstring()?,
            symlink_target: String::new(),
            sha1: [0; 20],
            flags: 0,
            install_tags: Vec::new(),
            parts: Vec::new(),
            md5: None,
            mime_type: None,
            sha256: None,
        });
    }
    for f in files.iter_mut() {
        f.symlink_target = r.fstring()?;
    }
    for f in files.iter_mut() {
        f.sha1 = r.array::<20>()?;
    }
    for f in files.iter_mut() {
        f.flags = r.u8()?;
    }
    for f in files.iter_mut() {
        let k = r.count()?;
        for _ in 0..k {
            f.install_tags.push(r.fstring()?);
        }
    }
    for f in files.iter_mut() {
        let k = r.count()?;
        let mut offset = 0u64;
        for _ in 0..k {
            let part_start = r.pos;
            let part_size = r.u32()?;
            let p = ChunkPart {
                guid: r.guid()?,
                offset: r.u32()?,
                size: r.u32()?,
                file_offset: offset,
            };
            offset += u64::from(p.size);
            f.parts.push(p);
            if part_size >= 28 {
                r.seek_section_end(part_start, part_size)?;
            }
        }
    }
    if version >= 1 {
        for f in files.iter_mut() {
            if r.u32()? != 0 {
                f.md5 = Some(r.array::<16>()?);
            }
        }
        for f in files.iter_mut() {
            f.mime_type = Some(r.fstring()?).filter(|s| !s.is_empty());
        }
    }
    if version >= 2 {
        for f in files.iter_mut() {
            f.sha256 = Some(r.array::<32>()?);
        }
    }
    r.seek_section_end(start, size)?;
    Ok(files)
}

fn read_custom_fields(r: &mut Reader<'_>) -> Result<BTreeMap<String, String>, Error> {
    let start = r.pos;
    let size = r.u32()?;
    let _version = r.u8()?;
    let n = r.count()? as usize;
    let mut keys = Vec::with_capacity(n);
    for _ in 0..n {
        keys.push(r.fstring()?);
    }
    let mut map = BTreeMap::new();
    for k in keys {
        map.insert(k, r.fstring()?);
    }
    r.seek_section_end(start, size)?;
    Ok(map)
}

fn read_encrypted(r: &mut Reader<'_>) -> Result<EncryptedPayload, Error> {
    let start = r.pos;
    let size = r.u32()?;
    let _version = r.u8()?;
    let _blob_size = r.u32()?;
    let h_start = r.pos;
    let h_size = r.u32()?;
    let _h_version = r.u32()?;
    let h_stored_as = r.u8()?;
    let _uncompressed = r.u32()?;
    let _compressed = r.u32()?;
    let iv_len = r.u32()? as usize;
    let iv = r.take(iv_len)?.to_vec();
    r.seek_section_end(h_start, h_size)?;
    let ct_len = r.u32()? as usize;
    let ciphertext = r.take(ct_len)?.to_vec();
    r.seek_section_end(start, size)?;
    Ok(EncryptedPayload {
        compressed: h_stored_as & 1 != 0,
        iv,
        ciphertext,
    })
}

pub(crate) fn parse(bytes: &[u8]) -> Result<Manifest, Error> {
    let mut h = Reader::new(bytes);
    if h.u32()? != MAGIC {
        return Err(Error::NotAManifest);
    }
    let header_size = h.u32()?;
    let size_uncompressed = h.u32()? as usize;
    let _size_compressed = h.u32()?;
    let sha = h.array::<20>()?;
    let stored_as = h.u8()?;
    let version = h.u32()?;
    let (mut secret_guid, mut encryption_tag) = (None, None);
    if version >= 22 {
        secret_guid = Some(h.guid()?);
        encryption_tag = Some(h.array::<16>()?);
    }
    h.seek_section_end(0, header_size)?;

    let raw = h.rest();
    let body = if stored_as & 1 != 0 {
        if size_uncompressed > MAX_BODY {
            return Err(Error::Corrupt("body too large"));
        }
        inflate(raw, Some(size_uncompressed))?
    } else {
        raw.to_vec()
    };
    if Sha1::digest(&body).as_slice() != sha {
        return Err(Error::HashMismatch);
    }

    let mut r = Reader::new(&body);
    let meta = read_meta(&mut r)?;
    let chunks = read_chunks(&mut r, meta.feature_level)?;
    let files = read_files(&mut r)?;
    let custom_fields = read_custom_fields(&mut r)?;
    let encrypted = if stored_as & 2 != 0 && meta.feature_level >= 24 {
        Some(read_encrypted(&mut r)?)
    } else {
        None
    };
    if stored_as & 2 != 0 && encrypted.is_none() {
        return Err(Error::Unsupported(
            "encrypted manifest below feature level 24",
        ));
    }
    Ok(Manifest {
        version,
        json: false,
        meta,
        chunks,
        files,
        custom_fields,
        secret_guid,
        encryption_tag,
        encrypted,
    })
}
