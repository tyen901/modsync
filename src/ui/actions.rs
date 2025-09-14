//! Long‑running actions triggered from the UI.
//!
//! The functions in this module encapsulate the logic for each menu
//! action. They perform blocking operations on a background thread
//! so as not to stall the async event loop.  Progress is reported via
//! `TaskUpdate` messages sent on the app's task channel. The primary
//! entry point is [`dispatch`], which examines the selected menu item
//! and spawns the appropriate action.

use super::state::App;
use super::state::TaskUpdate;
use crate::{arma, gitutils, modpack};
use anyhow::Result;
use tokio::task;

/// Dispatches the selected menu entry.  This function spawns
/// blocking operations on a threadpool so as not to block the async
/// event loop.  Errors returned from background tasks are captured
/// via the log channel; the function itself only returns an error if
/// the spawning of a task fails.
pub async fn dispatch(app: &mut App, idx: usize) -> Result<()> {
    match app.menu.get(idx).copied() {
        Some("Sync Modpack") => {
            let config = app.config.clone();
            let task_tx = app.task_tx.clone();
            // Define stages for sync.
            let stages = vec![
                "Prepare repo cache".to_string(),
                "Clone/Fetch repository".to_string(),
                "Sync files".to_string(),
            ];
            // Notify UI a task is starting.
            let _ = task_tx.send(TaskUpdate::Start {
                name: "Sync Modpack".to_string(),
                stages: stages.clone(),
            });
            task::spawn_blocking(move || {
                // Stage 0: prepare
                let _ = task_tx.send(TaskUpdate::StageStarted(0));
                if let Err(e) = config.ensure_repo_cache_for_url() {
                    let _ = task_tx.send(TaskUpdate::StageFailed(0, format!("{e}")));
                    let _ = task_tx.send(TaskUpdate::Aborted);
                    return;
                }
                let _ = task_tx.send(TaskUpdate::StageCompleted(0));

                // Stage 1: clone/fetch
                let _ = task_tx.send(TaskUpdate::StageStarted(1));
                let repo_path = config.repo_cache_path();
                match gitutils::clone_or_open_repo(&config.repo_url, &repo_path) {
                    Ok(repo) => {
                        let _ = gitutils::fetch(&repo);
                        let _ = task_tx.send(TaskUpdate::StageCompleted(1));
                        // Stage 2: sync files
                        let _ = task_tx.send(TaskUpdate::StageStarted(2));
                        match modpack::sync_modpack(&repo_path, &config.target_mod_dir) {
                            Ok(()) => {
                                let _ = task_tx.send(TaskUpdate::StageCompleted(2));
                                // Send finished state lines (simple example).
                                let state_lines = vec!["Sync: OK".to_string()];
                                let _ = task_tx.send(TaskUpdate::Finished(state_lines));
                            }
                            Err(e) => {
                                let _ = task_tx.send(TaskUpdate::StageFailed(2, format!("{e}")));
                                let _ = task_tx.send(TaskUpdate::Aborted);
                            }
                        }
                    }
                    Err(e) => {
                        let _ = task_tx.send(TaskUpdate::StageFailed(1, format!("{e}")));
                        let _ = task_tx.send(TaskUpdate::Aborted);
                    }
                }
            });
        }
        Some("Validate Files") => {
            let config = app.config.clone();
            let task_tx = app.task_tx.clone();
            let stages = vec![
                "Prepare".to_string(),
                "Run validation".to_string(),
                "Report".to_string(),
            ];
            let _ = task_tx.send(TaskUpdate::Start {
                name: "Validate Files".to_string(),
                stages: stages.clone(),
            });
            task::spawn_blocking(move || {
                let _ = task_tx.send(TaskUpdate::StageStarted(0));
                let repo_path = config.repo_cache_path();
                let _ = task_tx.send(TaskUpdate::StageCompleted(0));
                let _ = task_tx.send(TaskUpdate::StageStarted(1));
                match modpack::validate_modpack(&repo_path, &config.target_mod_dir) {
                    Ok(mismatches) => {
                        let mut state_lines = Vec::new();
                        if mismatches.is_empty() {
                            state_lines.push("All files valid".to_string());
                            state_lines.push("All files valid".to_string());
                        } else {
                            let msg = format!("{} file(s) need healing", mismatches.len());
                            state_lines.push(msg.clone());
                            for m in mismatches.iter().take(10) {
                                let line = format!("- {}", m.display());
                                state_lines.push(line);
                            }
                            if mismatches.len() > 10 {
                                state_lines.push("...".to_string());
                            }
                        }
                        let _ = task_tx.send(TaskUpdate::StageCompleted(1));
                        let _ = task_tx.send(TaskUpdate::StageCompleted(2));
                        let _ = task_tx.send(TaskUpdate::Finished(state_lines));
                    }
                    Err(e) => {
                        let _ = task_tx.send(TaskUpdate::StageFailed(1, format!("{e}")));
                        let _ = task_tx.send(TaskUpdate::Aborted);
                    }
                }
            });
        }
        Some("Check Updates") => {
            let config = app.config.clone();
            let task_tx = app.task_tx.clone();
            let stages = vec![
                "Prepare".to_string(),
                "Fetch".to_string(),
                "Compare heads".to_string(),
            ];
            let _ = task_tx.send(TaskUpdate::Start {
                name: "Check Updates".to_string(),
                stages: stages.clone(),
            });
            task::spawn_blocking(move || {
                let _ = task_tx.send(TaskUpdate::StageStarted(0));
                if let Err(e) = config.ensure_repo_cache_for_url() {
                    let _ = task_tx.send(TaskUpdate::StageFailed(0, format!("{e}")));
                    let _ = task_tx.send(TaskUpdate::StageFailed(0, format!("{e}")));
                    let _ = task_tx.send(TaskUpdate::Aborted);
                    return;
                }
                let _ = task_tx.send(TaskUpdate::StageCompleted(0));

                let _ = task_tx.send(TaskUpdate::StageStarted(1));
                let repo_path = config.repo_cache_path();
                match gitutils::clone_or_open_repo(&config.repo_url, &repo_path) {
                    Ok(repo) => {
                        let before = gitutils::head_oid(&repo).ok();
                        let _ = gitutils::fetch(&repo);
                        let after = gitutils::head_oid(&repo).ok();
                        let mut state_lines = Vec::new();
                        match (before, after) {
                            (Some(b), Some(a)) => {
                                if b != a {
                                    state_lines.push("Update available".to_string());
                                } else {
                                    state_lines.push("Up to date".to_string());
                                }
                            }
                            _ => {
                                state_lines.push("Could not determine update status".to_string());
                            }
                        }
                        let _ = task_tx.send(TaskUpdate::StageCompleted(1));
                        let _ = task_tx.send(TaskUpdate::Finished(state_lines));
                    }
                    Err(e) => {
                        let _ = task_tx.send(TaskUpdate::StageFailed(1, format!("{e}")));
                        let _ = task_tx.send(TaskUpdate::Aborted);
                    }
                }
            });
        }
        Some("Join Server") => {
            let config = app.config.clone();
            let task_tx = app.task_tx.clone();
            let stages = vec![
                "Read metadata".to_string(),
                "Find Arma".to_string(),
                "Launch".to_string(),
            ];
            let _ = task_tx.send(TaskUpdate::Start {
                name: "Join Server".to_string(),
                stages: stages.clone(),
            });
            task::spawn_blocking(move || match config.read_metadata() {
                Ok(Some(meta)) => {
                    let _ = task_tx.send(TaskUpdate::StageStarted(0));
                    let arma_path = config.arma_executable.or_else(arma::detect_arma_path);
                    let _ = task_tx.send(TaskUpdate::StageCompleted(0));
                    match arma_path {
                        Some(path) => {
                            let _ = task_tx.send(TaskUpdate::StageStarted(1));
                            match arma::launch_arma(&path, &meta) {
                                Ok(()) => {
                                    let _ = task_tx.send(TaskUpdate::StageCompleted(1));
                                    let _ = task_tx.send(TaskUpdate::Finished(vec![format!(
                                        "Launched {}",
                                        path.display()
                                    )]));
                                }
                                Err(_e) => {
                                    let _ =
                                        task_tx.send(TaskUpdate::StageFailed(1, format!("{_e}")));
                                    let _ = task_tx.send(TaskUpdate::Aborted);
                                }
                            }
                        }
                        None => {
                            let _ = task_tx.send(TaskUpdate::StageFailed(
                                1,
                                "Could not determine Arma executable path".to_string(),
                            ));
                            let _ = task_tx.send(TaskUpdate::Aborted);
                        }
                    }
                }
                Ok(None) => {
                    let _ = task_tx.send(TaskUpdate::Aborted);
                }
                Err(_e) => {
                    let _ = task_tx.send(TaskUpdate::Aborted);
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
