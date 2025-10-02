// src/sync/status.rs
// Shared SyncStatus enum used by the sync subsystem. This was previously part of the UI
// module; the enum has been moved here so sync logic doesn't depend on UI code.

#[derive(Debug, Clone, PartialEq)]
pub enum SyncStatus {
    Idle,
    CheckingRemote,
    UpdatingTorrent,
    CheckingLocal,
    LocalActive,
    RemoteChanged,
    Error(String),
}

impl Default for SyncStatus {
    fn default() -> Self {
        SyncStatus::Idle
    }
}
