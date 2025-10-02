// src/sync/types.rs

#[derive(Debug)]
pub enum LocalTorrentState {
    NotLoaded,
    Active {
        id: usize,
    },
}

#[derive(Debug)]
pub enum RemoteTorrentState {
    Unknown,
    Checked,
    UpdateAvailable,
}

#[derive(Debug)]
pub struct SyncState {
    pub local: LocalTorrentState,
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

use std::path::PathBuf;

/// Minimal config used by the sync subsystem. This replaces the previous
/// dependency on the top-level `AppConfig` and keeps the sync crate
/// independent from the old system.
#[derive(Debug, Clone)]
pub struct SyncConfig {
    pub torrent_url: String,
    pub download_path: PathBuf,
    pub max_upload_speed: Option<u32>,
    pub max_download_speed: Option<u32>,
    pub should_seed: bool,
    /// Optional path where the cached torrent file may be stored. The
    /// sync subsystem will not try to discover this itself; it must be
    /// supplied by the client if desired.
    pub cached_torrent_path: Option<PathBuf>,
}

impl Default for SyncConfig {
    fn default() -> Self {
        SyncConfig {
            torrent_url: String::new(),
            download_path: PathBuf::new(),
            max_upload_speed: None,
            max_download_speed: None,
            should_seed: false,
            cached_torrent_path: None,
        }
    }
}