// src/sync/utils.rs

//! Utility functions for sync operations

use anyhow::{Context, Result, anyhow};
use sha2::{Digest, Sha256};
use tokio::sync::mpsc;

use crate::config::get_cached_torrent_path;
use crate::ui::SyncStatus;
use super::messages::SyncEvent;

/// Helper function to send any event to UI
pub fn send_sync_event(tx: &mpsc::UnboundedSender<SyncEvent>, event: SyncEvent) {
    if let Err(e) = tx.send(event) {
        eprintln!("Sync: Failed to send event to UI: {}", e);
    }
}

/// Helper for sending status specifically
pub fn send_sync_status_event(tx: &mpsc::UnboundedSender<SyncEvent>, status: SyncStatus) {
    send_sync_event(tx, SyncEvent::StatusUpdate(status));
}

/// Function to download a torrent file from a URL
pub async fn download_torrent(url: &str, client: &reqwest::Client) -> Result<Vec<u8>> {
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
pub fn calculate_torrent_hash(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    let result = hasher.finalize();
    format!("{:x}", result)
}

/// Function to get the hash of the local torrent file (if it exists)
pub async fn get_local_torrent_hash() -> Result<Option<String>> {
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