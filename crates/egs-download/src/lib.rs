//! Download, patch, verify and repair engine. Spec: `docs/protocol/downloads-cdn.md`.
//!
//! No Tauri and no HTTP here: chunk bytes come from a caller-supplied fetcher, so
//! the engine is fully testable offline.

pub mod engine;
pub mod plan;
pub mod relocate;
pub mod verify;

pub use engine::{
    commit_staged, run, safe_join, Control, DlError, EngineConfig, FetchFn, Limiter, Outcome,
    Progress,
};
pub use plan::{plan, Fetch, FileJob, Piece, Plan, PlanOptions, Source};
pub use relocate::{move_dir, Moved};
pub use verify::{uninstall, verify, Issue, Problem};
