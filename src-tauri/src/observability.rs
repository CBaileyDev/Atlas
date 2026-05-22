//! Tracing setup. Pretty output to stderr in dev, JSON-lines to a
//! daily-rotated file under the platform-specific data directory.
//!
//! Plan §12.3 mandates:
//! - logs at `%APPDATA%\CodexAtlas\logs\atlas.log` on Windows,
//! - 7 days of rotation,
//! - JSON to file, pretty to stderr.

use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{fmt, EnvFilter};

/// Initialize tracing. Returns a `WorkerGuard` whose `Drop` impl flushes
/// the background log writer. **Hold this for the lifetime of the app**
/// — drop it and you lose buffered log lines on exit. (`WorkerGuard`
/// itself is already `#[must_use]`, so callers will get a lint warning
/// if they drop it immediately.)
pub fn init() -> WorkerGuard {
    // Read RUST_LOG; default to info for atlas crates, warn for everything else.
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("atlas_core=info,codex_atlas_lib=info,warn"));

    let log_dir = atlas_core::paths::log_dir().unwrap_or_else(|_| {
        // If we can't resolve the data dir for any reason, fall back to a
        // local "logs" folder. Better than crashing during startup.
        let p = std::path::PathBuf::from("logs");
        let _ = std::fs::create_dir_all(&p);
        p
    });

    let appender: RollingFileAppender = tracing_appender::rolling::Builder::new()
        .rotation(Rotation::DAILY)
        .filename_prefix("atlas")
        .filename_suffix("log")
        .max_log_files(7)
        .build(&log_dir)
        .expect("could not create log appender");

    let (file_writer, guard) = tracing_appender::non_blocking(appender);

    let file_layer = fmt::layer()
        .json()
        .with_writer(file_writer)
        .with_ansi(false)
        .with_target(true)
        .with_thread_ids(false)
        .with_file(false)
        .with_line_number(false);

    let stderr_layer = fmt::layer()
        .with_writer(std::io::stderr)
        .with_ansi(true)
        .with_target(true)
        .with_thread_ids(false)
        .compact();

    tracing_subscriber::registry()
        .with(env_filter)
        .with(file_layer)
        .with(stderr_layer)
        .init();

    guard
}
