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

use crate::config::get_cached_torrent_path; // Import cache path helper

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
    initial_config: AppConfig,
    api: librqbit::Api,
    ui_tx: mpsc::UnboundedSender<UiMessage>,
    mut config_update_rx: mpsc::UnboundedReceiver<AppConfig>,
    mut ui_rx: mpsc::UnboundedReceiver<UiMessage>,
    initial_torrent_id: Option<usize>, // Accept initial ID
) -> Result<()> {
    let mut state = SyncState {
        current_torrent_id: initial_torrent_id, // Initialize with the ID
        ..Default::default()
    };
    let mut current_config = initial_config;

    // Create HTTP client once
    let http_client = super::http::create_http_client().context("Failed to create HTTP client")?;
    
    // Send initial status based on whether a cached torrent was loaded
    if let Some(id) = state.current_torrent_id {
        // If we started with a cached torrent, immediately check its status
        println!("Sync: Refreshing status for initially loaded torrent ID: {}", id);
        refresh_managed_torrent_status(&api, &ui_tx, id);
        // Set overall sync status to Idle, actual torrent status comes from refresh
        send_sync_status(&ui_tx, SyncStatus::Idle); 
    } else {
        send_sync_status(&ui_tx, SyncStatus::Idle);
    }
    
    println!("Sync: Manager started. Initial Torrent ID: {:?}", state.current_torrent_id);

    loop {
        tokio::select! {
            // Handle messages from the UI
            Some(ui_message) = ui_rx.recv() => {
                match ui_message {
                    UiMessage::TriggerManualRefresh => {
                        println!("Sync: Manual refresh requested");
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
                    _ => {}
                }
            }

            // Handle config updates
            Some(new_config) = config_update_rx.recv() => {
                println!("Sync: Received config update: URL='{}', Path='{}'", 
                         new_config.torrent_url, new_config.download_path.display());
                
                current_config = new_config;
                
                send_sync_status(&ui_tx, SyncStatus::UpdatingTorrent); // Indicate change
                
                println!("Sync: Resetting ETag and current Torrent ID due to config change.");
                state.last_known_etag = None;
                let old_torrent_id = state.current_torrent_id.take();

                // Clear the UI display for the old torrent if it existed
                if old_torrent_id.is_some() {
                    println!("Sync: Sending None to UI to clear old torrent details.");
                    let _ = ui_tx.send(UiMessage::UpdateManagedTorrent(None));
                }
                
                // Explicitly trigger a refresh check after config change
                println!("Sync: Triggering refresh after config update.");
                perform_refresh(&current_config, &mut state, &api, &ui_tx, &http_client).await;
            }
        }
    }
}

// Extracted refresh logic
async fn perform_refresh(
    config: &AppConfig,
    state: &mut SyncState,
    api: &librqbit::Api,
    ui_tx: &mpsc::UnboundedSender<UiMessage>,
    http_client: &reqwest::Client,
) {
    if config.torrent_url.is_empty() {
        println!("Sync: No remote URL configured, skipping check.");
        send_sync_status(ui_tx, SyncStatus::Idle);
        // Ensure UI is cleared if there was an old torrent ID
        if state.current_torrent_id.is_some() {
            let _ = ui_tx.send(UiMessage::UpdateManagedTorrent(None));
            state.current_torrent_id = None; // Clear the ID as well
        }
        return;
    }

    println!("Sync: Checking for updates at {}...", config.torrent_url);
    send_sync_status(ui_tx, SyncStatus::CheckingRemote);
    
    match super::http::check_remote_torrent(
        &config.torrent_url,
        state.last_known_etag.as_deref(),
        http_client,
    ).await {
        Ok(check_result) => {
            if check_result.needs_update {
                println!("Sync: Remote torrent needs update. ETag: {:?}", check_result.etag);
                send_sync_status(ui_tx, SyncStatus::UpdatingTorrent);
                
                state.last_known_etag = check_result.etag;

                if let Some(torrent_content) = check_result.torrent_content {
                    // --- Save the new torrent content to cache --- 
                    match get_cached_torrent_path() {
                        Ok(cache_path) => {
                            println!("Sync: Saving updated torrent to cache: {}", cache_path.display());
                            if let Err(e) = tokio::fs::write(&cache_path, &torrent_content).await {
                                eprintln!("Sync: WARNING - Failed to write to cache file {}: {}", cache_path.display(), e);
                            }
                        }
                        Err(e) => {
                             eprintln!("Sync: WARNING - Failed get cache path: {}", e);
                        }
                    }
                    // -------------------------------------------
                    
                    match super::torrent::manage_torrent_task(
                        config,
                        api,
                        ui_tx,
                        state.current_torrent_id, // Pass current ID to forget
                        torrent_content, 
                    ).await {
                        Ok(new_id) => {
                            println!("Sync: Torrent task managed successfully. New ID: {:?}", new_id);
                            state.current_torrent_id = new_id;
                                                        
                            if let Some(id) = new_id {
                                refresh_managed_torrent_status(api, ui_tx, id);
                            }
                            // Let status be updated by refresh or next cycle
                        },
                        Err(e) => {
                            let err_msg = format!("Sync error managing torrent: {}", e);
                            eprintln!("Sync: {}", err_msg);
                            let _ = ui_tx.send(UiMessage::Error(err_msg.clone()));
                            send_sync_status(ui_tx, SyncStatus::Error(err_msg));
                        }
                    }
                } else {
                    let err_msg = "Sync error: Update needed but no content received".to_string();
                    eprintln!("Sync: {}", err_msg);
                    let _ = ui_tx.send(UiMessage::Error(err_msg.clone()));
                    send_sync_status(ui_tx, SyncStatus::Error(err_msg));
                }
            } else {
                println!("Sync: Remote torrent is up-to-date.");
                send_sync_status(ui_tx, SyncStatus::Idle);
                // Torrent is up-to-date. If we started with a cached torrent,
                // ensure its status is refreshed.
                if let Some(id) = state.current_torrent_id {
                     println!("Sync: Refreshing status for up-to-date torrent ID: {}", id);
                     refresh_managed_torrent_status(api, ui_tx, id);
                     // TODO: Consider unpausing the torrent here if needed.
                } else {
                    // This could happen if URL is set but first fetch failed, and now it's up-to-date (304).
                    // Or if cache was deleted. We should probably try to add it now if config is valid.
                    println!("Sync: Up-to-date but no active torrent ID. Attempting initial add if possible...");
                    // This path needs careful consideration - potentially fetch torrent content here?
                    // For now, just stay Idle.
                }
            }
        }
        Err(e) => {
            let err_msg = format!("HTTP check error: {}", e);
            eprintln!("Sync: {}", err_msg);
            let _ = ui_tx.send(UiMessage::Error(err_msg.clone()));
            send_sync_status(ui_tx, SyncStatus::Error(err_msg));
        }
    }
}

// Function to verify local folder contents
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
                
                if config.download_path.as_os_str().is_empty() {
                     println!("Sync: Cannot verify, download path is empty.");
                     send_sync_status(ui_tx, SyncStatus::Error("Download path not set".to_string()));
                     return;
                }
                
                match find_extra_files(&config.download_path, &expected_files_rel) {
                    Ok(extra_files) => {
                        println!("Sync: Verification complete. Found {} extra files.", extra_files.len());
                        if let Err(e) = ui_tx.send(UiMessage::ExtraFilesFound(extra_files)) {
                            eprintln!("Sync: Failed to send ExtraFilesFound message: {}", e);
                            send_sync_status(ui_tx, SyncStatus::Error("Failed to send results to UI".to_string()));
                        } else {
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
        send_sync_status(ui_tx, SyncStatus::Idle);
    } else {
        let combined_error = format!("Errors during deletion: {}", errors.join("; "));
        println!("Sync: {}", combined_error);
        let _ = ui_tx.send(UiMessage::Error(combined_error.clone()));
        send_sync_status(ui_tx, SyncStatus::Error("Deletion failed".to_string()));
    }
}

// Refresh status helper function
fn refresh_managed_torrent_status(
    api: &librqbit::Api,
    tx: &mpsc::UnboundedSender<UiMessage>,
    managed_id: usize,
) {
    println!("Sync: Fetching stats for torrent ID {}", managed_id);
    match api.api_stats_v1(managed_id.into()) {
        Ok(stats) => { 
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
            let _ = tx.send(UiMessage::UpdateManagedTorrent(None));
            
            let err_msg = format!("Failed to get torrent stats: {}", e);
            send_sync_status(tx, SyncStatus::Error(err_msg.clone()));
            let _ = tx.send(UiMessage::Error(err_msg));
        }
    }
} 