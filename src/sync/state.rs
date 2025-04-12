// src/sync/state.rs

// This module manages the overall state of the synchronization process.

use crate::config::AppConfig;
use crate::ui::{SyncStatus, UiMessage};
use anyhow::{Context, Result, anyhow};
use librqbit::TorrentStatsState; // Add import for TorrentStatsState
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use tokio::sync::mpsc; // Add for hashing

use crate::config::get_cached_torrent_path; // Import cache path helper

// Import the cleaner functions
use super::cleaner::{find_extra_files, get_expected_files_from_details};

// Enum to represent the local torrent state
#[derive(Debug)]
pub enum LocalTorrentState {
    NotLoaded, // No torrent loaded locally
    Active {
        // Torrent is active and being managed
        id: usize,
        // Other local state attributes can be added here
    },
}

// Enum to represent the remote torrent state
#[derive(Debug)]
pub enum RemoteTorrentState {
    Unknown, // State unknown (first run or error)
    Checked,
    UpdateAvailable {
        // Remote has an update that can be applied
        content: Vec<u8>,
    },
}

// Structure to hold the sync state with clear separation
#[derive(Debug)]
pub struct SyncState {
    local: LocalTorrentState,
    remote: RemoteTorrentState,
}

impl Default for SyncState {
    fn default() -> Self {
        SyncState {
            local: LocalTorrentState::NotLoaded,
            remote: RemoteTorrentState::Unknown,
        }
    }
}

// Helper function to send status update to UI
fn send_sync_status(tx: &mpsc::UnboundedSender<UiMessage>, status: SyncStatus) {
    if let Err(e) = tx.send(UiMessage::UpdateSyncStatus(status)) {
        eprintln!("Sync: Failed to send status update to UI: {}", e);
    }
}

// Main loop for the synchronization manager task
pub async fn run_sync_manager(
    initial_config: AppConfig,
    api: librqbit::Api,
    ui_tx: mpsc::UnboundedSender<UiMessage>,
    mut sync_cmd_rx: mpsc::UnboundedReceiver<UiMessage>,
    initial_torrent_id: Option<usize>, // Accept initial ID
) -> Result<()> {
    let mut state = SyncState {
        local: match initial_torrent_id {
            Some(id) => LocalTorrentState::Active { id },
            None => LocalTorrentState::NotLoaded,
        },
        remote: RemoteTorrentState::Unknown,
    };
    let mut current_config = initial_config;

    // Create HTTP client once
    let http_client = super::http::create_http_client().context("Failed to create HTTP client")?;

    // Send initial status based on whether a cached torrent was loaded
    if let LocalTorrentState::Active { id } = state.local {
        // If we started with a cached torrent, immediately check its status
        println!(
            "Sync: Refreshing status for initially loaded torrent ID: {}",
            id
        );
        refresh_managed_torrent_status(&api, &ui_tx, id);
        // Set overall sync status to Idle, actual torrent status comes from refresh
        send_sync_status(&ui_tx, SyncStatus::Idle);
    } else {
        send_sync_status(&ui_tx, SyncStatus::Idle);
    }

    println!("Sync: Manager started. Initial State: {:?}", state);

    loop {
        tokio::select! {
            // Handle command messages from the UI
            Some(cmd_message) = sync_cmd_rx.recv() => {
                match cmd_message {
                    UiMessage::UpdateConfig(new_config) => {
                        println!("Sync: Received configuration update.");
                        current_config = new_config;
                        // Potentially trigger a re-check or other action based on config change
                        // For now, just update the internal state.
                    }
                    UiMessage::TriggerFolderVerify => {
                        println!("Sync: Folder verification requested");
                        verify_folder_contents(&current_config, &state, &api, &ui_tx).await;
                    },
                    UiMessage::DeleteExtraFiles(files_to_delete) => {
                        println!("Sync: Deletion requested for {} files", files_to_delete.len());
                        delete_files(&files_to_delete, &ui_tx).await;
                    },
                    UiMessage::ApplyRemoteUpdate(torrent_content) => {
                        println!("Sync: Apply remote update requested ({} bytes)", torrent_content.len());
                        
                        match apply_remote_update(&current_config, &mut state, &api, &ui_tx, torrent_content).await {
                            true => {
                                state.remote = RemoteTorrentState::Checked; // Update state on success
                                
                                // Verification logic after successful update
                                if let LocalTorrentState::Active { id } = state.local {
                                    println!("Sync: Checking for extra files after update");
                                    send_sync_status(&ui_tx, SyncStatus::CheckingLocal);
                                    // ... (rest of verification logic remains the same)
                                    match api.api_torrent_details(id.into()) {
                                        Ok(details) => {
                                            let expected_files = get_expected_files_from_details(&details);
                                            match find_extra_files(&current_config.download_path, &expected_files) {
                                                Ok(extra_files) => {
                                                    println!("Sync: Found {} extra files after update", extra_files.len());
                                                    if let Err(e) = ui_tx.send(UiMessage::ExtraFilesFound(extra_files)) {
                                                        eprintln!("Sync: Failed to send extra files list to UI: {}", e);
                                                    }
                                                    send_sync_status(&ui_tx, SyncStatus::Idle);
                                                },
                                                Err(e) => {
                                                    let err_msg = format!("Failed to find extra files after update: {}", e);
                                                    eprintln!("Sync: {}", err_msg);
                                                    let _ = ui_tx.send(UiMessage::Error(err_msg.clone()));
                                                    send_sync_status(&ui_tx, SyncStatus::Error(err_msg));
                                                }
                                            }
                                        },
                                        Err(e) => {
                                            let err_msg = format!("Failed to get torrent details after update: {}", e);
                                            eprintln!("Sync: {}", err_msg);
                                            let _ = ui_tx.send(UiMessage::Error(err_msg.clone()));
                                            send_sync_status(&ui_tx, SyncStatus::Error(err_msg));
                                        }
                                    }
                                } else {
                                     send_sync_status(&ui_tx, SyncStatus::Idle); // No active torrent to verify against
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
                    UiMessage::ForceDownloadAndCompare(url) => {
                        println!("Sync: Force download and compare requested for URL: {}", url);
                        current_config.torrent_url = url.clone(); // Update config internally
                        direct_download_and_compare(&current_config, &mut state, &api, &ui_tx, &http_client).await;
                    },
                    // Explicitly ignore messages intended for the UI (shouldn't arrive here)
                    UiMessage::UpdateManagedTorrent(_) |
                    UiMessage::TorrentAdded(_) |
                    UiMessage::Error(_) |
                    UiMessage::UpdateSyncStatus(_) |
                    UiMessage::ExtraFilesFound(_) |
                    UiMessage::RemoteUpdateFound(_) => {
                         eprintln!("Sync: Received unexpected UI update message: {:?}. Ignoring.", cmd_message);
                    }
                }
            },
            // Define a timeout to periodically refresh the status
            _ = tokio::time::sleep(std::time::Duration::from_secs(10)) => {
                // Refresh the torrent status periodically
                if let LocalTorrentState::Active { id } = state.local {
                    refresh_managed_torrent_status(&api, &ui_tx, id);

                    // Every 10 minutes, also check for remote updates
                    static mut LAST_UPDATE_CHECK: Option<std::time::Instant> = None;
                    let now = std::time::Instant::now();
                    let should_check = unsafe {
                        match LAST_UPDATE_CHECK {
                            Some(last) => now.duration_since(last).as_secs() >= 600, // 10 minutes
                            None => true
                        }
                    };

                    if should_check {
                        unsafe { LAST_UPDATE_CHECK = Some(now); }
                        println!("Sync: Periodic remote check triggered");
                        direct_download_and_compare(&current_config, &mut state, &api, &ui_tx, &http_client).await;
                    }
                }
            }
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
    // Only proceed if we have an active torrent
    if let LocalTorrentState::Active { id } = state.local {
        if config.download_path.as_os_str().is_empty() {
            let err_msg = "Download path not configured".to_string();
            eprintln!("Sync: {}", err_msg);
            let _ = ui_tx.send(UiMessage::Error(err_msg.clone()));
            send_sync_status(ui_tx, SyncStatus::Error(err_msg));
            return;
        }

        println!(
            "Sync: Verifying folder contents at {}",
            config.download_path.display()
        );
        send_sync_status(ui_tx, SyncStatus::CheckingLocal);

        match api.api_torrent_details(id.into()) {
            Ok(details) => {
                // Get the expected files list from torrent
                let expected_files = get_expected_files_from_details(&details);

                // Check for extra files in the download directory
                match find_extra_files(&config.download_path, &expected_files) {
                    Ok(extra_files) => {
                        println!("Sync: Found {} extra files in directory", extra_files.len());
                        // Notify UI of extra files for potential deletion
                        if let Err(e) = ui_tx.send(UiMessage::ExtraFilesFound(extra_files)) {
                            eprintln!("Sync: Failed to send extra files list to UI: {}", e);
                        }
                        send_sync_status(ui_tx, SyncStatus::Idle);
                    }
                    Err(e) => {
                        let err_msg = format!("Failed to find extra files: {}", e);
                        eprintln!("Sync: {}", err_msg);
                        let _ = ui_tx.send(UiMessage::Error(err_msg.clone()));
                        send_sync_status(ui_tx, SyncStatus::Error(err_msg));
                    }
                }
            }
            Err(e) => {
                let err_msg = format!("Failed to get torrent details: {}", e);
                eprintln!("Sync: {}", err_msg);
                let _ = ui_tx.send(UiMessage::Error(err_msg.clone()));
                send_sync_status(ui_tx, SyncStatus::Error(err_msg));
            }
        }
    } else {
        let err_msg = "No active torrent to verify against".to_string();
        eprintln!("Sync: {}", err_msg);
        let _ = ui_tx.send(UiMessage::Error(err_msg.clone()));
        send_sync_status(ui_tx, SyncStatus::Error(err_msg));
    }
}

// Function to delete extra files
async fn delete_files(files_to_delete: &[PathBuf], ui_tx: &mpsc::UnboundedSender<UiMessage>) {
    println!("Sync: Deleting {} files", files_to_delete.len());
    send_sync_status(ui_tx, SyncStatus::CheckingLocal); // Re-use the CheckingLocal status

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
        let _ = ui_tx.send(UiMessage::Error(err_msg.clone()));
        send_sync_status(ui_tx, SyncStatus::Error(err_msg));
    } else {
        println!("Sync: All files deleted successfully");
        // Clear any existing error and set status back to idle
        send_sync_status(ui_tx, SyncStatus::Idle);
    }

    // Let UI know that deletion is complete (empty list = no more files to delete)
    if let Err(e) = ui_tx.send(UiMessage::ExtraFilesFound(Vec::new())) {
        eprintln!("Sync: Failed to send empty extra files list to UI: {}", e);
    }
}

// Helper function to refresh the status of the managed torrent
fn refresh_managed_torrent_status(
    api: &librqbit::Api,
    tx: &mpsc::UnboundedSender<UiMessage>,
    managed_id: usize,
) {
    println!("Sync: Fetching stats for torrent ID {}", managed_id);
    match api.api_stats_v1(managed_id.into()) {
        Ok(stats) => {
            // Send the torrent stats update
            if let Err(e) = tx.send(UiMessage::UpdateManagedTorrent(Some((managed_id, stats)))) {
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
                        send_sync_status(tx, SyncStatus::CheckingLocal);
                    }
                    TorrentStatsState::Live => {
                        // Torrent is active (downloading or seeding)
                        send_sync_status(tx, SyncStatus::LocalActive);
                    }
                    TorrentStatsState::Paused => {
                        // Torrent is paused but still loaded
                        send_sync_status(tx, SyncStatus::LocalActive);
                    }
                    TorrentStatsState::Error => {
                        // Torrent has an error
                        let err_msg = refreshed_stats
                            .error
                            .unwrap_or_else(|| "Unknown error".to_string());
                        send_sync_status(tx, SyncStatus::Error(err_msg.clone()));
                        let _ = tx.send(UiMessage::Error(err_msg));
                    }
                }
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

// Function to apply a remote update
async fn apply_remote_update(
    config: &AppConfig,
    state: &mut SyncState,
    api: &librqbit::Api,
    ui_tx: &mpsc::UnboundedSender<UiMessage>,
    torrent_content: Vec<u8>,
) -> bool {
    send_sync_status(ui_tx, SyncStatus::UpdatingTorrent);

    // Get current torrent ID to forget if we have one
    let current_id_to_forget = match state.local {
        LocalTorrentState::Active { id } => Some(id),
        LocalTorrentState::NotLoaded => None,
    };

    // Process the update with the torrent manager
    match super::torrent::manage_torrent_task(
        config,
        api,
        ui_tx,
        current_id_to_forget,
        torrent_content,
    )
    .await
    {
        Ok(new_id) => {
            println!(
                "Sync: Torrent task managed successfully. New ID: {:?}",
                new_id
            );

            // Update local state with new torrent ID
            state.local = match new_id {
                Some(id) => LocalTorrentState::Active { id },
                None => LocalTorrentState::NotLoaded,
            };

            if let LocalTorrentState::Active { id } = state.local {
                refresh_managed_torrent_status(api, ui_tx, id);
            }
            // Let status be updated by refresh or next cycle
            true
        }
        Err(e) => {
            let err_msg = format!("Sync error managing torrent: {}", e);
            eprintln!("Sync: {}", err_msg);
            let _ = ui_tx.send(UiMessage::Error(err_msg.clone()));
            send_sync_status(ui_tx, SyncStatus::Error(err_msg));
            false
        }
    }
}

/// Function to directly download a remote torrent and compare with local
async fn direct_download_and_compare(
    config: &AppConfig,
    state: &mut SyncState,
    api: &librqbit::Api,
    ui_tx: &mpsc::UnboundedSender<UiMessage>,
    http_client: &reqwest::Client,
) {
    if config.torrent_url.is_empty() {
        println!("Sync: No remote URL configured, skipping direct download.");
        send_sync_status(ui_tx, SyncStatus::Idle);
        return;
    }

    println!(
        "Sync: Directly downloading torrent from {}...",
        config.torrent_url
    );
    send_sync_status(ui_tx, SyncStatus::CheckingRemote);

    // Download the remote torrent file
    match download_torrent(&config.torrent_url, http_client).await {
        Ok(remote_torrent) => {
            println!(
                "Sync: Downloaded remote torrent successfully ({} bytes)",
                remote_torrent.len()
            );

            // Calculate hash of remote torrent
            let remote_hash = calculate_torrent_hash(&remote_torrent);
            println!("Sync: Remote torrent hash: {}", remote_hash);

            // Get local torrent hash (if exists)
            let local_hash_result = get_local_torrent_hash(state).await;

            match local_hash_result {
                Ok(Some(local_hash)) => {
                    println!("Sync: Local torrent hash: {}", local_hash);

                    // Compare hashes
                    if remote_hash != local_hash {
                        println!(
                            "Sync: Torrent has changed! Remote hash different from local hash."
                        );

                        // Generate fake ETag (use the hash)
                        let etag = remote_hash.clone();

                        // Save the new torrent to cache
                        if let Ok(cache_path) = get_cached_torrent_path() {
                            println!(
                                "Sync: Saving updated torrent to cache: {}",
                                cache_path.display()
                            );
                            if let Err(e) = tokio::fs::write(&cache_path, &remote_torrent).await {
                                eprintln!(
                                    "Sync: WARNING - Failed to write to cache file {}: {}",
                                    cache_path.display(),
                                    e
                                );
                            }
                        }

                        // Update the remote state
                        state.remote = RemoteTorrentState::UpdateAvailable {
                            content: remote_torrent.clone(),
                        };

                        // Send update message to UI
                        if let Err(e) = ui_tx.send(UiMessage::RemoteUpdateFound(remote_torrent)) {
                            let err_msg =
                                format!("Failed to send update notification to UI: {}", e);
                            eprintln!("Sync: {}", err_msg);
                            send_sync_status(ui_tx, SyncStatus::Error(err_msg));
                        } else {
                            send_sync_status(ui_tx, SyncStatus::RemoteChanged);
                        }
                    } else {
                        println!("Sync: Torrent is unchanged. Local and remote hashes match.");
                        send_sync_status(ui_tx, SyncStatus::Idle);
                    }
                }
                Ok(None) => {
                    println!("Sync: No local torrent found. This is a new torrent.");

                    // Generate fake ETag (use the hash)
                    let etag = remote_hash.clone();

                    // Save the new torrent to cache
                    if let Ok(cache_path) = get_cached_torrent_path() {
                        println!(
                            "Sync: Saving new torrent to cache: {}",
                            cache_path.display()
                        );
                        if let Err(e) = tokio::fs::write(&cache_path, &remote_torrent).await {
                            eprintln!(
                                "Sync: WARNING - Failed to write to cache file {}: {}",
                                cache_path.display(),
                                e
                            );
                        }
                    }

                    // Update the remote state
                    state.remote = RemoteTorrentState::UpdateAvailable {
                        content: remote_torrent.clone(),
                    };

                    // Send update message to UI
                    if let Err(e) = ui_tx.send(UiMessage::RemoteUpdateFound(remote_torrent)) {
                        let err_msg = format!("Failed to send update notification to UI: {}", e);
                        eprintln!("Sync: {}", err_msg);
                        send_sync_status(ui_tx, SyncStatus::Error(err_msg));
                    } else {
                        send_sync_status(ui_tx, SyncStatus::RemoteChanged);
                    }
                }
                Err(e) => {
                    let err_msg = format!("Failed to get local torrent hash: {}", e);
                    eprintln!("Sync: {}", err_msg);
                    let _ = ui_tx.send(UiMessage::Error(err_msg.clone()));
                    send_sync_status(ui_tx, SyncStatus::Error(err_msg));
                }
            }
        }
        Err(e) => {
            let err_msg = format!("Failed to download remote torrent: {}", e);
            eprintln!("Sync: {}", err_msg);
            let _ = ui_tx.send(UiMessage::Error(err_msg.clone()));
            send_sync_status(ui_tx, SyncStatus::Error(err_msg));
        }
    }
}

/// Function to download a torrent file from a URL
async fn download_torrent(url: &str, client: &reqwest::Client) -> Result<Vec<u8>> {
    println!("Sync: Downloading torrent from: {}", url);

    let response = client
        .get(url)
        .send()
        .await
        .with_context(|| format!("Failed to send request to {}", url))?;

    if !response.status().is_success() {
        return Err(anyhow!("HTTP error: {}", response.status()));
    }

    let content = response
        .bytes()
        .await
        .with_context(|| format!("Failed to read response body from {}", url))?;

    Ok(content.to_vec())
}

/// Function to calculate a hash for a torrent file
fn calculate_torrent_hash(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    let result = hasher.finalize();
    format!("{:x}", result)
}

/// Function to get the hash of the local torrent file (if it exists)
async fn get_local_torrent_hash(_state: &SyncState) -> Result<Option<String>> {
    // First check if we can get the cached torrent path
    let cache_path = get_cached_torrent_path()?;

    // Check if the file exists
    if !cache_path.exists() {
        println!(
            "Sync: No local torrent cache file found at {}",
            cache_path.display()
        );
        return Ok(None);
    }

    // Read the file
    let data = tokio::fs::read(&cache_path).await.with_context(|| {
        format!(
            "Failed to read cached torrent file: {}",
            cache_path.display()
        )
    })?;

    // Calculate hash
    let hash = calculate_torrent_hash(&data);

    Ok(Some(hash))
}
