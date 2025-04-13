// src/ui/state.rs
// UI state structure and action enum definitions

use std::path::PathBuf;
use std::collections::HashSet;
use std::time::Instant;
use crate::ui::utils::SyncStatus;
use crate::ui::torrent_file_tree::TorrentFileTree;

/// Represents a modal dialog state
#[derive(Debug)]
pub enum ModalState {
    MissingFiles(HashSet<PathBuf>),
    ExtraFiles(Vec<PathBuf>),
    RemoteUpdateAvailable,
    None,
}

/// File statistics for display in the UI
#[derive(Debug, Clone)]
pub struct TorrentFileStats {
    pub name: Option<String>,
    pub info_hash: Option<String>,
    pub output_folder: Option<String>,
    pub files: Vec<(String, u64)>, // (path, size)
}

/// Torrent statistics for the UI
#[derive(Debug, Clone)]
pub struct TorrentStats {
    pub id: usize,
    pub total_bytes: u64,
    pub progress_bytes: u64,
    pub uploaded_bytes: u64,
    pub download_speed: f64,
    pub upload_speed: f64,
    pub progress: f64,
    pub state: String,
    pub is_finished: bool,
    pub time_remaining: Option<String>,
}

/// UI State contains all the data needed by the UI components
#[derive(Debug)]
pub struct UiState {
    // Configuration
    pub config_url: String,
    pub config_path: String,
    pub download_path: PathBuf,
    
    // Error state
    pub last_error: Option<String>,
    
    // Current sync status
    pub sync_status: SyncStatus,
    
    // Torrent details
    pub torrent_stats: Option<TorrentStats>,
    pub torrent_files: Option<TorrentFileStats>,
    
    // UI components state
    pub file_tree: TorrentFileTree,
    
    // Modal dialog state
    pub modal_state: ModalState,
    
    // Refresh timing
    pub last_update: Option<Instant>,
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            config_url: String::new(),
            config_path: String::new(),
            download_path: PathBuf::new(),
            last_error: None,
            sync_status: SyncStatus::Idle,
            torrent_stats: None,
            torrent_files: None,
            file_tree: TorrentFileTree::default(),
            modal_state: ModalState::None,
            last_update: None,
        }
    }
}

impl UiState {
    /// Create a new UI State
    pub fn new(
        config_url: String,
        config_path: String,
    ) -> Self {
        Self {
            config_url,
            config_path: config_path.clone(),
            download_path: PathBuf::from(&config_path),
            ..Default::default()
        }
    }
    
    /// Check if the configuration is valid
    pub fn is_config_valid(&self) -> bool {
        !self.config_url.is_empty() && !self.download_path.as_os_str().is_empty()
    }
    
    /// Check if the download path is set
    pub fn is_download_path_set(&self) -> bool {
        !self.download_path.as_os_str().is_empty()
    }
    
    /// Get time since last update
    pub fn time_since_update(&self) -> Option<std::time::Duration> {
        self.last_update.map(|time| Instant::now().duration_since(time))
    }
}

/// UI Actions represent all possible UI interactions
#[derive(Debug, Clone)]
pub enum UiAction {
    // Configuration actions
    SaveConfig,
    UpdateFromRemote,
    VerifyLocalFiles,
    OpenDownloadFolder,
    
    // Torrent actions
    FixMissingFiles,
    DeleteExtraFiles,
    ApplyRemoteUpdate,
    
    // Modal dismissal actions
    DismissMissingFilesModal,
    DismissExtraFilesModal,
    DismissRemoteUpdateModal,
    
    // No action
    None,
} 