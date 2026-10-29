//! Turn (new manifest, old manifest?, install tags, resume state) into an
//! ordered set of file writes and chunk fetches (docs/protocol/downloads-cdn.md §3).

use std::collections::{HashMap, HashSet};

use egs_manifest::{FileManifest, Guid, Manifest};

/// Keep a fetched chunk in memory only while its next use is within this many
/// part-uses; beyond that it is evicted and fetched again when needed.
pub const LOOKAHEAD_PARTS: usize = 256;

/// Where the bytes of one file range come from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    /// Slice of a downloaded chunk: (fetch index, offset in chunk).
    Fetch { fetch: usize, offset: u32 },
    /// Copy from the existing old file at `old_offset` (patching).
    Old { old_offset: u64 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Piece {
    pub source: Source,
    pub size: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileJob {
    /// Index into the new manifest's `files`.
    pub file: usize,
    pub pieces: Vec<Piece>,
    /// Bytes copied from the old file instead of downloaded.
    pub reused: u64,
}

/// One chunk download. The same chunk can appear twice if it was evicted in between.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fetch {
    pub chunk: usize,
    /// Position (in part-use order) after which the data is no longer needed.
    pub last_use: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Plan {
    pub jobs: Vec<FileJob>,
    pub fetches: Vec<Fetch>,
    /// Files in the old build that are not in the new one (deleted after success).
    pub delete: Vec<String>,
    /// Files already correct (unchanged or completed before a restart).
    pub skipped: usize,
    pub install_size: u64,
    /// Compressed bytes to download.
    pub download_size: u64,
    /// Bytes copied from old files.
    pub reuse_size: u64,
    /// Extra disk space needed at the peak (can be negative for shrinking updates).
    pub disk_delta: i64,
    /// Peak bytes of chunk data held in memory by the plan itself.
    pub peak_cache: u64,
}

#[derive(Debug, Clone, Default)]
pub struct PlanOptions {
    pub tags: HashSet<String>,
    /// Files finished before an interruption (skip them).
    pub completed: HashSet<String>,
    /// Re-download every selected file (repair without patching).
    pub no_reuse: bool,
    /// Limit to these files (repair). `None` = all selected files.
    pub only: Option<HashSet<String>>,
}

pub fn plan(new: &Manifest, old: Option<&Manifest>, opts: &PlanOptions) -> Plan {
    let index = new.chunk_index();
    let old_files: HashMap<&str, &FileManifest> = old
        .map(|o| o.files.iter().map(|f| (f.filename.as_str(), f)).collect())
        .unwrap_or_default();
    let selected: Vec<(usize, &FileManifest)> = new
        .files
        .iter()
        .enumerate()
        .filter(|(_, f)| f.selected(&opts.tags))
        .collect();

    let mut p = Plan {
        install_size: selected.iter().map(|(_, f)| f.size()).sum(),
        ..Plan::default()
    };

    // Files to (re)write.
    let mut to_write: Vec<(usize, &FileManifest)> = Vec::new();
    for &(i, f) in &selected {
        if let Some(only) = &opts.only {
            if !only.contains(&f.filename) {
                p.skipped += 1;
                continue;
            }
        } else {
            let unchanged = old_files
                .get(f.filename.as_str())
                .is_some_and(|o| o.sha1 == f.sha1);
            if unchanged || opts.completed.contains(&f.filename) {
                p.skipped += 1;
                continue;
            }
        }
        to_write.push((i, f));
    }

    // Deletions: old files not present (or no longer selected) in the new build.
    if let Some(o) = old {
        let keep: HashSet<&str> = selected.iter().map(|(_, f)| f.filename.as_str()).collect();
        p.delete = o
            .files
            .iter()
            .map(|f| f.filename.as_str())
            .filter(|n| !keep.contains(n))
            .map(str::to_owned)
            .collect();
    }

    // Build pieces; collect the part-use sequence for downloaded data.
    let mut uses: Vec<Guid> = Vec::new();
    let mut piece_use: Vec<Vec<Option<usize>>> = Vec::with_capacity(to_write.len());
    for &(i, f) in &to_write {
        let old_f = if opts.no_reuse {
            None
        } else {
            old_files.get(f.filename.as_str())
        };
        let mut job = FileJob {
            file: i,
            pieces: Vec::with_capacity(f.parts.len()),
            reused: 0,
        };
        let mut job_uses = Vec::with_capacity(f.parts.len());
        for part in &f.parts {
            let reused = old_f.and_then(|of| {
                of.parts.iter().find_map(|op| {
                    let contains = op.guid == part.guid
                        && op.offset <= part.offset
                        && u64::from(part.offset) + u64::from(part.size)
                            <= u64::from(op.offset) + u64::from(op.size);
                    contains.then(|| op.file_offset + u64::from(part.offset - op.offset))
                })
            });
            match reused {
                Some(old_offset) => {
                    job.pieces.push(Piece {
                        source: Source::Old { old_offset },
                        size: part.size,
                    });
                    job.reused += u64::from(part.size);
                    job_uses.push(None);
                }
                None => {
                    job.pieces.push(Piece {
                        source: Source::Fetch {
                            fetch: usize::MAX,
                            offset: part.offset,
                        },
                        size: part.size,
                    });
                    job_uses.push(Some(uses.len()));
                    uses.push(part.guid);
                }
            }
        }
        p.reuse_size += job.reused;
        p.jobs.push(job);
        piece_use.push(job_uses);
    }

    // Decide fetches: a chunk is fetched at its first use, and again if its
    // previous use was more than LOOKAHEAD_PARTS uses earlier (it was evicted).
    let mut fetch_of_use: Vec<usize> = Vec::with_capacity(uses.len());
    let mut last_fetch: HashMap<Guid, (usize, usize)> = HashMap::new(); // guid -> (fetch idx, last use pos)
    for (pos, g) in uses.iter().enumerate() {
        match last_fetch.get_mut(g) {
            Some((fi, last)) if pos - *last <= LOOKAHEAD_PARTS => {
                *last = pos;
                p.fetches[*fi].last_use = pos;
                fetch_of_use.push(*fi);
            }
            _ => {
                let chunk = index.get(g).copied().unwrap_or(usize::MAX);
                p.fetches.push(Fetch {
                    chunk,
                    last_use: pos,
                });
                let fi = p.fetches.len() - 1;
                last_fetch.insert(*g, (fi, pos));
                fetch_of_use.push(fi);
            }
        }
    }
    for (job, uses_of_job) in p.jobs.iter_mut().zip(piece_use) {
        for (piece, u) in job.pieces.iter_mut().zip(uses_of_job) {
            if let (Source::Fetch { fetch, .. }, Some(pos)) = (&mut piece.source, u) {
                *fetch = fetch_of_use[pos];
            }
        }
    }
    p.download_size = p
        .fetches
        .iter()
        .filter_map(|f| new.chunks.get(f.chunk))
        .map(|c| c.file_size.max(0) as u64)
        .sum();

    // Peak memory: simulate fetch-at-first-use / free-after-last-use.
    let mut first_use = vec![usize::MAX; p.fetches.len()];
    for (pos, &fi) in fetch_of_use.iter().enumerate() {
        if first_use[fi] == usize::MAX {
            first_use[fi] = pos;
        }
    }
    let mut delta: Vec<i64> = vec![0; uses.len() + 1];
    for (fi, f) in p.fetches.iter().enumerate() {
        let size = new
            .chunks
            .get(f.chunk)
            .map(|c| i64::from(c.window_size))
            .unwrap_or(0);
        if first_use[fi] != usize::MAX {
            delta[first_use[fi]] += size;
            delta[f.last_use + 1] -= size;
        }
    }
    let (mut live, mut peak) = (0i64, 0i64);
    for d in delta {
        live += d;
        peak = peak.max(live);
    }
    p.peak_cache = peak.max(0) as u64;

    // Disk: new bytes written minus old bytes replaced/deleted, plus one temp file at a time.
    let mut added: i64 = 0;
    let mut largest_tmp: i64 = 0;
    for &(_, f) in &to_write {
        let size = f.size() as i64;
        let old_size = old_files
            .get(f.filename.as_str())
            .map(|o| o.size() as i64)
            .unwrap_or(0);
        added += size - old_size;
        if old_size > 0 {
            largest_tmp = largest_tmp.max(size);
        }
    }
    let deleted: i64 = p
        .delete
        .iter()
        .filter_map(|n| old_files.get(n.as_str()))
        .map(|f| f.size() as i64)
        .sum();
    p.disk_delta = added + largest_tmp - deleted;
    p
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use egs_manifest::{ChunkInfo, ChunkPart, Meta};

    pub fn chunk(n: u32, size: u32) -> ChunkInfo {
        ChunkInfo {
            guid: [n, 0, 0, 0],
            hash: u64::from(n),
            sha1: [0; 20],
            group: 0,
            window_size: size,
            file_size: i64::from(size / 2),
            secret_guid: None,
            window_size_compressed: None,
            encryption_tag: None,
        }
    }

    pub fn file(name: &str, sha: u8, parts: &[(u32, u32, u32)], tags: &[&str]) -> FileManifest {
        let mut off = 0;
        FileManifest {
            filename: name.into(),
            symlink_target: String::new(),
            sha1: [sha; 20],
            flags: 0,
            install_tags: tags.iter().map(|t| (*t).to_owned()).collect(),
            parts: parts
                .iter()
                .map(|&(g, o, s)| {
                    let p = ChunkPart {
                        guid: [g, 0, 0, 0],
                        offset: o,
                        size: s,
                        file_offset: off,
                    };
                    off += u64::from(s);
                    p
                })
                .collect(),
            md5: None,
            mime_type: None,
            sha256: None,
        }
    }

    pub fn manifest(chunks: Vec<ChunkInfo>, files: Vec<FileManifest>) -> Manifest {
        Manifest {
            version: 18,
            json: false,
            meta: Meta {
                feature_level: 18,
                ..Meta::default()
            },
            chunks,
            files,
            custom_fields: Default::default(),
            secret_guid: None,
            encryption_tag: None,
            encrypted: None,
        }
    }

    #[test]
    fn fresh_install_fetches_each_chunk_once_and_respects_tags() {
        let m = manifest(
            vec![chunk(1, 100), chunk(2, 100), chunk(3, 100)],
            vec![
                file("a", 1, &[(1, 0, 50), (2, 0, 100)], &[]),
                file("b", 2, &[(1, 50, 50)], &[]),
                file("lang/de", 3, &[(3, 0, 100)], &["de"]),
            ],
        );
        let p = plan(&m, None, &PlanOptions::default());
        assert_eq!(p.jobs.len(), 2);
        assert_eq!(p.fetches.len(), 2, "chunk 1 is shared and fetched once");
        assert_eq!(p.download_size, 100);
        assert_eq!(p.install_size, 200);
        assert_eq!(p.peak_cache, 200);

        let de = PlanOptions {
            tags: ["de".to_owned()].into(),
            ..PlanOptions::default()
        };
        assert_eq!(plan(&m, None, &de).jobs.len(), 3);
    }

    #[test]
    fn update_skips_unchanged_reuses_old_parts_and_deletes_removed() {
        let old = manifest(
            vec![chunk(1, 100), chunk(2, 100)],
            vec![
                file("keep", 1, &[(1, 0, 100)], &[]),
                file("patch", 2, &[(2, 0, 100)], &[]),
                file("gone", 3, &[(1, 0, 10)], &[]),
            ],
        );
        let new = manifest(
            vec![chunk(1, 100), chunk(2, 100), chunk(4, 100)],
            vec![
                file("keep", 1, &[(1, 0, 100)], &[]),
                file("patch", 9, &[(2, 0, 60), (4, 0, 40)], &[]),
            ],
        );
        let p = plan(&new, Some(&old), &PlanOptions::default());
        assert_eq!(p.skipped, 1);
        assert_eq!(p.jobs.len(), 1);
        assert_eq!(p.reuse_size, 60, "first 60 bytes come from the old file");
        assert_eq!(p.fetches.len(), 1, "only the new chunk is downloaded");
        assert_eq!(p.delete, vec!["gone".to_owned()]);
        assert_eq!(p.jobs[0].pieces[0].source, Source::Old { old_offset: 0 });
    }

    #[test]
    fn resume_skips_completed_and_far_reuse_refetches() {
        let mut files = vec![file("first", 1, &[(1, 0, 10)], &[])];
        for i in 0..LOOKAHEAD_PARTS + 5 {
            files.push(file(
                &format!("filler{i}"),
                2,
                &[(2, (i % 50) as u32, 1)],
                &[],
            ));
        }
        files.push(file("last", 3, &[(1, 10, 10)], &[]));
        let m = manifest(vec![chunk(1, 100), chunk(2, 100)], files);
        let p = plan(&m, None, &PlanOptions::default());
        // chunk 1 is used at the start and again after > LOOKAHEAD uses: fetched twice.
        assert_eq!(p.fetches.iter().filter(|f| f.chunk == 0).count(), 2);
        assert!(p.peak_cache <= 200);

        let resumed = PlanOptions {
            completed: ["first".to_owned()].into(),
            ..PlanOptions::default()
        };
        assert_eq!(plan(&m, None, &resumed).skipped, 1);
    }
}
