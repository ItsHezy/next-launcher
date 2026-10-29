//! Launch arguments, process start and tracking, third-party store hand-off.
//! Spec: docs/protocol/launch.md.
//!
//! No Tauri and no HTTP: the app fetches exchange codes and ownership tokens and passes
//! them in, so argument building is a pure, tested function.

pub mod args;
#[cfg(windows)]
pub mod process;
pub mod third_party;

pub use args::{build, split_args, ExchangeCode, Launch, LaunchError, LaunchSpec};
