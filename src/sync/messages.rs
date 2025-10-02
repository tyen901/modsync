use crate::config::AppConfig;
use crate::ui::utils::SyncStatus;
use std::path::PathBuf;
use std::sync::Arc;
use std::collections::HashSet;

#[derive(Debug)]
pub enum SyncCommand {
    UpdateConfig(AppConfig),
    VerifyFolder,
    DeleteFiles(Vec<PathBuf>),
    ApplyUpdate(Vec<u8>),
    DownloadAndCompare(String),
    FixMissingFiles,
}

#[derive(Debug, Clone)]
pub enum SyncEvent {
    ManagedTorrentUpdate(Option<(usize, Arc<librqbit::TorrentStats>)>),
    TorrentAdded(usize),
    Error(String),
    StatusUpdate(SyncStatus),
    ExtraFilesFound(Vec<PathBuf>),
    RemoteUpdateFound(Vec<u8>),
    MissingFilesFound(HashSet<PathBuf>),
}