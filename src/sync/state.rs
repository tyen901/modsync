// src/sync/state.rs

// This module could manage the overall state of the synchronization process.
// - Keeping track of the current torrent ID being managed.
// - Storing the last known ETag or other change detection info.
// - Handling timers or triggers for periodic checks.

use crate::config::AppConfig;
use crate::ui::{UiMessage, SyncStatus};
use anyhow::{Context, Result};
use tokio::sync::mpsc;
use std::path::PathBuf;

// Import the cleaner functions
use super::cleaner::{find_extra_files, get_expected_files_from_details};

// Structure to hold the sync state
#[derive(Debug, Default)]
pub struct SyncState {
    current_torrent_id: Option<usize>,
    last_known_etag: Option<String>,
    // Add other state variables as needed
}

// Helper function to send status update to UI
fn send_sync_status(
    tx: &mpsc::UnboundedSender<UiMessage>,
    status: SyncStatus
) {
    if let Err(e) = tx.send(UiMessage::UpdateSyncStatus(status)) {
        eprintln!("Sync: Failed to send status update to UI: {}", e);
    }
}

// Main loop for the synchronization manager task
pub async fn run_sync_manager(
    initial_config: AppConfig, // Start with the initial config
    api: librqbit::Api,
    ui_tx: mpsc::UnboundedSender<UiMessage>,
    mut config_update_rx: mpsc::UnboundedReceiver<AppConfig>, // Added receiver parameter
    mut ui_rx: mpsc::UnboundedReceiver<UiMessage>, // Add receiver for UI messages
) -> Result<()> {
    let mut state = SyncState::default();
    let mut current_config = initial_config;

    // Create HTTP client once
    let http_client = super::http::create_http_client().context("Failed to create HTTP client")?;
    
    // Send initial Idle status
    send_sync_status(&ui_tx, SyncStatus::Idle);
    
    println!("Sync: Manager started. Waiting for manual refresh requests...");

    loop {
        tokio::select! {
            // Handle messages from the UI
            Some(ui_message) = ui_rx.recv() => {
                match ui_message {
                    UiMessage::TriggerManualRefresh => {
                        println!("Sync: Manual refresh requested");
                        
                        // Perform the refresh
                        perform_refresh(&current_config, &mut state, &api, &ui_tx, &http_client).await;
                    },
                    UiMessage::TriggerFolderVerify => {
                        println!("Sync: Folder verification requested");
                        verify_folder_contents(&current_config, &state, &api, &ui_tx).await;
                    },
                    UiMessage::DeleteExtraFiles(files_to_delete) => {
                        println!("Sync: Deletion requested for {} files", files_to_delete.len());
                        delete_files(&files_to_delete, &ui_tx).await;
                    }
                    // Ignore other UI messages - they're meant for the UI thread
                    _ => {}
                }
            }

            // Handle config updates
            Some(new_config) = config_update_rx.recv() => {
                println!("Sync: Received config update: URL='{}', Path='{}'", 
                         new_config.torrent_url, new_config.download_path.display());
                
                // Update the config being used by the sync task
                current_config = new_config;
                
                // Update UI status to reflect config change
                send_sync_status(&ui_tx, SyncStatus::UpdatingTorrent);
                
                // Reset sync state
                println!("Sync: Resetting ETag and current Torrent ID due to config change.");
                state.last_known_etag = None;
                let old_torrent_id = state.current_torrent_id.take(); // Clear current ID

                // Clear the UI display for the old torrent
                if old_torrent_id.is_some() {
                    println!("Sync: Sending None to UI to clear old torrent details.");
                    if let Err(e) = ui_tx.send(UiMessage::UpdateManagedTorrent(None)) {
                        eprintln!("Sync: Failed to send None update after config change: {}", e);
                    }
                }
                
                // Return to idle state
                send_sync_status(&ui_tx, SyncStatus::Idle);
            }
        }
    }
    // Note: This loop runs forever currently. Need graceful shutdown mechanism.
    // Ok(())
}

// Function to verify local folder contents against the current torrent
async fn verify_folder_contents(
    config: &AppConfig,
    state: &SyncState,
    api: &librqbit::Api,
    ui_tx: &mpsc::UnboundedSender<UiMessage>,
) {
    send_sync_status(ui_tx, SyncStatus::CheckingLocal);
    
    if let Some(torrent_id) = state.current_torrent_id {
        match api.api_torrent_details(torrent_id.into()) {
            Ok(details) => {
                let expected_files_rel = get_expected_files_from_details(&details);
                
                // Check download path validity
                if config.download_path.as_os_str().is_empty() {
                     println!("Sync: Cannot verify, download path is empty.");
                     send_sync_status(ui_tx, SyncStatus::Error("Download path not set".to_string()));
                     return;
                }
                
                // Scan the directory
                match find_extra_files(&config.download_path, &expected_files_rel) {
                    Ok(extra_files) => {
                        println!("Sync: Verification complete. Found {} extra files.", extra_files.len());
                        // Send result to UI
                        if let Err(e) = ui_tx.send(UiMessage::ExtraFilesFound(extra_files)) {
                            eprintln!("Sync: Failed to send ExtraFilesFound message: {}", e);
                            send_sync_status(ui_tx, SyncStatus::Error("Failed to send results to UI".to_string()));
                        } else {
                             // Return to Idle only after successfully sending the message
                             send_sync_status(ui_tx, SyncStatus::Idle);
                        }
                    }
                    Err(e) => {
                        let err_msg = format!("Error scanning local files: {}", e);
                        eprintln!("Sync: {}", err_msg);
                        send_sync_status(ui_tx, SyncStatus::Error(err_msg));
                    }
                }
            }
            Err(e) => {
                let err_msg = format!("Failed to get torrent details for verification: {}", e);
                eprintln!("Sync: {}", err_msg);
                send_sync_status(ui_tx, SyncStatus::Error(err_msg));
            }
        }
    } else {
        println!("Sync: No active torrent to verify against.");
        send_sync_status(ui_tx, SyncStatus::Error("No active torrent".to_string()));
        // Optionally send ExtraFilesFound(vec![]) to clear UI prompt if needed
        // let _ = ui_tx.send(UiMessage::ExtraFilesFound(Vec::new()));
    }
}

// Function to delete specified files
async fn delete_files(files_to_delete: &[PathBuf], ui_tx: &mpsc::UnboundedSender<UiMessage>) {
    println!("Sync: Attempting to delete {} files...", files_to_delete.len());
    let mut errors = Vec::new();
    for file_path in files_to_delete {
        println!("Sync: Deleting {}", file_path.display());
        if let Err(e) = tokio::fs::remove_file(file_path).await {
            let err_msg = format!("Failed to delete {}: {}", file_path.display(), e);
            eprintln!("Sync: {}", err_msg);
            errors.push(err_msg);
        } else {
             println!("Sync: Successfully deleted {}", file_path.display());
        }
    }

    if errors.is_empty() {
        println!("Sync: All requested files deleted successfully.");
        // Send a success message? Or just rely on next verification?
        send_sync_status(ui_tx, SyncStatus::Idle); // Return to Idle after deletion
    } else {
        let combined_error = format!("Errors during deletion: {}", errors.join("; "));
        println!("Sync: {}", combined_error);
        let _ = ui_tx.send(UiMessage::Error(combined_error.clone()));
        send_sync_status(ui_tx, SyncStatus::Error("Deletion failed".to_string()));
    }
}

// Extract the refresh logic into its own function for better organization
async fn perform_refresh(
    config: &AppConfig,
    state: &mut SyncState,
    api: &librqbit::Api,
    ui_tx: &mpsc::UnboundedSender<UiMessage>,
    http_client: &reqwest::Client,
) {
    // Check if URL is configured before proceeding
    if config.torrent_url.is_empty() {
        println!("Sync: No remote URL configured, skipping check.");
        // Send idle status to UI
        send_sync_status(ui_tx, SyncStatus::Idle);
        return;
    }

    println!("Sync: Checking for updates...");
    // Send checking status to UI
    send_sync_status(ui_tx, SyncStatus::CheckingRemote);
    
    match super::http::check_remote_torrent(
        &config.torrent_url,
        state.last_known_etag.as_deref(),
        http_client,
    ).await {
        Ok(check_result) => {
            if check_result.needs_update {
                println!("Sync: Remote torrent needs update. ETag: {:?}", check_result.etag);
                // Update UI with updating status
                send_sync_status(ui_tx, SyncStatus::UpdatingTorrent);
                
                // Update state with the new ETag
                state.last_known_etag = check_result.etag;

                // Ensure we have content before proceeding
                if let Some(torrent_content) = check_result.torrent_content {
                    // Call manage_torrent_task with the new content
                    match super::torrent::manage_torrent_task(
                        config,
                        api,
                        ui_tx,
                        state.current_torrent_id, // Pass current ID to forget
                        torrent_content, // Pass fetched content
                    ).await {
                        Ok(new_id) => {
                            // Update state with the new torrent ID
                            println!("Sync: Torrent task managed successfully. New ID: {:?}", new_id);
                            state.current_torrent_id = new_id; // Store the new ID
                            
                            // Refresh torrent details in UI immediately after adding/updating.
                            // The UI should derive its detailed status from this message.
                            if let Some(id) = new_id {
                                refresh_managed_torrent_status(
                                    api, 
                                    ui_tx, 
                                    id
                                );
                            }
                        },
                        Err(e) => {
                            eprintln!("Sync: Error managing torrent task: {}", e);
                            let err_msg = format!("Sync error: {}", e);
                            let _ = ui_tx.send(UiMessage::Error(err_msg.clone()));
                            send_sync_status(ui_tx, SyncStatus::Error(err_msg));
                        }
                    }
                } else {
                    // This case should ideally not happen if needs_update is true based on 200 OK,
                    // but handle defensively.
                    let err_msg = "Sync error: No content received for update".to_string();
                    eprintln!("Sync: Inconsistent state - torrent needs update but no content received.");
                    let _ = ui_tx.send(UiMessage::Error(err_msg.clone()));
                    send_sync_status(ui_tx, SyncStatus::Error(err_msg));
                }
            } else {
                println!("Sync: Remote torrent is up-to-date.");
                // Return to Idle state after check if no update needed
                send_sync_status(ui_tx, SyncStatus::Idle);
            }

            // Refresh current torrent status in UI regardless of update
            if let Some(managed_id) = state.current_torrent_id {
                println!("Sync: Refreshing status for managed torrent ID: {}", managed_id);
                refresh_managed_torrent_status(
                    api, 
                    ui_tx, 
                    managed_id
                );
            } else {
                println!("Sync: No active torrent ID to refresh status for.");
            }
        }
        Err(e) => {
            let err_msg = format!("HTTP check error: {}", e);
            eprintln!("Sync: Error checking remote torrent: {}", e);
            let _ = ui_tx.send(UiMessage::Error(err_msg.clone()));
            send_sync_status(ui_tx, SyncStatus::Error(err_msg));
        }
    }
}

// Helper function within sync::state to get status of the managed torrent
fn refresh_managed_torrent_status(
    api: &librqbit::Api,
    tx: &mpsc::UnboundedSender<UiMessage>,
    managed_id: usize, // Expect a specific ID here
) {
    println!("Sync: Fetching stats for torrent ID {}", managed_id);
    // Use api_stats_v1 to get TorrentStats
    match api.api_stats_v1(managed_id.into()) {
        Ok(stats) => { // stats is the TorrentStats
            
            // Update torrent stats in UI
            if let Err(e) = tx.send(UiMessage::UpdateManagedTorrent(Some((managed_id, stats)))) {
                eprintln!(
                    "Sync: Failed to send managed torrent stats update to UI (ID {}): {}",
                    managed_id, e
                );
            }
        }
        Err(e) => {
            eprintln!(
                "Sync: Error fetching torrent stats for ID {}: {}. Sending None to UI.",
                managed_id, e
            );
            // Send None to clear the UI details on error
            let _ = tx.send(UiMessage::UpdateManagedTorrent(None));
            
            // Send error status update
            let err_msg = format!("Failed to get torrent stats: {}", e);
            send_sync_status(tx, SyncStatus::Error(err_msg.clone()));
            let _ = tx.send(UiMessage::Error(err_msg));
        }
    }
} 