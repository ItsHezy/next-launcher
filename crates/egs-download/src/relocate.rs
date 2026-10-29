//! Move an install folder somewhere else (downloads-cdn.md §8).
//!
//! Same drive: one rename. Another drive: copy everything (game files and anything
//! the game wrote next to them), check each copy's size, flush it, and only then
//! delete the old folder. Until that delete the old folder is never touched.

use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use crate::engine::DlError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Moved {
    pub files: usize,
    pub bytes: u64,
    /// Same drive: renamed in place, nothing copied.
    pub renamed: bool,
    /// Everything was copied, but some old files couldn't be deleted (e.g. in use).
    pub left_behind: bool,
}

/// Lower-cased path components, for Windows' case-insensitive comparisons.
fn key(p: &Path) -> Vec<String> {
    p.components()
        .filter(|c| !matches!(c, Component::CurDir))
        .map(|c| c.as_os_str().to_string_lossy().to_lowercase())
        .collect()
}

/// True if `inner` is `outer` or inside it.
pub fn is_within(inner: &Path, outer: &Path) -> bool {
    let (i, o) = (key(inner), key(outer));
    i.len() >= o.len() && i[..o.len()] == o[..]
}

/// Same drive letter / UNC share (a rename can work).
pub fn same_volume(a: &Path, b: &Path) -> bool {
    match (a.components().next(), b.components().next()) {
        (Some(Component::Prefix(x)), Some(Component::Prefix(y))) => {
            x.as_os_str().to_string_lossy().to_lowercase()
                == y.as_os_str().to_string_lossy().to_lowercase()
        }
        (x, y) => x == y,
    }
}

/// Contents of a folder, relative to it.
#[derive(Debug, Default)]
pub struct Tree {
    /// (relative path, size)
    pub files: Vec<(PathBuf, u64)>,
    pub dirs: Vec<PathBuf>,
}

impl Tree {
    pub fn bytes(&self) -> u64 {
        self.files.iter().map(|(_, s)| s).sum()
    }
}

/// Every file and sub-folder under `dir`. Links are refused: following one could
/// copy or delete data outside the game folder.
pub fn scan(dir: &Path) -> Result<Tree, DlError> {
    let (mut files, mut dirs) = (Vec::new(), Vec::new());
    let mut stack = vec![PathBuf::new()];
    while let Some(rel) = stack.pop() {
        for entry in std::fs::read_dir(dir.join(&rel))? {
            let entry = entry?;
            let ft = entry.file_type()?;
            let r = rel.join(entry.file_name());
            if ft.is_symlink() {
                return Err(DlError::Io(format!(
                    "{} is a link; move this folder by hand",
                    r.display()
                )));
            }
            if ft.is_dir() {
                dirs.push(r.clone());
                stack.push(r);
            } else {
                files.push((r, entry.metadata()?.len()));
            }
        }
    }
    Ok(Tree { files, dirs })
}

fn check_target(from: &Path, to: &Path) -> Result<(), DlError> {
    if is_within(to, from) {
        return Err(DlError::Path(
            "the new folder can't be inside the old one".into(),
        ));
    }
    if !from.is_dir() {
        return Err(DlError::Io(format!("{} doesn't exist", from.display())));
    }
    match std::fs::read_dir(to) {
        Ok(mut it) => {
            if it.next().is_some() {
                return Err(DlError::Io(format!(
                    "{} already has files in it; choose an empty or new folder",
                    to.display()
                )));
            }
            std::fs::remove_dir(to)?; // empty: let the rename / copy create it
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e.into()),
    }
    Ok(())
}

fn cross_device(e: &std::io::Error) -> bool {
    // ERROR_NOT_SAME_DEVICE on Windows, EXDEV elsewhere.
    e.kind() == std::io::ErrorKind::CrossesDevices || e.raw_os_error() == Some(17)
}

/// Move `from` to `to` (which must not exist, or be an empty folder). `progress`
/// receives bytes copied. Returns `Ok(None)` if cancelled; the old folder is then intact
/// and the partial copy removed. `force_copy` skips the rename attempt (tests).
pub fn move_dir(
    from: &Path,
    to: &Path,
    force_copy: bool,
    progress: &AtomicU64,
    cancel: &AtomicBool,
) -> Result<Option<Moved>, DlError> {
    check_target(from, to)?;
    if let Some(parent) = to.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tree = scan(from)?;
    let (files, bytes) = (tree.files.len(), tree.bytes());
    if !force_copy {
        match std::fs::rename(from, to) {
            Ok(()) => {
                progress.store(bytes, Ordering::Relaxed);
                return Ok(Some(Moved {
                    files,
                    bytes,
                    renamed: true,
                    left_behind: false,
                }));
            }
            Err(e) if cross_device(&e) => {}
            Err(e) => return Err(e.into()),
        }
    }
    match copy_all(from, to, &tree, progress, cancel) {
        Ok(true) => {}
        Ok(false) => {
            let _ = std::fs::remove_dir_all(to);
            return Ok(None);
        }
        Err(e) => {
            let _ = std::fs::remove_dir_all(to);
            return Err(e);
        }
    }
    let left_behind = std::fs::remove_dir_all(from).is_err();
    Ok(Some(Moved {
        files,
        bytes,
        renamed: false,
        left_behind,
    }))
}

/// `Ok(false)` = cancelled.
fn copy_all(
    from: &Path,
    to: &Path,
    tree: &Tree,
    progress: &AtomicU64,
    cancel: &AtomicBool,
) -> Result<bool, DlError> {
    std::fs::create_dir(to)?;
    for d in &tree.dirs {
        std::fs::create_dir_all(to.join(d))?;
    }
    let mut buf = vec![0u8; 4 << 20];
    for (rel, size) in &tree.files {
        let (src_path, dst_path) = (from.join(rel), to.join(rel));
        let io = |e: std::io::Error| DlError::Io(format!("{}: {e}", rel.display()));
        let mut src = std::fs::File::open(&src_path).map_err(io)?;
        let meta = src.metadata().map_err(io)?;
        let mut dst = std::fs::File::create_new(&dst_path).map_err(io)?;
        let mut copied = 0u64;
        loop {
            if cancel.load(Ordering::Relaxed) {
                return Ok(false);
            }
            let n = src.read(&mut buf).map_err(io)?;
            if n == 0 {
                break;
            }
            dst.write_all(&buf[..n]).map_err(io)?;
            copied += n as u64;
            progress.fetch_add(n as u64, Ordering::Relaxed);
        }
        if copied != *size || dst.metadata().map_err(io)?.len() != *size {
            return Err(DlError::Io(format!(
                "{} changed while it was being copied",
                rel.display()
            )));
        }
        if let Ok(t) = meta.modified() {
            let _ = dst.set_modified(t);
        }
        dst.sync_all().map_err(io)?;
        if meta.permissions().readonly() {
            let _ = std::fs::set_permissions(&dst_path, meta.permissions());
        }
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn game(dir: &Path) {
        std::fs::create_dir_all(dir.join("bin/sub")).unwrap();
        std::fs::write(dir.join("bin/sub/a.exe"), vec![7u8; 5_000_000]).unwrap();
        std::fs::write(dir.join("b.pak"), b"pak").unwrap();
        std::fs::create_dir_all(dir.join("Saved/empty")).unwrap();
        std::fs::write(dir.join("Saved/settings.ini"), b"user data").unwrap();
    }

    fn assert_game(dir: &Path) {
        assert_eq!(
            std::fs::read(dir.join("bin/sub/a.exe")).unwrap().len(),
            5_000_000
        );
        assert_eq!(std::fs::read(dir.join("b.pak")).unwrap(), b"pak");
        assert_eq!(
            std::fs::read(dir.join("Saved/settings.ini")).unwrap(),
            b"user data"
        );
        assert!(dir.join("Saved/empty").is_dir());
    }

    #[test]
    fn rename_and_copy_both_move_everything() {
        for force_copy in [false, true] {
            let tmp = tempfile::tempdir().unwrap();
            let (from, to) = (tmp.path().join("Game"), tmp.path().join("moved/Game"));
            game(&from);
            let progress = AtomicU64::new(0);
            let m = move_dir(&from, &to, force_copy, &progress, &AtomicBool::new(false))
                .unwrap()
                .unwrap();
            assert_eq!((m.files, m.bytes, m.renamed), (3, 5_000_012, !force_copy));
            assert!(!m.left_behind);
            assert!(!from.exists(), "old folder gone");
            assert_game(&to);
            assert_eq!(progress.load(Ordering::Relaxed), 5_000_012);
        }
    }

    #[test]
    fn cancel_keeps_the_original_and_removes_the_copy() {
        let tmp = tempfile::tempdir().unwrap();
        let (from, to) = (tmp.path().join("Game"), tmp.path().join("New"));
        game(&from);
        let r = move_dir(&from, &to, true, &AtomicU64::new(0), &AtomicBool::new(true)).unwrap();
        assert!(r.is_none());
        assert_game(&from);
        assert!(!to.exists());
    }

    #[test]
    fn refuses_unsafe_targets() {
        let tmp = tempfile::tempdir().unwrap();
        let from = tmp.path().join("Game");
        game(&from);
        let (p, c) = (AtomicU64::new(0), AtomicBool::new(false));
        assert!(matches!(
            move_dir(&from, &from.join("inner"), false, &p, &c),
            Err(DlError::Path(_))
        ));
        let busy = tmp.path().join("Busy");
        std::fs::create_dir_all(&busy).unwrap();
        std::fs::write(busy.join("other.txt"), b"x").unwrap();
        assert!(move_dir(&from, &busy, false, &p, &c).is_err());
        assert!(
            busy.join("other.txt").exists(),
            "non-empty target untouched"
        );
        assert_game(&from);

        // An empty target folder is fine.
        let empty = tmp.path().join("Empty");
        std::fs::create_dir_all(&empty).unwrap();
        assert!(move_dir(&from, &empty, true, &p, &c).unwrap().is_some());
        assert_game(&empty);
    }

    #[test]
    fn path_helpers() {
        assert!(is_within(
            Path::new("C:/Games/X/y"),
            Path::new("c:/games/x")
        ));
        assert!(!is_within(
            Path::new("C:/Games/Xy"),
            Path::new("C:/Games/X")
        ));
        assert!(same_volume(Path::new("C:\\A"), Path::new("c:\\B")));
        assert!(!same_volume(Path::new("C:\\A"), Path::new("D:\\A")));
    }
}
