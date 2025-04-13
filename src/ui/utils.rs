// src/ui/utils.rs
// Common UI utilities and formatting functions

use eframe::egui::Color32;

/// Helper function to format speed in bytes/sec to KB/s or MB/s
pub fn format_speed(bytes_per_sec: f64) -> String {
    if bytes_per_sec < 1024.0 {
        format!("{:.0} B/s", bytes_per_sec)
    } else if bytes_per_sec < 1024.0 * 1024.0 {
        format!("{:.1} KB/s", bytes_per_sec / 1024.0)
    } else {
        format!("{:.1} MB/s", bytes_per_sec / (1024.0 * 1024.0))
    }
}

/// Helper function to format file size in bytes to human-readable format
pub fn format_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.2} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}

/// Enum representing the overall sync task status
#[derive(Debug, Clone, PartialEq)]
pub enum SyncStatus {
    Idle,                  // Not performing any sync operations
    CheckingRemote,        // Checking the remote torrent for updates
    UpdatingTorrent,       // Updating/replacing the managed torrent
    CheckingLocal,         // Verifying local files against torrent manifest
    LocalActive,           // Local torrent is active and seeding/downloading
    RemoteChanged,         // Remote torrent has changed, update available
    Error(String),         // Error in the sync process
}

impl SyncStatus {
    pub fn display_color(&self) -> Color32 {
        match self {
            SyncStatus::Idle => Color32::GRAY,
            SyncStatus::CheckingRemote => Color32::YELLOW,
            SyncStatus::UpdatingTorrent => Color32::BLUE,
            SyncStatus::CheckingLocal => Color32::LIGHT_BLUE,
            SyncStatus::LocalActive => Color32::GREEN,
            SyncStatus::RemoteChanged => Color32::GOLD,
            SyncStatus::Error(_) => Color32::RED,
        }
    }
    
    pub fn display_text(&self) -> String {
        match self {
            SyncStatus::Idle => "Sync: Idle".to_string(),
            SyncStatus::CheckingRemote => "Sync: Checking Remote".to_string(),
            SyncStatus::UpdatingTorrent => "Sync: Updating Torrent".to_string(),
            SyncStatus::CheckingLocal => "Sync: Verifying Local Files".to_string(),
            SyncStatus::LocalActive => "Local: Active & Seeding".to_string(),
            SyncStatus::RemoteChanged => "Remote: Update Available".to_string(),
            SyncStatus::Error(err) => format!("Sync Error: {}", err),
        }
    }
} 