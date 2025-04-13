// src/sync/messages.rs

//! Defines the message types used for communication between the sync manager and UI

use crate::config::AppConfig;
use crate::ui::SyncStatus;
use std::path::PathBuf;
use std::sync::Arc;
use std::collections::HashSet;

/// Commands that can be sent from the UI to the Sync Manager
#[derive(Debug)]
pub enum SyncCommand {
    /// Update the configuration used by the sync manager
    UpdateConfig(AppConfig),
    
    /// Verify local folder contents against the torrent manifest
    VerifyFolder,
    
    /// Delete specified files that are not part of the torrent
    DeleteFiles(Vec<PathBuf>),
    
    /// Apply a remote torrent update with the provided torrent data
    ApplyUpdate(Vec<u8>),
    
    /// Download and compare a torrent from the specified URL
    DownloadAndCompare(String),
    
    /// Fix missing files by restarting the torrent
    FixMissingFiles,
}

/// Events that can be sent from the Sync Manager to the UI
#[derive(Debug, Clone)]
pub enum SyncEvent {
    /// Update about the managed torrent's status
    /// Wrapped in Arc to avoid Clone requirements for TorrentStats
    ManagedTorrentUpdate(Option<(usize, Arc<librqbit::TorrentStats>)>),
    
    /// Notification that a torrent was added with the given ID
    TorrentAdded(usize),
    
    /// Error message from the sync manager
    Error(String),
    
    /// Update about the overall sync status
    StatusUpdate(SyncStatus),
    
    /// Notification about extra files found in the download directory
    ExtraFilesFound(Vec<PathBuf>),
    
    /// Notification that a remote update is available
    RemoteUpdateFound(Vec<u8>),
    
    /// Notification about missing files found in the download directory
    MissingFilesFound(HashSet<PathBuf>),
} 