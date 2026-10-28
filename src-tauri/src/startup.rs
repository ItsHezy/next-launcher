//! Startup timeline: Rust marks are milliseconds since the first line of `main`;
//! frontend marks (performance.now()) are mapped onto the same clock.

use std::path::Path;
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

use serde::{Deserialize, Serialize};

static T0: OnceLock<Instant> = OnceLock::new();
static MARKS: Mutex<Vec<Mark>> = Mutex::new(Vec::new());

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Mark {
    pub name: String,
    /// Milliseconds since `main()` started.
    pub ms: f64,
}

/// What the frontend reports once its first frame has painted.
#[derive(Debug, Clone, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct FrontendMarks {
    /// `performance.now()` at the moment the report was sent.
    pub now: f64,
    /// Named `performance.now()` values (ms since the page's time origin).
    pub marks: Vec<FrontendMark>,
}

#[derive(Debug, Clone, Deserialize, specta::Type)]
pub struct FrontendMark {
    pub name: String,
    pub t: f64,
}

/// Call first thing in `main`.
pub fn start() {
    let _ = T0.set(Instant::now());
    mark("main");
}

pub fn elapsed_ms() -> f64 {
    T0.get()
        .map(|t| t.elapsed().as_secs_f64() * 1000.0)
        .unwrap_or(0.0)
}

pub fn mark(name: &str) {
    let ms = elapsed_ms();
    if let Ok(mut m) = MARKS.lock() {
        m.push(Mark {
            name: name.to_owned(),
            ms,
        });
    }
}

/// Merge frontend marks into the timeline (IPC latency of ~1 ms is ignored).
pub fn add_frontend(report: &FrontendMarks) {
    let origin = elapsed_ms() - report.now;
    if let Ok(mut m) = MARKS.lock() {
        for fm in &report.marks {
            m.push(Mark {
                name: format!("ui:{}", fm.name),
                ms: origin + fm.t,
            });
        }
        m.sort_by(|a, b| a.ms.total_cmp(&b.ms));
    }
}

pub fn snapshot() -> Vec<Mark> {
    MARKS.lock().map(|m| m.clone()).unwrap_or_default()
}

/// Human-readable table for `--bench-startup` and the log.
pub fn render(marks: &[Mark]) -> String {
    let mut out = String::from("startup timeline (ms since main):\n");
    for m in marks {
        out.push_str(&format!("  {:>8.1}  {}\n", m.ms, m.name));
    }
    out
}

pub fn write_json(path: &Path, marks: &[Mark]) -> std::io::Result<()> {
    let body = serde_json::to_vec_pretty(marks).map_err(std::io::Error::other)?;
    std::fs::write(path, body)
}
