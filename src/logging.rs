//! Simple logging initialisation for the TUI-friendly logger.
//!
//! We avoid writing to stdout/stderr directly during the TUI lifetime.
//! Instead we initialise a logger that buffers to a file in the same
//! directory as the config/ executable. The UI already shows in-app logs
//! via a channel; this logger is only used for diagnostics outside the
//! UI or when running headless.

use simplelog::*;
use std::fs::File;
use std::path::PathBuf;

/// Initialise a simple file-backed logger next to the executable. This is
/// safe to call multiple times; repeated calls are ignored.
pub fn init() {
    // If running under the Rust test harness, avoid initialising the
    // file-backed logger by default — tests should keep the console
    // output clean. Tests set the `RUST_TEST_THREADS` environment
    // variable; honour an explicit override via `MODSYNC_FORCE_LOG`.
    if std::env::var("RUST_TEST_THREADS").is_ok() && std::env::var("MODSYNC_FORCE_LOG").is_err() {
        return;
    }

    // Determine path: place a file named `modsync.log` next to the
    // executable where permissions are likely appropriate.
    let mut path = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("modsync.log"));
    path.set_file_name("modsync.log");

    // Attempt to open logfile; if we fail, silently ignore so we don't
    // crash the app during startup.
    if let Ok(file) = File::options().create(true).append(true).open(&path) {
        // Ignore errors setting logger; if a logger is already set this
        // will return an error and we just continue.
    let mut cb = ConfigBuilder::new();
    let _ = cb.set_time_offset_to_local();
    let _ = WriteLogger::init(LevelFilter::Info, cb.build(), file);
    }
}
