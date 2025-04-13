// src/sync/local.rs

//! Operations related to the local torrent state

use crate::config::AppConfig;
use crate::ui::SyncStatus;
use librqbit::TorrentStatsState;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::mpsc;

use super::cleaner::{find_extra_files, get_expected_files_from_details};
use super::messages::SyncEvent;
use super::types::{LocalTorrentState, SyncState};
use super::utils::send_sync_status_event;

/// Function to verify local folder contents
pub async fn verify_folder_contents(
    config: &AppConfig,
    state: &SyncState,
    api: &librqbit::Api,
    ui_tx: &mpsc::UnboundedSender<SyncEvent>,
) {
    // Only proceed if we have an active torrent
    if let LocalTorrentState::Active { id } = state.local {
        if config.download_path.as_os_str().is_empty() {
            let err_msg = "Download path not configured".to_string();
            eprintln!("Sync: {}", err_msg);
            let _ = ui_tx.send(SyncEvent::Error(err_msg.clone()));
            send_sync_status_event(ui_tx, SyncStatus::Error(err_msg));
            return;
        }

        println!(
            "Sync: Verifying folder contents at {}",
            config.download_path.display()
        );
        send_sync_status_event(ui_tx, SyncStatus::CheckingLocal);

        match api.api_torrent_details(id.into()) {
            Ok(details) => {
                // Get the expected files list from torrent
                let expected_files = get_expected_files_from_details(&details);

                // Check for extra files in the download directory
                match find_extra_files(&config.download_path, &expected_files) {
                    Ok(extra_files) => {
                        println!("Sync: Found {} extra files in directory", extra_files.len());
                        // Notify UI of extra files for potential deletion
                        if let Err(e) = ui_tx.send(SyncEvent::ExtraFilesFound(extra_files)) {
                            eprintln!("Sync: Failed to send extra files list to UI: {}", e);
                        }
                        send_sync_status_event(ui_tx, SyncStatus::Idle);
                    }
                    Err(e) => {
                        let err_msg = format!("Failed to find extra files: {}", e);
                        eprintln!("Sync: {}", err_msg);
                        let _ = ui_tx.send(SyncEvent::Error(err_msg.clone()));
                        send_sync_status_event(ui_tx, SyncStatus::Error(err_msg));
                    }
                }
            }
            Err(e) => {
                let err_msg = format!("Failed to get torrent details: {}", e);
                eprintln!("Sync: {}", err_msg);
                let _ = ui_tx.send(SyncEvent::Error(err_msg.clone()));
                send_sync_status_event(ui_tx, SyncStatus::Error(err_msg));
            }
        }
    } else {
        let err_msg = "No active torrent to verify against".to_string();
        eprintln!("Sync: {}", err_msg);
        let _ = ui_tx.send(SyncEvent::Error(err_msg.clone()));
        send_sync_status_event(ui_tx, SyncStatus::Error(err_msg));
    }
}

/// Function to delete extra files
pub async fn delete_files(files_to_delete: &[PathBuf], ui_tx: &mpsc::UnboundedSender<SyncEvent>) {
    println!("Sync: Deleting {} files", files_to_delete.len());
    send_sync_status_event(ui_tx, SyncStatus::CheckingLocal); // Re-use the CheckingLocal status

    let mut errors = Vec::new();

    for file_path in files_to_delete {
        println!("Sync: Deleting file: {}", file_path.display());
        if let Err(e) = tokio::fs::remove_file(file_path).await {
            let err_msg = format!("Failed to delete {}: {}", file_path.display(), e);
            eprintln!("Sync: {}", err_msg);
            errors.push(err_msg);
        }
    }

    if !errors.is_empty() {
        let err_msg = format!("Errors during file deletion: {}", errors.join(", "));
        let _ = ui_tx.send(SyncEvent::Error(err_msg.clone()));
        send_sync_status_event(ui_tx, SyncStatus::Error(err_msg));
    } else {
        println!("Sync: All files deleted successfully");
        // Clear any existing error and set status back to idle
        send_sync_status_event(ui_tx, SyncStatus::Idle);
    }

    // Let UI know that deletion is complete (empty list = no more files to delete)
    if let Err(e) = ui_tx.send(SyncEvent::ExtraFilesFound(Vec::new())) {
        eprintln!("Sync: Failed to send empty extra files list to UI: {}", e);
    }
}

/// Helper function to refresh the status of the managed torrent
pub fn refresh_managed_torrent_status_event(
    api: &librqbit::Api,
    tx: &mpsc::UnboundedSender<SyncEvent>,
    managed_id: usize,
) {
    println!("Sync: Fetching stats for torrent ID {}", managed_id);
    match api.api_stats_v1(managed_id.into()) {
        Ok(stats) => {
            // Send the torrent stats update - wrap in Arc
            if let Err(e) = tx.send(SyncEvent::ManagedTorrentUpdate(Some((managed_id, Arc::new(stats))))) {
                eprintln!(
                    "Sync: Failed to send managed torrent stats update to UI (ID {}): {}",
                    managed_id, e
                );
                return;
            }

            // Attempt to get a cloned copy of stats for our own use
            if let Ok(refreshed_stats) = api.api_stats_v1(managed_id.into()) {
                // Update the overall sync status to reflect that we have an active local torrent
                // Only do this if the torrent is in a "normal" state (not checking, etc.)
                match refreshed_stats.state {
                    TorrentStatsState::Initializing => {
                        // Torrent is still checking files
                        send_sync_status_event(tx, SyncStatus::CheckingLocal);
                    }
                    TorrentStatsState::Live => {
                        // Torrent is active (downloading or seeding)
                        send_sync_status_event(tx, SyncStatus::LocalActive);
                    }
                    TorrentStatsState::Paused => {
                        // Torrent is paused but still loaded
                        send_sync_status_event(tx, SyncStatus::LocalActive);
                    }
                    TorrentStatsState::Error => {
                        // Torrent has an error
                        let err_msg = refreshed_stats
                            .error
                            .unwrap_or_else(|| "Unknown error".to_string());
                        send_sync_status_event(tx, SyncStatus::Error(err_msg.clone()));
                        let _ = tx.send(SyncEvent::Error(err_msg));
                    }
                }
            }
        }
        Err(e) => {
            eprintln!(
                "Sync: Error fetching torrent stats for ID {}: {}. Sending None to UI.",
                managed_id, e
            );
            let _ = tx.send(SyncEvent::ManagedTorrentUpdate(None));

            let err_msg = format!("Failed to get torrent stats: {}", e);
            send_sync_status_event(tx, SyncStatus::Error(err_msg.clone()));
            let _ = tx.send(SyncEvent::Error(err_msg));
        }
    }
} 