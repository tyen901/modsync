// src/ui/state.rs
// UI state structure and action enum definitions

use std::path::PathBuf;
use std::collections::HashSet;
use std::time::Instant;
use crate::ui::utils::SyncStatus;
use crate::ui::torrent_file_tree::TorrentFileTree;

/// Represents a modal dialog state
#[derive(Debug, Clone)]
pub enum ModalState {
    MissingFiles(HashSet<PathBuf>),
    ExtraFiles(Vec<PathBuf>),
    RemoteUpdateAvailable,
    Settings,
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

/// Represents the available tabs in the torrent display
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TorrentTab {
    Details,
    Files,
}

impl Default for TorrentTab {
    fn default() -> Self {
        Self::Details
    }
}

/// UI State contains all the data needed by the UI components
#[derive(Debug)]
pub struct UiState {
    // Configuration
    pub config_url: String,
    pub config_path: String,
    pub download_path: PathBuf,
    
    // Profile settings
    pub should_seed: bool,
    pub max_upload_speed: Option<u64>,
    pub max_download_speed: Option<u64>,
    pub max_upload_speed_str: String,
    pub max_download_speed_str: String,
    
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
    
    // Torrent display tab state
    pub torrent_tab_state: TorrentTab,
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            config_url: String::new(),
            config_path: String::new(),
            download_path: PathBuf::new(),
            should_seed: true,
            max_upload_speed: None,
            max_download_speed: None,
            max_upload_speed_str: String::new(),
            max_download_speed_str: String::new(),
            last_error: None,
            sync_status: SyncStatus::Idle,
            torrent_stats: None,
            torrent_files: None,
            file_tree: TorrentFileTree::default(),
            modal_state: ModalState::None,
            last_update: None,
            torrent_tab_state: TorrentTab::default(),
        }
    }
}

impl UiState {
    /// Create a new UI State
    pub fn new(
        config_url: String,
        config_path: String,
        should_seed: bool,
        max_upload_speed: Option<u64>,
        max_download_speed: Option<u64>,
    ) -> Self {
        Self {
            config_url,
            config_path: config_path.clone(),
            download_path: PathBuf::from(&config_path),
            should_seed,
            max_upload_speed,
            max_download_speed,
            max_upload_speed_str: max_upload_speed.map_or(String::new(), |v| v.to_string()),
            max_download_speed_str: max_download_speed.map_or(String::new(), |v| v.to_string()),
            torrent_tab_state: TorrentTab::default(),
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
    
    /// Parse speed strings to Option<u64>
    pub fn parse_speed_limit(&self, speed_str: &str) -> Option<u64> {
        if speed_str.is_empty() {
            None
        } else {
            speed_str.parse::<u64>().ok()
        }
    }
}

/// UI Actions represent all possible UI interactions
#[derive(Debug, Clone, PartialEq)]
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
    
    // Modal actions
    ShowSettingsModal,
    SaveSettingsAndDismiss,
    
    // Modal dismissal actions
    DismissMissingFilesModal,
    DismissExtraFilesModal,
    DismissRemoteUpdateModal,
    DismissSettingsModal,
    
    // No action
    None,
} 