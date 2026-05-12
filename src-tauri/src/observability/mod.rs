/// 可观测性模块
/// 提供指标采集（计数器、直方图）和结构化追踪能力

mod metrics;
mod trace;

pub use metrics::*;
pub use trace::*;

// ---------------------------------------------------------------------------
// Tracing bootstrap: stdout + daily-rotated file logs
// ---------------------------------------------------------------------------
//
// Logs go to `~/.uclaw/logs/uclaw.log.YYYY-MM-DD`. The non-blocking
// writer's `WorkerGuard` MUST be held until process exit or pending
// lines are dropped — `init()` returns it; `main()` binds it to a
// `let _guard = ...;` and lets it drop at shutdown.

use std::path::PathBuf;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{fmt, prelude::*, EnvFilter, Registry};

/// Resolve the log directory, creating it if needed.
fn log_dir() -> PathBuf {
    let base = home_dir().unwrap_or_else(|| PathBuf::from("."));
    let dir = base.join(".uclaw").join("logs");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// Best-effort home directory lookup. Falls back to current dir if HOME
/// isn't set (unusual on macOS but defensive).
fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

fn env_filter() -> EnvFilter {
    EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        // chromiumoxide::handler emits WARN per CDP event Chrome sends
        // outside its schema — noise. Silence it. Match the prior main.rs default.
        EnvFilter::new("info,chromiumoxide::handler=error")
    })
}

/// Initialize tracing for the whole process.
///
/// Returns a `WorkerGuard` whose Drop flushes any pending non-blocking
/// file writes. Hold it in `main` for the lifetime of the process.
pub fn init() -> WorkerGuard {
    let dir = log_dir();
    let file_appender = tracing_appender::rolling::daily(&dir, "uclaw.log");
    let (file_writer, guard) = tracing_appender::non_blocking(file_appender);

    let stdout_layer = fmt::layer().with_writer(std::io::stdout);
    let file_layer = fmt::layer()
        .with_writer(file_writer)
        .with_ansi(false); // no escape codes in the on-disk log

    Registry::default()
        .with(env_filter())
        .with(stdout_layer)
        .with(file_layer)
        .init();

    tracing::info!(?dir, "tracing initialized");

    guard
}
