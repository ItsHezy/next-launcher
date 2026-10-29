//! Rotating file logs (Settings → Support → Logs opens this folder).
//! Every line passes through [`crate::redact`] before it is written.

use std::path::Path;
use std::sync::OnceLock;

use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{fmt, EnvFilter};

static GUARD: OnceLock<WorkerGuard> = OnceLock::new();

pub fn init(dir: &Path, debug: bool) {
    let default = if debug || cfg!(debug_assertions) {
        "info,next_launcher=debug,egs_cache=debug"
    } else {
        "info"
    };
    let filter = EnvFilter::try_from_env("NL_LOG").unwrap_or_else(|_| EnvFilter::new(default));

    let appender = tracing_appender::rolling::Builder::new()
        .rotation(tracing_appender::rolling::Rotation::DAILY)
        .filename_prefix("next-launcher")
        .filename_suffix("log")
        .max_log_files(7)
        .build(dir);

    let Ok(appender) = appender else {
        // Logging must never stop the app from starting.
        return;
    };
    let (writer, guard) = tracing_appender::non_blocking(appender);
    let _ = GUARD.set(guard);

    let _ = fmt()
        .with_env_filter(filter)
        .with_writer(crate::redact::Redacting(writer))
        .with_ansi(false)
        .with_target(true)
        .try_init();
}
