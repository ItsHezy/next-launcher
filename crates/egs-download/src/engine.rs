//! Executes a [`Plan`]: parallel chunk fetches into a bounded cache, files
//! assembled in order into `<name>.nl-tmp`, SHA-1 checked, then renamed.

use std::collections::HashMap;
use std::future::Future;
use std::path::{Component, Path, PathBuf};
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, AtomicU8, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use egs_manifest::Manifest;
use sha1::{Digest, Sha1};
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};
use tokio::sync::{Notify, OwnedSemaphorePermit, Semaphore};

use crate::plan::{Plan, Source};

pub type BoxFuture<T> = Pin<Box<dyn Future<Output = T> + Send>>;

/// Fetch and decode one chunk (by index into `manifest.chunks`); returns uncompressed data.
/// The caller handles HTTP, CDN fallback, signed URLs and retries.
pub type FetchFn = Arc<dyn Fn(usize) -> BoxFuture<Result<Vec<u8>, String>> + Send + Sync>;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum DlError {
    #[error("download failed: {0}")]
    Fetch(String),
    #[error("disk error: {0}")]
    Io(String),
    #[error("{0} doesn't match its checksum")]
    Corrupt(String),
    #[error("unsafe path in manifest: {0}")]
    Path(String),
}

impl From<std::io::Error> for DlError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e.to_string())
    }
}

/// Run / pause / cancel shared between the UI and the engine.
#[derive(Clone, Default)]
pub struct Control {
    state: Arc<AtomicU8>,
    wake: Arc<Notify>,
}

const RUN: u8 = 0;
const PAUSE: u8 = 1;
const CANCEL: u8 = 2;

impl Control {
    /// Clear a pause so the same control can drive the next run.
    pub fn resume(&self) {
        self.state.store(RUN, Ordering::SeqCst);
    }
    pub fn pause(&self) {
        self.state.store(PAUSE, Ordering::SeqCst);
        self.wake.notify_waiters();
    }
    pub fn cancel(&self) {
        self.state.store(CANCEL, Ordering::SeqCst);
        self.wake.notify_waiters();
    }
    fn stopped(&self) -> Option<Outcome> {
        match self.state.load(Ordering::SeqCst) {
            PAUSE => Some(Outcome::Paused),
            CANCEL => Some(Outcome::Cancelled),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Completed,
    Paused,
    Cancelled,
}

/// Byte-rate limiter shared by all workers (`0` = unlimited). Rate can change live.
#[derive(Default)]
pub struct Limiter {
    bytes_per_sec: AtomicU64,
    state: Mutex<(f64, Option<Instant>)>,
}

impl Limiter {
    pub fn new(bytes_per_sec: u64) -> Self {
        Self {
            bytes_per_sec: AtomicU64::new(bytes_per_sec),
            state: Mutex::new((0.0, None)),
        }
    }
    pub fn set_rate(&self, bytes_per_sec: u64) {
        self.bytes_per_sec.store(bytes_per_sec, Ordering::SeqCst);
    }
    /// Wait until `n` bytes may be transferred.
    pub async fn acquire(&self, n: u64) {
        let rate = self.bytes_per_sec.load(Ordering::SeqCst);
        if rate == 0 {
            return;
        }
        let wait = {
            let Ok(mut s) = self.state.lock() else { return };
            let now = Instant::now();
            let (tokens, last) = &mut *s;
            let elapsed = last
                .map(|l| now.duration_since(l).as_secs_f64())
                .unwrap_or(1.0);
            // Bucket holds at most one second of tokens.
            *tokens = (*tokens + elapsed * rate as f64).min(rate as f64);
            *last = Some(now);
            *tokens -= n as f64;
            if *tokens >= 0.0 {
                Duration::ZERO
            } else {
                Duration::from_secs_f64(-*tokens / rate as f64)
            }
        };
        if !wait.is_zero() {
            tokio::time::sleep(wait).await;
        }
    }
}

/// Receives progress; called from the engine task (keep it cheap).
pub trait Progress: Send + Sync {
    /// Compressed bytes downloaded so far / uncompressed bytes written so far.
    fn bytes(&self, downloaded: u64, written: u64);
    /// A file finished and was renamed into place (for resume).
    fn file_done(&self, filename: &str);
}

pub struct EngineConfig {
    pub workers: usize,
    /// Where `Source::Old` pieces are read from. `None` = the write dir (in-place).
    /// Staged updates write to a staging dir and read unchanged bytes from the live install.
    pub old_dir: Option<PathBuf>,
    /// Memory budget for chunk data; raised to the plan's peak if smaller.
    pub cache_budget: u64,
    pub limiter: Arc<Limiter>,
}

/// Join a manifest path onto the install dir, refusing absolute paths and `..`.
pub fn safe_join(dir: &Path, rel: &str) -> Result<PathBuf, DlError> {
    let rel_path = Path::new(rel);
    let mut out = dir.to_path_buf();
    for c in rel_path.components() {
        match c {
            Component::Normal(p) => out.push(p),
            Component::CurDir => {}
            _ => return Err(DlError::Path(rel.to_owned())),
        }
    }
    if out == dir {
        return Err(DlError::Path(rel.to_owned()));
    }
    Ok(out)
}

fn tmp_path(p: &Path) -> PathBuf {
    let mut s = p.as_os_str().to_owned();
    s.push(".nl-tmp");
    PathBuf::from(s)
}

type Entry = (Arc<Vec<u8>>, OwnedSemaphorePermit);

struct Shared {
    cache: Mutex<HashMap<usize, Entry>>,
    arrived: Notify,
    error: Mutex<Option<String>>,
    downloaded: AtomicU64,
}

/// Run a plan to completion, pause or cancel.
pub async fn run(
    manifest: Arc<Manifest>,
    plan: Arc<Plan>,
    install_dir: PathBuf,
    fetch: FetchFn,
    cfg: EngineConfig,
    control: Control,
    progress: Arc<dyn Progress>,
) -> Result<Outcome, DlError> {
    // Validate every path before touching the disk.
    for job in &plan.jobs {
        safe_join(&install_dir, &manifest.files[job.file].filename)?;
    }
    for d in &plan.delete {
        safe_join(&install_dir, d)?;
    }
    tokio::fs::create_dir_all(&install_dir).await?;

    let max_window = manifest
        .chunks
        .iter()
        .map(|c| u64::from(c.window_size))
        .max()
        .unwrap_or(1 << 20);
    let workers = cfg.workers.max(1);
    let budget_kib = cfg
        .cache_budget
        .max(plan.peak_cache + workers as u64 * max_window)
        .div_ceil(1024);
    let budget = Arc::new(Semaphore::new(
        budget_kib.min(Semaphore::MAX_PERMITS as u64) as usize,
    ));
    let slots = Arc::new(Semaphore::new(workers));
    let shared = Arc::new(Shared {
        cache: Mutex::new(HashMap::new()),
        arrived: Notify::new(),
        error: Mutex::new(None),
        downloaded: AtomicU64::new(0),
    });

    // Dispatcher: budget is acquired strictly in fetch order, so the writer's next
    // chunk always gets memory before later ones (no deadlock).
    let dispatcher = {
        let (manifest, plan, shared, control) = (
            manifest.clone(),
            plan.clone(),
            shared.clone(),
            control.clone(),
        );
        let limiter = cfg.limiter.clone();
        tokio::spawn(async move {
            for (fi, f) in plan.fetches.iter().enumerate() {
                if control.stopped().is_some() {
                    break;
                }
                let Some(info) = manifest.chunks.get(f.chunk) else {
                    *shared.error.lock().unwrap_or_else(|e| e.into_inner()) =
                        Some("chunk missing from manifest".into());
                    shared.arrived.notify_waiters();
                    break;
                };
                let need = (u64::from(info.window_size).div_ceil(1024)).max(1) as u32;
                let Ok(permit) = budget.clone().acquire_many_owned(need).await else {
                    break;
                };
                let Ok(slot) = slots.clone().acquire_owned().await else {
                    break;
                };
                let (fetch, shared, limiter, control) = (
                    fetch.clone(),
                    shared.clone(),
                    limiter.clone(),
                    control.clone(),
                );
                let (chunk, sha, size) = (f.chunk, info.sha1, info.file_size.max(0) as u64);
                tokio::spawn(async move {
                    let _slot = slot;
                    if control.stopped().is_some() {
                        return;
                    }
                    limiter.acquire(size).await;
                    let result = fetch(chunk).await.and_then(|data| {
                        if sha != [0; 20] && Sha1::digest(&data).as_slice() != sha {
                            Err("chunk checksum mismatch".into())
                        } else {
                            Ok(data)
                        }
                    });
                    match result {
                        Ok(data) => {
                            shared.downloaded.fetch_add(size, Ordering::Relaxed);
                            if let Ok(mut c) = shared.cache.lock() {
                                c.insert(fi, (Arc::new(data), permit));
                            }
                        }
                        Err(e) => {
                            if let Ok(mut err) = shared.error.lock() {
                                err.get_or_insert(e);
                            }
                        }
                    }
                    shared.arrived.notify_waiters();
                });
            }
        })
    };

    let old_dir = cfg.old_dir.clone().unwrap_or_else(|| install_dir.clone());
    let result = write_all(
        &manifest,
        &plan,
        &install_dir,
        &old_dir,
        &shared,
        &control,
        progress.as_ref(),
    )
    .await;
    dispatcher.abort();
    if let Ok(mut c) = shared.cache.lock() {
        c.clear();
    }
    let outcome = result?;
    // Staged runs leave deletions to the caller's commit step.
    if outcome == Outcome::Completed && cfg.old_dir.is_none() {
        for name in &plan.delete {
            let p = safe_join(&install_dir, name)?;
            match tokio::fs::remove_file(&p).await {
                Ok(()) => remove_empty_parents(&p, &install_dir).await,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => {
                    tracing::warn!(error = %e, "could not delete a file removed by the update")
                }
            }
        }
    }
    Ok(outcome)
}

async fn wait_for(
    shared: &Shared,
    control: &Control,
    fi: usize,
) -> Result<Result<Arc<Vec<u8>>, Outcome>, DlError> {
    loop {
        let notified = shared.arrived.notified();
        tokio::pin!(notified);
        notified.as_mut().enable();
        let wake = control.wake.notified();
        tokio::pin!(wake);
        wake.as_mut().enable();
        if let Some(entry) = shared
            .cache
            .lock()
            .ok()
            .and_then(|c| c.get(&fi).map(|e| e.0.clone()))
        {
            return Ok(Ok(entry));
        }
        if let Some(e) = shared.error.lock().ok().and_then(|e| e.clone()) {
            return Err(DlError::Fetch(e));
        }
        if let Some(o) = control.stopped() {
            return Ok(Err(o));
        }
        tokio::select! {
            _ = &mut notified => {}
            _ = &mut wake => {}
            _ = tokio::time::sleep(Duration::from_secs(5)) => {}
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn write_all(
    manifest: &Manifest,
    plan: &Plan,
    dir: &Path,
    old_dir: &Path,
    shared: &Shared,
    control: &Control,
    progress: &dyn Progress,
) -> Result<Outcome, DlError> {
    let mut written = 0u64;
    let mut pos = 0usize; // position in part-use order (matches the planner)
    let mut last_report = Instant::now();
    let mut buf = Vec::new();

    for job in &plan.jobs {
        let file = &manifest.files[job.file];
        let path = safe_join(dir, &file.filename)?;
        if !file.symlink_target.is_empty() {
            tracing::warn!("skipping a symlink entry (not supported on Windows)");
            continue;
        }
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let tmp = tmp_path(&path);
        let mut out =
            tokio::io::BufWriter::with_capacity(4 << 20, tokio::fs::File::create(&tmp).await?);
        let mut old = None;
        let mut hasher = Sha1::new();

        for piece in &job.pieces {
            if let Some(o) = control.stopped() {
                drop(out);
                let _ = tokio::fs::remove_file(&tmp).await;
                return Ok(o);
            }
            let size = piece.size as usize;
            match piece.source {
                Source::Old { old_offset } => {
                    if old.is_none() {
                        old =
                            Some(tokio::fs::File::open(safe_join(old_dir, &file.filename)?).await?);
                    }
                    let f = old.as_mut().ok_or_else(|| DlError::Io("old file".into()))?;
                    f.seek(std::io::SeekFrom::Start(old_offset)).await?;
                    buf.resize(size, 0);
                    f.read_exact(&mut buf).await?;
                    hasher.update(&buf);
                    out.write_all(&buf).await?;
                }
                Source::Fetch { fetch, offset } => {
                    let data = match wait_for(shared, control, fetch).await? {
                        Ok(d) => d,
                        Err(o) => {
                            drop(out);
                            let _ = tokio::fs::remove_file(&tmp).await;
                            return Ok(o);
                        }
                    };
                    let start = offset as usize;
                    let slice = data
                        .get(start..start + size)
                        .ok_or_else(|| DlError::Corrupt(file.filename.clone()))?;
                    hasher.update(slice);
                    out.write_all(slice).await?;
                    if plan.fetches[fetch].last_use == pos {
                        if let Ok(mut c) = shared.cache.lock() {
                            c.remove(&fetch); // releases its memory budget
                        }
                    }
                    pos += 1;
                }
            }
            written += piece.size as u64;
            if last_report.elapsed() >= Duration::from_millis(250) {
                progress.bytes(shared.downloaded.load(Ordering::Relaxed), written);
                last_report = Instant::now();
            }
        }

        out.flush().await?;
        let f = out.into_inner();
        f.sync_data().await?;
        drop(f);
        drop(old);
        if file.sha1 != [0; 20] && hasher.finalize().as_slice() != file.sha1 {
            let _ = tokio::fs::remove_file(&tmp).await;
            return Err(DlError::Corrupt(file.filename.clone()));
        }
        tokio::fs::rename(&tmp, &path).await?;
        progress.file_done(&file.filename);
    }
    progress.bytes(shared.downloaded.load(Ordering::Relaxed), written);
    Ok(Outcome::Completed)
}

/// Move a finished staged update into the install and apply deletions.
/// Every staged file was already verified; renames on one volume are fast.
/// Present in the staging folder while a commit is moving files: the install is a mix of
/// old and new files until the commit finishes (callers refuse to launch meanwhile).
pub const COMMITTING_MARKER: &str = ".nl-committing";

/// True when an earlier commit was interrupted (crash, power loss) and must be finished.
pub fn commit_interrupted(install: &Path) -> bool {
    install.join(".nl-staging").join(COMMITTING_MARKER).exists()
}

/// Move staged files over the install, then delete files the update removed. Safe to run
/// again after an interruption: files already moved are simply no longer in staging.
pub async fn commit_staged(
    staging: &Path,
    install: &Path,
    delete: &[String],
) -> Result<(), DlError> {
    // The staging folder may already be gone if a crash came after the moves but before
    // the install was recorded; then only the deletions remain.
    tokio::fs::create_dir_all(staging).await?;
    tokio::fs::write(staging.join(COMMITTING_MARKER), b"").await?;
    let mut stack = vec![staging.to_path_buf()];
    while let Some(d) = stack.pop() {
        let mut rd = tokio::fs::read_dir(&d).await?;
        while let Some(e) = rd.next_entry().await? {
            let p = e.path();
            if e.file_type().await?.is_dir() {
                stack.push(p);
                continue;
            }
            if p.to_string_lossy().ends_with(".nl-tmp")
                || p.file_name() == Some(COMMITTING_MARKER.as_ref())
            {
                continue;
            }
            let rel = p
                .strip_prefix(staging)
                .map_err(|_| DlError::Path(p.display().to_string()))?;
            let target = install.join(rel);
            if let Some(parent) = target.parent() {
                tokio::fs::create_dir_all(parent).await?;
            }
            tokio::fs::rename(&p, &target).await?;
        }
    }
    for name in delete {
        let p = safe_join(install, name)?;
        match tokio::fs::remove_file(&p).await {
            Ok(()) => remove_empty_parents(&p, install).await,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => tracing::warn!(error = %e, "could not delete a file removed by the update"),
        }
    }
    let _ = tokio::fs::remove_dir_all(staging).await;
    Ok(())
}

async fn remove_empty_parents(file: &Path, root: &Path) {
    let mut dir = file.parent();
    while let Some(d) = dir {
        if d == root || !d.starts_with(root) {
            break;
        }
        if tokio::fs::remove_dir(d).await.is_err() {
            break; // not empty
        }
        dir = d.parent();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::{plan, PlanOptions};
    use egs_manifest::{ChunkInfo, ChunkPart, FileManifest, Meta};
    use std::sync::atomic::AtomicUsize;

    /// `(chunk id, offset, size)` slices that make up one file.
    type Parts<'a> = &'a [(u32, u32, u32)];

    /// Builds chunks with real data and SHA-1s, and a manifest using them.
    fn fixture(
        files: &[(&str, Parts<'_>)],
        chunks: &[(u32, Vec<u8>)],
    ) -> (Manifest, HashMap<usize, Vec<u8>>) {
        let infos: Vec<ChunkInfo> = chunks
            .iter()
            .map(|(id, data)| ChunkInfo {
                guid: [*id, 0, 0, 0],
                hash: 0,
                sha1: Sha1::digest(data).into(),
                group: 0,
                window_size: data.len() as u32,
                file_size: data.len() as i64,
                secret_guid: None,
                window_size_compressed: None,
                encryption_tag: None,
            })
            .collect();
        let by_guid: HashMap<u32, &Vec<u8>> = chunks.iter().map(|(id, d)| (*id, d)).collect();
        let fms = files
            .iter()
            .map(|(name, parts)| {
                let mut content = Vec::new();
                let mut off = 0;
                let parts = parts
                    .iter()
                    .map(|&(g, o, s)| {
                        content.extend_from_slice(&by_guid[&g][o as usize..(o + s) as usize]);
                        let p = ChunkPart {
                            guid: [g, 0, 0, 0],
                            offset: o,
                            size: s,
                            file_offset: off,
                        };
                        off += u64::from(s);
                        p
                    })
                    .collect();
                FileManifest {
                    filename: (*name).into(),
                    symlink_target: String::new(),
                    sha1: Sha1::digest(&content).into(),
                    flags: 0,
                    install_tags: vec![],
                    parts,
                    md5: None,
                    mime_type: None,
                    sha256: None,
                }
            })
            .collect();
        let m = Manifest {
            version: 18,
            json: false,
            meta: Meta {
                feature_level: 18,
                ..Meta::default()
            },
            chunks: infos,
            files: fms,
            custom_fields: Default::default(),
            secret_guid: None,
            encryption_tag: None,
            encrypted: None,
        };
        let data = chunks
            .iter()
            .enumerate()
            .map(|(i, (_, d))| (i, d.clone()))
            .collect();
        (m, data)
    }

    struct Rec(Mutex<Vec<String>>);
    impl Progress for Rec {
        fn bytes(&self, _: u64, _: u64) {}
        fn file_done(&self, f: &str) {
            self.0.lock().unwrap().push(f.to_owned());
        }
    }

    fn fetcher(data: HashMap<usize, Vec<u8>>, calls: Arc<AtomicUsize>) -> FetchFn {
        let data = Arc::new(data);
        Arc::new(move |i| {
            let (data, calls) = (data.clone(), calls.clone());
            Box::pin(async move {
                calls.fetch_add(1, Ordering::SeqCst);
                tokio::time::sleep(Duration::from_millis(2)).await;
                data.get(&i)
                    .cloned()
                    .ok_or_else(|| "no such chunk".to_owned())
            })
        })
    }

    fn cfg() -> EngineConfig {
        EngineConfig {
            workers: 4,
            old_dir: None,
            cache_budget: 1 << 20,
            limiter: Arc::new(Limiter::new(0)),
        }
    }

    #[tokio::test]
    async fn installs_files_byte_exact() {
        let a: Vec<u8> = (0..5000u32).map(|i| (i * 7) as u8).collect();
        let b: Vec<u8> = (0..3000u32).map(|i| (i * 13 + 1) as u8).collect();
        let (m, data) = fixture(
            &[
                ("bin/game.exe", &[(1, 0, 4000), (2, 100, 2000)]),
                ("data/a.pak", &[(1, 4000, 1000)]),
                ("empty.txt", &[]),
            ],
            &[(1, a.clone()), (2, b.clone())],
        );
        let dir = tempfile::tempdir().unwrap();
        let p = plan(&m, None, &PlanOptions::default());
        let calls = Arc::new(AtomicUsize::new(0));
        let rec = Arc::new(Rec(Mutex::new(vec![])));
        let out = run(
            Arc::new(m),
            Arc::new(p),
            dir.path().to_path_buf(),
            fetcher(data, calls.clone()),
            cfg(),
            Control::default(),
            rec.clone(),
        )
        .await
        .unwrap();
        assert_eq!(out, Outcome::Completed);
        let exe = std::fs::read(dir.path().join("bin/game.exe")).unwrap();
        assert_eq!(&exe[..4000], &a[..4000]);
        assert_eq!(&exe[4000..], &b[100..2100]);
        assert_eq!(
            std::fs::read(dir.path().join("data/a.pak")).unwrap(),
            a[4000..5000]
        );
        assert_eq!(
            std::fs::read(dir.path().join("empty.txt")).unwrap().len(),
            0
        );
        assert_eq!(calls.load(Ordering::SeqCst), 2, "each chunk fetched once");
        assert_eq!(rec.0.lock().unwrap().len(), 3);
        assert!(!dir.path().join("bin/game.exe.nl-tmp").exists());
    }

    #[tokio::test]
    async fn bad_chunk_data_fails_without_leaving_files() {
        let a = vec![1u8; 100];
        let (m, mut data) = fixture(&[("f", &[(1, 0, 100)])], &[(1, a)]);
        data.insert(0, vec![2u8; 100]); // corrupt
        let dir = tempfile::tempdir().unwrap();
        let p = plan(&m, None, &PlanOptions::default());
        let err = run(
            Arc::new(m),
            Arc::new(p),
            dir.path().to_path_buf(),
            fetcher(data, Arc::new(AtomicUsize::new(0))),
            cfg(),
            Control::default(),
            Arc::new(Rec(Mutex::new(vec![]))),
        )
        .await
        .unwrap_err();
        assert!(matches!(err, DlError::Fetch(_)), "{err:?}");
        assert!(!dir.path().join("f").exists());
    }

    #[tokio::test]
    async fn cancel_stops_and_cleans_temp_file() {
        let a = vec![3u8; 100];
        let (m, data) = fixture(&[("f", &[(1, 0, 100)])], &[(1, a)]);
        let dir = tempfile::tempdir().unwrap();
        let p = plan(&m, None, &PlanOptions::default());
        let ctl = Control::default();
        ctl.cancel();
        let out = run(
            Arc::new(m),
            Arc::new(p),
            dir.path().to_path_buf(),
            fetcher(data, Arc::new(AtomicUsize::new(0))),
            cfg(),
            ctl,
            Arc::new(Rec(Mutex::new(vec![]))),
        )
        .await
        .unwrap();
        assert_eq!(out, Outcome::Cancelled);
        assert!(!dir.path().join("f").exists() && !dir.path().join("f.nl-tmp").exists());
    }

    /// Pausing stops the run early; a new run planned with the finished files skips them.
    #[tokio::test]
    async fn pause_then_resume_finishes_the_rest() {
        let chunks: Vec<(u32, Vec<u8>)> = (1..=6u32).map(|id| (id, vec![id as u8; 1000])).collect();
        let names = ["a", "b", "c", "d", "e", "f"];
        let parts: Vec<[(u32, u32, u32); 1]> = (1..=6u32).map(|id| [(id, 0, 1000)]).collect();
        let files: Vec<(&str, Parts<'_>)> = names
            .iter()
            .zip(&parts)
            .map(|(n, p)| (*n, &p[..]))
            .collect();
        let (m, data) = fixture(&files, &chunks);
        let m = Arc::new(m);
        let dir = tempfile::tempdir().unwrap();

        struct PauseAfterFirst(Control, Mutex<Vec<String>>);
        impl Progress for PauseAfterFirst {
            fn bytes(&self, _: u64, _: u64) {}
            fn file_done(&self, f: &str) {
                self.1.lock().unwrap().push(f.to_owned());
                self.0.pause();
            }
        }
        let ctl = Control::default();
        let rec = Arc::new(PauseAfterFirst(ctl.clone(), Mutex::new(vec![])));
        let out = run(
            m.clone(),
            Arc::new(plan(&m, None, &PlanOptions::default())),
            dir.path().to_path_buf(),
            fetcher(data.clone(), Arc::new(AtomicUsize::new(0))),
            EngineConfig {
                workers: 1,
                ..cfg()
            },
            ctl,
            rec.clone(),
        )
        .await
        .unwrap();
        assert_eq!(out, Outcome::Paused);
        let done: std::collections::HashSet<String> =
            rec.1.lock().unwrap().iter().cloned().collect();
        assert!(!done.is_empty() && done.len() < names.len(), "{done:?}");

        let calls = Arc::new(AtomicUsize::new(0));
        let rest = plan(
            &m,
            None,
            &PlanOptions {
                completed: done.clone(),
                ..PlanOptions::default()
            },
        );
        let out = run(
            m.clone(),
            Arc::new(rest),
            dir.path().to_path_buf(),
            fetcher(data, calls.clone()),
            cfg(),
            Control::default(),
            Arc::new(Rec(Mutex::new(vec![]))),
        )
        .await
        .unwrap();
        assert_eq!(out, Outcome::Completed);
        assert_eq!(
            calls.load(Ordering::SeqCst),
            names.len() - done.len(),
            "finished files aren't fetched again"
        );
        for (i, n) in names.iter().enumerate() {
            assert_eq!(
                std::fs::read(dir.path().join(n)).unwrap(),
                vec![(i + 1) as u8; 1000]
            );
            assert!(!dir.path().join(format!("{n}.nl-tmp")).exists());
        }
    }

    #[tokio::test]
    async fn update_patches_from_old_file_and_deletes_removed() {
        let a: Vec<u8> = (0..1000u32).map(|i| i as u8).collect();
        let b: Vec<u8> = (0..1000u32).map(|i| (255 - i % 256) as u8).collect();
        let (old, old_data) = fixture(
            &[("f", &[(1, 0, 600)]), ("gone", &[(1, 0, 10)])],
            &[(1, a.clone())],
        );
        let dir = tempfile::tempdir().unwrap();
        let p = plan(&old, None, &PlanOptions::default());
        run(
            Arc::new(old.clone()),
            Arc::new(p),
            dir.path().to_path_buf(),
            fetcher(old_data, Arc::new(AtomicUsize::new(0))),
            cfg(),
            Control::default(),
            Arc::new(Rec(Mutex::new(vec![]))),
        )
        .await
        .unwrap();

        // New build: keeps the first 400 bytes of chunk 1 and appends new chunk 2 data.
        let (new, new_data) = fixture(
            &[("f", &[(1, 0, 400), (2, 0, 300)])],
            &[(1, a.clone()), (2, b.clone())],
        );
        let p = plan(&new, Some(&old), &PlanOptions::default());
        assert_eq!(p.reuse_size, 400);
        let calls = Arc::new(AtomicUsize::new(0));
        run(
            Arc::new(new),
            Arc::new(p),
            dir.path().to_path_buf(),
            fetcher(new_data, calls.clone()),
            cfg(),
            Control::default(),
            Arc::new(Rec(Mutex::new(vec![]))),
        )
        .await
        .unwrap();
        let f = std::fs::read(dir.path().join("f")).unwrap();
        assert_eq!(&f[..400], &a[..400]);
        assert_eq!(&f[400..], &b[..300]);
        assert_eq!(
            calls.load(Ordering::SeqCst),
            1,
            "only the new chunk is downloaded"
        );
        assert!(!dir.path().join("gone").exists());
    }

    #[tokio::test]
    async fn staged_update_keeps_old_install_until_commit() {
        let a: Vec<u8> = (0..1000u32).map(|i| i as u8).collect();
        let b: Vec<u8> = vec![9u8; 1000];
        let (old, old_data) = fixture(
            &[("f", &[(1, 0, 600)]), ("gone", &[(1, 0, 10)])],
            &[(1, a.clone())],
        );
        let dir = tempfile::tempdir().unwrap();
        let install = dir.path().join("game");
        let p = plan(&old, None, &PlanOptions::default());
        run(
            Arc::new(old.clone()),
            Arc::new(p),
            install.clone(),
            fetcher(old_data, Arc::new(AtomicUsize::new(0))),
            cfg(),
            Control::default(),
            Arc::new(Rec(Mutex::new(vec![]))),
        )
        .await
        .unwrap();
        let before = std::fs::read(install.join("f")).unwrap();

        let (new, new_data) = fixture(
            &[("f", &[(1, 0, 400), (2, 0, 300)])],
            &[(1, a.clone()), (2, b.clone())],
        );
        let p = plan(&new, Some(&old), &PlanOptions::default());
        let staging = install.join(".nl-staging");
        let mut c = cfg();
        c.old_dir = Some(install.clone());
        let delete = p.delete.clone();
        run(
            Arc::new(new),
            Arc::new(p),
            staging.clone(),
            fetcher(new_data, Arc::new(AtomicUsize::new(0))),
            c,
            Control::default(),
            Arc::new(Rec(Mutex::new(vec![]))),
        )
        .await
        .unwrap();
        assert_eq!(
            std::fs::read(install.join("f")).unwrap(),
            before,
            "live install untouched before commit"
        );
        assert!(install.join("gone").exists());

        commit_staged(&staging, &install, &delete).await.unwrap();
        let f = std::fs::read(install.join("f")).unwrap();
        assert_eq!(&f[..400], &a[..400]);
        assert_eq!(&f[400..], &b[..300]);
        assert!(!install.join("gone").exists() && !staging.exists());
        assert!(!commit_interrupted(&install));
    }

    /// A crash part-way through a commit: some files moved, the marker left behind. Running
    /// the commit again finishes it.
    #[tokio::test]
    async fn interrupted_commit_finishes_on_the_next_run() {
        let dir = tempfile::tempdir().unwrap();
        let install = dir.path().join("game");
        let staging = install.join(".nl-staging");
        std::fs::create_dir_all(staging.join("sub")).unwrap();
        std::fs::write(install.join("a"), b"old a").unwrap();
        std::fs::write(install.join("b"), b"old b").unwrap();
        std::fs::write(install.join("removed"), b"x").unwrap();
        // What the crash left: `a` already moved, `sub/b2` and `b` still staged, marker present.
        std::fs::write(install.join("a"), b"new a").unwrap();
        std::fs::write(staging.join("b"), b"new b").unwrap();
        std::fs::write(staging.join("sub").join("b2"), b"new b2").unwrap();
        std::fs::write(staging.join(COMMITTING_MARKER), b"").unwrap();
        assert!(commit_interrupted(&install));

        commit_staged(&staging, &install, &["removed".to_owned()])
            .await
            .unwrap();
        assert_eq!(std::fs::read(install.join("a")).unwrap(), b"new a");
        assert_eq!(std::fs::read(install.join("b")).unwrap(), b"new b");
        assert_eq!(
            std::fs::read(install.join("sub").join("b2")).unwrap(),
            b"new b2"
        );
        assert!(!install.join("removed").exists());
        assert!(
            !install.join(COMMITTING_MARKER).exists(),
            "marker never copied into the game"
        );
        assert!(!staging.exists() && !commit_interrupted(&install));

        // Crash after the moves, before the install was recorded: staging is gone; the rerun
        // still succeeds and just applies deletions.
        std::fs::write(install.join("removed2"), b"x").unwrap();
        commit_staged(&staging, &install, &["removed2".to_owned()])
            .await
            .unwrap();
        assert!(!install.join("removed2").exists() && !staging.exists());
    }

    #[test]
    fn safe_join_rejects_escapes() {
        let d = Path::new("C:/Games/X");
        assert!(safe_join(d, "bin/a.exe").is_ok());
        assert!(safe_join(d, "../evil.exe").is_err());
        assert!(safe_join(d, "C:/Windows/evil.exe").is_err());
        assert!(safe_join(d, "/abs").is_err());
        assert!(safe_join(d, "").is_err());
    }

    #[tokio::test]
    async fn limiter_paces_transfers() {
        let l = Limiter::new(1000);
        let t = Instant::now();
        l.acquire(1000).await; // first second's tokens are available
        l.acquire(500).await; // must wait ~0.5 s
        assert!(
            t.elapsed() >= Duration::from_millis(400),
            "{:?}",
            t.elapsed()
        );
    }
}
