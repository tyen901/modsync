//! Long‑running actions triggered from the UI.
//!
//! The functions in this module encapsulate the logic for each menu
//! action.  They perform potentially blocking operations on a
//! background thread so as not to stall the async event loop.  Log
//! messages are sent back to the UI via the `log_tx` channel on the
//! [`App`] state.  The primary entry point is [`dispatch`], which
//! examines the selected menu item and spawns the appropriate action.

use anyhow::Result;
use super::state::App;
use tokio::task;
use crate::{gitutils, modpack, arma};

/// Dispatches the selected menu entry.  This function spawns
/// blocking operations on a threadpool so as not to block the async
/// event loop.  Errors returned from background tasks are captured
/// via the log channel; the function itself only returns an error if
/// the spawning of a task fails.
pub async fn dispatch(app: &mut App, idx: usize) -> Result<()> {
    match app.menu.get(idx).copied() {
        Some("Sync Modpack") => {
            app.log("Starting sync...");
            let config = app.config.clone();
            let log_tx = app.log_tx.clone();
            task::spawn_blocking(move || {
                // Ensure any previous repo URL mismatch clears the cache.
                if let Err(e) = config.ensure_repo_cache_for_url() {
                    let _ = log_tx.send(format!("Failed to ensure repo cache: {e}"));
                }

                let repo_path = config.repo_cache_path();
                match gitutils::clone_or_open_repo(&config.repo_url, &repo_path) {
                    Ok(repo) => {
                        let _ = gitutils::fetch(&repo);
                        match modpack::sync_modpack(&repo_path, &config.target_mod_dir) {
                            Ok(()) => {
                                let _ = log_tx.send("Sync complete".to_string());
                            }
                            Err(e) => {
                                let _ = log_tx.send(format!("Sync failed: {e}"));
                            }
                        }
                    }
                    Err(e) => {
                        let _ = log_tx.send(format!("Failed to clone or open repository: {e}"));
                    }
                }
            });
        }
        Some("Validate Files") => {
            app.log("Validating files...");
            let config = app.config.clone();
            let log_tx = app.log_tx.clone();
            task::spawn_blocking(move || {
                let repo_path = config.repo_cache_path();
                match modpack::validate_modpack(&repo_path, &config.target_mod_dir) {
                    Ok(mismatches) => {
                        if mismatches.is_empty() {
                            let _ = log_tx.send("All files are valid".to_string());
                        } else {
                            let msg = format!("{} file(s) need healing", mismatches.len());
                            let _ = log_tx.send(msg);
                            for m in mismatches.iter().take(10) {
                                let _ = log_tx.send(format!("- {}", m.display()));
                            }
                            if mismatches.len() > 10 {
                                let _ = log_tx.send("...".to_string());
                            }
                        }
                    }
                    Err(e) => {
                        let _ = log_tx.send(format!("Validation failed: {e}"));
                    }
                }
            });
        }
        Some("Check Updates") => {
            app.log("Checking for updates...");
            let config = app.config.clone();
            let log_tx = app.log_tx.clone();
            task::spawn_blocking(move || {
                // Ensure the repo cache is valid for the configured URL.
                if let Err(e) = config.ensure_repo_cache_for_url() {
                    let _ = log_tx.send(format!("Failed to ensure repo cache: {e}"));
                }

                let repo_path = config.repo_cache_path();
                match gitutils::clone_or_open_repo(&config.repo_url, &repo_path) {
                    Ok(repo) => {
                        let before = gitutils::head_oid(&repo).ok();
                        let _ = gitutils::fetch(&repo);
                        let after = gitutils::head_oid(&repo).ok();
                        match (before, after) {
                            (Some(b), Some(a)) => {
                                if b != a {
                                    let _ = log_tx.send("Update available".to_string());
                                } else {
                                    let _ = log_tx.send("Up to date".to_string());
                                }
                            }
                            _ => {
                                let _ = log_tx.send("Could not determine update status".to_string());
                            }
                        }
                    }
                    Err(e) => {
                        let _ = log_tx.send(format!("Failed to check updates: {e}"));
                    }
                }
            });
        }
        Some("Join Server") => {
            app.log("Preparing to join server...");
            let config = app.config.clone();
            let log_tx = app.log_tx.clone();
            task::spawn_blocking(move || match config.read_metadata() {
                Ok(Some(meta)) => {
                    let arma_path = config.arma_executable.or_else(arma::detect_arma_path);
                    match arma_path {
                        Some(path) => match arma::launch_arma(&path, &meta) {
                            Ok(()) => {
                                let _ = log_tx.send(format!(
                                    "Launched Arma at {} and connected to {}:{}",
                                    path.display(), meta.address, meta.port
                                ));
                            }
                            Err(e) => {
                                let _ = log_tx.send(format!("Failed to launch Arma: {e}"));
                            }
                        },
                        None => {
                            let _ = log_tx.send("Could not determine Arma executable path".to_string());
                        }
                    }
                }
                Ok(None) => {
                    let _ = log_tx.send("metadata.json not found in repository".to_string());
                }
                Err(e) => {
                    let _ = log_tx.send(format!("Failed to read metadata: {e}"));
                }
            });
        }
        Some("Quit") => {
            // Immediate exit.  Using process::exit here replicates the
            // behaviour of the original implementation.  Dropping out of
            // the event loop would also work but this keeps the user
            // experience consistent with the menu option.
            std::process::exit(0);
        }
        _ => {}
    }
    Ok(())
}