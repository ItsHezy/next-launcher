//! Binary manifest writer (feature level < 22). Used for cloud-save uploads
//! (docs/protocol/cloud-saves.md) and to round-trip-test the reader.

use std::io::Write;

use sha1::{Digest, Sha1};

use crate::binary::MAGIC;
use crate::model::{Guid, Manifest};
use crate::Error;

struct W(Vec<u8>);

impl W {
    fn u8(&mut self, v: u8) {
        self.0.push(v);
    }
    fn u32(&mut self, v: u32) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn u64(&mut self, v: u64) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn bytes(&mut self, b: &[u8]) {
        self.0.extend_from_slice(b);
    }
    fn guid(&mut self, g: &Guid) {
        for w in g {
            self.u32(*w);
        }
    }
    /// ASCII when possible, else UTF-16LE with a negative length.
    fn fstring(&mut self, s: &str) {
        if s.is_empty() {
            self.u32(0);
        } else if s.is_ascii() {
            self.u32(s.len() as u32 + 1);
            self.bytes(s.as_bytes());
            self.u8(0);
        } else {
            let units: Vec<u16> = s.encode_utf16().collect();
            self.u32((-(units.len() as i32 + 1)) as u32);
            for u in units {
                self.bytes(&u.to_le_bytes());
            }
            self.bytes(&[0, 0]);
        }
    }
    /// Run `f`, then patch the leading u32 with the section size.
    fn section(&mut self, f: impl FnOnce(&mut W)) {
        let start = self.0.len();
        self.u32(0);
        f(self);
        let size = (self.0.len() - start) as u32;
        self.0[start..start + 4].copy_from_slice(&size.to_le_bytes());
    }
}

/// Serialise to a zlib-compressed binary manifest.
pub fn write(m: &Manifest) -> Result<Vec<u8>, Error> {
    if m.meta.feature_level >= 22 || m.encrypted.is_some() {
        return Err(Error::Unsupported("writing feature level 22+"));
    }
    let mut b = W(Vec::new());
    b.section(|b| {
        b.u8(m.meta.data_version);
        b.u32(m.meta.feature_level);
        b.u8(u8::from(m.meta.is_file_data));
        b.u32(m.meta.app_id);
        b.fstring(&m.meta.app_name);
        b.fstring(&m.meta.build_version);
        b.fstring(&m.meta.launch_exe);
        b.fstring(&m.meta.launch_command);
        b.u32(m.meta.prereq_ids.len() as u32);
        for p in &m.meta.prereq_ids {
            b.fstring(p);
        }
        b.fstring(&m.meta.prereq_name);
        b.fstring(&m.meta.prereq_path);
        b.fstring(&m.meta.prereq_args);
        if m.meta.data_version >= 1 {
            b.fstring(m.meta.build_id.as_deref().unwrap_or_default());
        }
        if m.meta.data_version >= 2 {
            b.fstring(&m.meta.uninstall_action_path);
            b.fstring(&m.meta.uninstall_action_args);
        }
    });
    b.section(|b| {
        b.u8(0);
        b.u32(m.chunks.len() as u32);
        for c in &m.chunks {
            b.guid(&c.guid);
        }
        for c in &m.chunks {
            b.u64(c.hash);
        }
        for c in &m.chunks {
            b.bytes(&c.sha1);
        }
        for c in &m.chunks {
            b.u8(c.group);
        }
        for c in &m.chunks {
            b.u32(c.window_size);
        }
        for c in &m.chunks {
            b.u64(c.file_size as u64);
        }
    });
    let fml_version: u8 = if m.files.iter().any(|f| f.sha256.is_some()) {
        2
    } else if m
        .files
        .iter()
        .any(|f| f.md5.is_some() || f.mime_type.is_some())
    {
        1
    } else {
        0
    };
    b.section(|b| {
        b.u8(fml_version);
        b.u32(m.files.len() as u32);
        for f in &m.files {
            b.fstring(&f.filename);
        }
        for f in &m.files {
            b.fstring(&f.symlink_target);
        }
        for f in &m.files {
            b.bytes(&f.sha1);
        }
        for f in &m.files {
            b.u8(f.flags);
        }
        for f in &m.files {
            b.u32(f.install_tags.len() as u32);
            for t in &f.install_tags {
                b.fstring(t);
            }
        }
        for f in &m.files {
            b.u32(f.parts.len() as u32);
            for p in &f.parts {
                b.u32(28);
                b.guid(&p.guid);
                b.u32(p.offset);
                b.u32(p.size);
            }
        }
        if fml_version >= 1 {
            for f in &m.files {
                match f.md5 {
                    Some(md5) => {
                        b.u32(1);
                        b.bytes(&md5);
                    }
                    None => b.u32(0),
                }
            }
            for f in &m.files {
                b.fstring(f.mime_type.as_deref().unwrap_or_default());
            }
        }
        if fml_version >= 2 {
            for f in &m.files {
                b.bytes(&f.sha256.unwrap_or([0; 32]));
            }
        }
    });
    b.section(|b| {
        b.u8(0);
        b.u32(m.custom_fields.len() as u32);
        for k in m.custom_fields.keys() {
            b.fstring(k);
        }
        for v in m.custom_fields.values() {
            b.fstring(v);
        }
    });

    let body = b.0;
    let sha: [u8; 20] = Sha1::digest(&body).into();
    let mut z = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    z.write_all(&body).map_err(|_| Error::Corrupt("zlib"))?;
    let compressed = z.finish().map_err(|_| Error::Corrupt("zlib"))?;

    let mut out = W(Vec::with_capacity(41 + compressed.len()));
    out.u32(MAGIC);
    out.u32(41);
    out.u32(body.len() as u32);
    out.u32(compressed.len() as u32);
    out.bytes(&sha);
    out.u8(1);
    out.u32(m.version.max(m.meta.feature_level));
    out.bytes(&compressed);
    Ok(out.0)
}
