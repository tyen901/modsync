// src/sync/manager.rs

//! Main manager for the synchronization process

use anyhow::{Context, Result};
use std::time::Instant;
use tokio::sync::mpsc;

use crate::sync::status::SyncStatus;
use super::types::SyncConfig;

use super::cleaner::{find_extra_files, get_expected_files_from_details};
use super::local::{delete_files, refresh_managed_torrent_status_event, verify_folder_contents, fix_missing_files};
use super::messages::{SyncCommand, SyncEvent};
use super::remote::{apply_remote_update, direct_download_and_compare};
use super::types::{LocalTorrentState, RemoteTorrentState, SyncState};
use super::utils::send_sync_status_event;

pub async fn run_sync_manager(
    api: librqbit::Api,
    ui_tx: mpsc::UnboundedSender<SyncEvent>,
    mut sync_cmd_rx: mpsc::UnboundedReceiver<SyncCommand>,
    initial_torrent_id: Option<usize>, // Accept initial ID
) -> Result<()> {
    let mut state = SyncState {
        local: match initial_torrent_id {
            Some(id) => LocalTorrentState::Active { id },
            None => LocalTorrentState::NotLoaded,
        },
        remote: RemoteTorrentState::Unknown,
    };

    // Create HTTP client once
    let http_client = super::http::create_http_client().context("Failed to create HTTP client")?;
    
    // Track the last time we checked for updates
    let mut last_update_check: Option<std::time::Instant> = None;

    // Send initial status based on whether a cached torrent was loaded
    if let LocalTorrentState::Active { id } = state.local {
        // If we started with a cached torrent, immediately check its status
        println!(
            "Sync: Refreshing status for initially loaded torrent ID: {}",
            id
        );
        refresh_managed_torrent_status_event(&api, &ui_tx, id);
        // Set overall sync status to Idle, actual torrent status comes from refresh
        send_sync_status_event(&ui_tx, SyncStatus::Idle);
    } else {
        send_sync_status_event(&ui_tx, SyncStatus::Idle);
    }

    println!("Sync: Manager started. Initial State: {:?}", state);

    loop {
        tokio::select! {
            // Handle command messages from the UI
            Some(cmd_message) = sync_cmd_rx.recv() => {
                match cmd_message {
                    SyncCommand::UpdateConfig(_new_config) => {
                            // Configuration updates are now handled via the external
                            // configuration API. The manager no longer stores or
                            // mutates a local config copy. For now just acknowledge
                            // receipt and notify the UI; the real apply will come
                            // from the configuration API when implemented.
                            println!("Sync: Received configuration update (forwarded to config API)");
                            let _ = ui_tx.send(SyncEvent::Error("Configuration update received; it will be applied via the config API.".to_string()));
                    }
                    SyncCommand::VerifyFolder => {
                        println!("Sync: Folder verification requested");
                            // Use a placeholder config; the real config will be
                            // supplied by the configuration API in future.
                            let cfg = SyncConfig::default();
                            verify_folder_contents(&cfg, &mut state, &api, &ui_tx).await;
                    },
                    SyncCommand::FixMissingFiles => {
                        println!("Sync: Fix missing files requested");
                            let cfg = SyncConfig::default();
                            fix_missing_files(&cfg, &mut state, &api, &ui_tx).await;
                    },
                    SyncCommand::DeleteFiles(files_to_delete) => {
                        println!("Sync: Deletion requested for {} files", files_to_delete.len());
                        delete_files(&files_to_delete, &ui_tx).await;
                    },
                    SyncCommand::ApplyUpdate(torrent_content) => {
                        println!("Sync: Apply remote update requested ({} bytes)", torrent_content.len());
                            let cfg = SyncConfig::default();

                            match apply_remote_update(&cfg, &mut state, &api, &ui_tx, torrent_content).await {
                            true => {
                                state.remote = RemoteTorrentState::Checked; // Update state on success
                                
                                // Verification logic after successful update
                                if let LocalTorrentState::Active { id } = state.local {
                                    println!("Sync: Checking for extra files after update");
                                    send_sync_status_event(&ui_tx, SyncStatus::CheckingLocal);
                                    match api.api_torrent_details(id.into()) {
                                        Ok(details) => {
                                            let expected_files = get_expected_files_from_details(&details);
                                            match find_extra_files(&cfg.download_path, &expected_files) {
                                                Ok(extra_files) => {
                                                    println!("Sync: Found {} extra files after update", extra_files.len());
                                                    if let Err(e) = ui_tx.send(SyncEvent::ExtraFilesFound(extra_files)) {
                                                        eprintln!("Sync: Failed to send extra files list to UI: {}", e);
                                                    }
                                                    send_sync_status_event(&ui_tx, SyncStatus::Idle);
                                                },
                                                Err(e) => {
                                                    let err_msg = format!("Failed to find extra files after update: {}", e);
                                                    eprintln!("Sync: {}", err_msg);
                                                    let _ = ui_tx.send(SyncEvent::Error(err_msg.clone()));
                                                    send_sync_status_event(&ui_tx, SyncStatus::Error(err_msg));
                                                }
                                            }
                                        },
                                        Err(e) => {
                                            let err_msg = format!("Failed to get torrent details after update: {}", e);
                                            eprintln!("Sync: {}", err_msg);
                                            let _ = ui_tx.send(SyncEvent::Error(err_msg.clone()));
                                            send_sync_status_event(&ui_tx, SyncStatus::Error(err_msg));
                                        }
                                    }
                                } else {
                                     send_sync_status_event(&ui_tx, SyncStatus::Idle); // No active torrent to verify against
                                }
                            },
                            false => {
                                // If update failed, set remote state back to Unknown
                                // Maybe also set local state to NotLoaded if appropriate?
                                state.remote = RemoteTorrentState::Unknown;
                                // Error status is sent by apply_remote_update itself
                            }
                        }
                    },
                    SyncCommand::DownloadAndCompare(url) => {
                        println!("Sync: Force download and compare requested for URL: {}", url);
                        let mut cfg = SyncConfig::default();
                        cfg.torrent_url = url.clone();
                        direct_download_and_compare(&cfg, &mut state, &api, &ui_tx, &http_client).await;
                    },
                    // No need for a catch-all since all variants are explicitly handled
                }
            },
            // Define a timeout to periodically refresh the status
            _ = tokio::time::sleep(std::time::Duration::from_secs(10)) => {
                // Refresh the torrent status periodically
                if let LocalTorrentState::Active { id } = state.local {
                    refresh_managed_torrent_status_event(&api, &ui_tx, id);

                    // Every 10 minutes, also check for remote updates
                    let now = Instant::now();
                    let should_check = match last_update_check {
                        Some(last) => now.duration_since(last).as_secs() >= 600, // 10 minutes
                        None => true
                    };

                    if should_check {
                        last_update_check = Some(now);
                        println!("Sync: Periodic remote check triggered");
                        // No local config is stored in the manager; use default
                        // placeholder until the external config API is available.
                        let periodic_cfg = SyncConfig::default();
                        direct_download_and_compare(&periodic_cfg, &mut state, &api, &ui_tx, &http_client).await;
                    }
                }
            }
        }
    }
}