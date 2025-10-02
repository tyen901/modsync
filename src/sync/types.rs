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