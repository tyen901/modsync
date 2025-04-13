// src/sync/types.rs

//! Defines the types used to track the state of local and remote torrent files

/// Enum to represent the local torrent state
#[derive(Debug)]
pub enum LocalTorrentState {
    /// No torrent loaded locally
    NotLoaded,
    
    /// Torrent is active and being managed
    Active {
        /// The ID of the active torrent
        id: usize,
        // Other local state attributes can be added here
    },
}

/// Enum to represent the remote torrent state
#[derive(Debug)]
pub enum RemoteTorrentState {
    /// State unknown (first run or error)
    Unknown,
    
    /// Remote torrent checked and up-to-date
    Checked,
    
    /// Remote has an update that can be applied
    UpdateAvailable,
}

/// Structure to hold the sync state with clear separation
#[derive(Debug)]
pub struct SyncState {
    /// The state of the local torrent
    pub local: LocalTorrentState,
    
    /// The state of the remote torrent
    pub remote: RemoteTorrentState,
}

impl Default for SyncState {
    fn default() -> Self {
        SyncState {
            local: LocalTorrentState::NotLoaded,
            remote: RemoteTorrentState::Unknown,
        }
    }
} 