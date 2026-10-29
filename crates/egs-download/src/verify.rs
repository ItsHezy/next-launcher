//! Verify an install against its manifest, and uninstall it (downloads-cdn.md §4, §7).

use std::collections::HashSet;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use egs_manifest::Manifest;
use sha1::{Digest, Sha1};

use crate::engine::{safe_join, DlError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Problem {
    Missing,
    WrongSize { expected: u64, actual: u64 },
    WrongHash,
    Unreadable(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Issue {
    pub filename: String,
    pub problem: Problem,
}

/// Check every selected file's size and SHA-1. `progress` receives bytes hashed so far.
/// Runs on blocking threads, `parallel` files at a time.
pub async fn verify(
    manifest: Arc<Manifest>,
    dir: PathBuf,
    tags: HashSet<String>,
    parallel: usize,
    progress: Arc<AtomicU64>,
    cancel: Arc<std::sync::atomic::AtomicBool>,
) -> Result<Vec<Issue>, DlError> {
    let files: Vec<usize> = manifest
        .files
        .iter()
        .enumerate()
        .filter(|(_, f)| f.selected(&tags) && f.symlink_target.is_empty())
        .map(|(i, _)| i)
        .collect();
    for &i in &files {
        safe_join(&dir, &manifest.files[i].filename)?;
    }
    let sem = Arc::new(tokio::sync::Semaphore::new(parallel.max(1)));
    let mut set = tokio::task::JoinSet::new();
    for i in files {
        let (m, dir, progress, cancel, sem) = (
            manifest.clone(),
            dir.clone(),
            progress.clone(),
            cancel.clone(),
            sem.clone(),
        );
        set.spawn(async move {
            let _permit = sem.acquire_owned().await;
            let filename = m.files[i].filename.clone();
            tokio::task::spawn_blocking(move || check_file(&m, i, &dir, &progress, &cancel))
                .await
                .unwrap_or_else(|e| {
                    Some(Issue {
                        filename,
                        problem: Problem::Unreadable(e.to_string()),
                    })
                })
        });
    }
    let mut issues = Vec::new();
    while let Some(r) = set.join_next().await {
        if let Ok(Some(issue)) = r {
            issues.push(issue);
        }
    }
    issues.sort_by(|a, b| a.filename.cmp(&b.filename));
    Ok(issues)
}

fn check_file(
    m: &Manifest,
    i: usize,
    dir: &Path,
    progress: &AtomicU64,
    cancel: &std::sync::atomic::AtomicBool,
) -> Option<Issue> {
    let f = &m.files[i];
    let issue = |problem| {
        Some(Issue {
            filename: f.filename.clone(),
            problem,
        })
    };
    let path = safe_join(dir, &f.filename).ok()?;
    let expected = f.size();
    let meta = match std::fs::metadata(&path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            progress.fetch_add(expected, Ordering::Relaxed);
            return issue(Problem::Missing);
        }
        Err(e) => return issue(Problem::Unreadable(e.to_string())),
    };
    if meta.len() != expected {
        progress.fetch_add(expected, Ordering::Relaxed);
        return issue(Problem::WrongSize {
            expected,
            actual: meta.len(),
        });
    }
    let mut file = match std::fs::File::open(&path) {
        Ok(f) => f,
        Err(e) => return issue(Problem::Unreadable(e.to_string())),
    };
    let mut hasher = Sha1::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        if cancel.load(Ordering::Relaxed) {
            return None;
        }
        match file.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                hasher.update(&buf[..n]);
                progress.fetch_add(n as u64, Ordering::Relaxed);
            }
            Err(e) => return issue(Problem::Unreadable(e.to_string())),
        }
    }
    (f.sha1 != [0; 20] && hasher.finalize().as_slice() != f.sha1).then(|| Issue {
        filename: f.filename.clone(),
        problem: Problem::WrongHash,
    })
}

/// Delete exactly the files the manifest installed (all tags), then empty folders.
/// Never removes the install folder itself if anything else is left in it.
pub fn uninstall(manifest: &Manifest, dir: &Path) -> Result<usize, DlError> {
    let mut removed = 0;
    let mut dirs: HashSet<PathBuf> = HashSet::new();
    for f in &manifest.files {
        let p = safe_join(dir, &f.filename)?;
        match std::fs::remove_file(&p) {
            Ok(()) => removed += 1,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(DlError::Io(format!("{}: {e}", f.filename))),
        }
        let _ = std::fs::remove_file(PathBuf::from(format!("{}.nl-tmp", p.display())));
        let mut d = p.parent();
        while let Some(x) = d {
            if x == dir || !x.starts_with(dir) {
                break;
            }
            dirs.insert(x.to_path_buf());
            d = x.parent();
        }
    }
    // Deepest first.
    let mut dirs: Vec<PathBuf> = dirs.into_iter().collect();
    dirs.sort_by_key(|d| std::cmp::Reverse(d.components().count()));
    for d in dirs {
        let _ = std::fs::remove_dir(d); // fails (kept) if not empty
    }
    let _ = std::fs::remove_dir(dir); // only if now empty
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::tests::{chunk, file, manifest};
    use std::sync::atomic::AtomicBool;

    #[tokio::test]
    async fn verify_finds_missing_wrong_size_and_bad_hash() {
        let dir = tempfile::tempdir().unwrap();
        let content = b"hello".to_vec();
        let mut good = file("good.txt", 0, &[(1, 0, 5)], &[]);
        good.sha1 = Sha1::digest(&content).into();
        let mut bad = file("bad.txt", 0, &[(1, 0, 5)], &[]);
        bad.sha1 = Sha1::digest(b"world").into();
        let m = manifest(
            vec![chunk(1, 100)],
            vec![
                good,
                bad,
                file("short.txt", 0, &[(1, 0, 5)], &[]),
                file("missing.txt", 0, &[(1, 0, 5)], &[]),
            ],
        );
        std::fs::write(dir.path().join("good.txt"), &content).unwrap();
        std::fs::write(dir.path().join("bad.txt"), &content).unwrap();
        std::fs::write(dir.path().join("short.txt"), b"hi").unwrap();

        let issues = verify(
            Arc::new(m),
            dir.path().to_path_buf(),
            HashSet::new(),
            2,
            Arc::new(AtomicU64::new(0)),
            Arc::new(AtomicBool::new(false)),
        )
        .await
        .unwrap();
        let names: Vec<_> = issues
            .iter()
            .map(|i| (i.filename.as_str(), &i.problem))
            .collect();
        assert_eq!(
            names,
            vec![
                ("bad.txt", &Problem::WrongHash),
                ("missing.txt", &Problem::Missing),
                (
                    "short.txt",
                    &Problem::WrongSize {
                        expected: 5,
                        actual: 2
                    }
                ),
            ]
        );
    }

    #[test]
    fn uninstall_removes_only_manifest_files() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("Game");
        std::fs::create_dir_all(dir.join("bin/sub")).unwrap();
        std::fs::write(dir.join("bin/sub/a.exe"), b"x").unwrap();
        std::fs::write(dir.join("b.pak"), b"x").unwrap();
        std::fs::write(dir.join("user-save.dat"), b"keep me").unwrap();
        let m = manifest(
            vec![chunk(1, 10)],
            vec![
                file("bin/sub/a.exe", 0, &[(1, 0, 1)], &[]),
                file("b.pak", 0, &[(1, 0, 1)], &[]),
            ],
        );

        assert_eq!(uninstall(&m, &dir).unwrap(), 2);
        assert!(!dir.join("bin").exists(), "empty folders removed");
        assert!(
            dir.join("user-save.dat").exists(),
            "files not in the manifest are kept"
        );
        assert!(dir.exists());
    }
}
