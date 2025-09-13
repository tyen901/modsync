//! Utilities for discovering and launching Arma 3.
//!
//! The game executable may live in different places on different operating
//! systems.  On Windows it is typically installed via Steam under
//! `C:\\Program Files (x86)\\Steam\\steamapps\\common\\Arma 3\\arma3_x64.exe`, whereas
//! on Linux (via Proton) it might be in your Steam library under
//! `~/.steam/steam/steamapps/common/Arma 3/arma3_x64.exe`.  The `detect_arma_path`
//! function tries a handful of common locations and honours the
//! `ARMA3_PATH` environment variable if set.

use anyhow::{Context, Result};
use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::config::Metadata;

/// Attempts to detect a reasonable default location for the Arma 3
/// executable on this system.  Returns `None` if no candidate could be
/// found.  The logic is simple and does not cover every installation
/// scenario, but it should work for typical setups on Windows and Linux.
pub fn detect_arma_path() -> Option<PathBuf> {
    // Honour explicit override via environment variable.
    if let Ok(p) = env::var("ARMA3_PATH") {
        let candidate = PathBuf::from(p);
        if candidate.exists() {
            return Some(candidate);
        }
    }
    // Check a few common Windows locations (assuming NTFS style paths).
    let windows_candidates = [
        "C:/Program Files (x86)/Steam/steamapps/common/Arma 3/arma3_x64.exe",
        "C:/Program Files/Steam/steamapps/common/Arma 3/arma3_x64.exe",
    ];
    for cand in &windows_candidates {
        let path = PathBuf::from(cand);
        if path.exists() {
            return Some(path);
        }
    }
    // Check common Linux Proton installation paths.
    let home = env::var("HOME").unwrap_or_else(|_| String::from(""));
    let linux_candidates = [
        format!("{home}/.steam/steam/steamapps/common/Arma 3/arma3_x64.exe"),
        format!("{home}/.local/share/Steam/steamapps/common/Arma 3/arma3_x64.exe"),
    ];
    for cand in &linux_candidates {
        let path = PathBuf::from(cand);
        if path.exists() {
            return Some(path);
        }
    }
    None
}

/// Launches the Arma 3 executable and instructs it to connect to the
/// specified server.  The command line flags are based on the Arma
/// documentation; you can add further flags (such as `-mod`) if required.
pub fn launch_arma(arma_executable: &Path, meta: &Metadata) -> Result<()> {
    let mut cmd = Command::new(arma_executable);
    // Build command line arguments.  We include -connect, -port and
    // optionally -password.  The default Arma port is 2302 but we allow
    // overriding it via the metadata.
    cmd.arg(format!("-connect={}", meta.address));
    cmd.arg(format!("-port={}", meta.port));
    if let Some(pass) = &meta.password {
        cmd.arg(format!("-password={}", pass));
    }
    // Spawn the game process.  We do not wait for it to finish (that would
    // block the UI).  Any error launching the process is returned to the
    // caller.
    cmd.spawn().with_context(|| {
        format!(
            "Failed to launch Arma 3 executable at {}",
            arma_executable.display()
        )
    })?;
    Ok(())
}
