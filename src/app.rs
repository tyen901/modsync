// src/app.rs

use crate::config::AppConfig;
use crate::ui::{UiMessage, SyncStatus};
use eframe::egui;
use librqbit::api::{Api, TorrentStats};
use tokio::sync::mpsc;
use std::path::PathBuf; // Import PathBuf

// Main application struct
pub struct MyApp {
    pub(crate) api: Api, // librqbit API handle
    pub(crate) managed_torrent_stats: Option<(usize, TorrentStats)>,
    pub(crate) ui_tx: mpsc::UnboundedSender<UiMessage>,            // Channel Sender for UI updates
    pub(crate) ui_rx: mpsc::UnboundedReceiver<UiMessage>,          // Channel Receiver for UI updates
    // Added sender for config updates to sync task
    pub(crate) config_update_tx: mpsc::UnboundedSender<AppConfig>,
    pub(crate) sync_cmd_tx: mpsc::UnboundedSender<UiMessage>,         // Store the sync command sender
    pub(crate) config: AppConfig,                                     // Current application config
    // Temporary fields for UI input before saving
    pub(crate) config_edit_url: String,       // Temp storage for URL input
    pub(crate) config_edit_path_str: String, // Temp storage for path input
    pub(crate) last_error: Option<String>,  // To display errors in the UI
    pub(crate) sync_status: SyncStatus,     // Current status of the sync task
    pub(crate) extra_files_to_prompt: Option<Vec<PathBuf>>, // Files for delete prompt
    pub(crate) file_tree: crate::ui::torrent_file_tree::TorrentFileTree, // Add state for the file tree
}

impl MyApp {
    // Creates a new instance of the application
    pub fn new(
        api: Api,
        ui_tx: mpsc::UnboundedSender<UiMessage>,
        ui_rx: mpsc::UnboundedReceiver<UiMessage>,
        config_update_tx: mpsc::UnboundedSender<AppConfig>,
        sync_cmd_tx: mpsc::UnboundedSender<UiMessage>,
        initial_config: AppConfig,
    ) -> Self {
        let config_edit_url = initial_config.torrent_url.clone();
        let config_edit_path_str = initial_config
            .download_path
            .to_string_lossy()
            .into_owned();
        Self {
            api,
            managed_torrent_stats: None,
            ui_tx,
            ui_rx,
            config_update_tx,
            sync_cmd_tx,         // Store the sync command sender
            config: initial_config,
            config_edit_url,
            config_edit_path_str,
            last_error: None,
            sync_status: SyncStatus::Idle, // Start in idle state
            extra_files_to_prompt: None, // Initialize prompt state
            file_tree: Default::default(), // Initialize file tree state
        }
    }
}

// Implement the eframe::App trait for the main application struct
impl eframe::App for MyApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Schedule a repaint periodically to ensure UI stays fresh
        ctx.request_repaint_after(std::time::Duration::from_secs(1));
        
        // Process any messages received from background tasks
        while let Ok(message) = self.ui_rx.try_recv() {
            match message {
                UiMessage::UpdateManagedTorrent(torrent_stats_opt) => {
                    println!("UI received managed torrent stats update: {:?}", torrent_stats_opt.as_ref().map(|(id, _)| id));
                    self.managed_torrent_stats = torrent_stats_opt;
                    
                    // Clear error on successful update
                    self.last_error = None; 
                }
                UiMessage::TorrentAdded(id) => {
                    println!("UI notified: Torrent {} added/managed", id);
                    self.last_error = None;
                    
                    // Immediately refresh torrent stats when added
                    self.refresh_current_torrent_stats();
                }
                UiMessage::Error(err_msg) => {
                    eprintln!("UI received error: {}", err_msg);
                    self.last_error = Some(err_msg.clone());
                    // Still set sync status to Error since this is about the sync process
                    self.sync_status = SyncStatus::Error(err_msg);
                }
                UiMessage::UpdateSyncStatus(status) => {
                    println!("UI received sync status update: {:?}", status);
                    // Check if we need to refresh when returning to idle
                    let should_refresh = status == SyncStatus::Idle;
                    
                    self.sync_status = status;
                    // If the new status isn't an error, clear the last error
                    if !matches!(self.sync_status, SyncStatus::Error(_)) {
                        self.last_error = None;
                    }
                    
                    // If returning to idle after an update, refresh torrent details
                    if should_refresh {
                        self.refresh_current_torrent_stats();
                    }
                }
                // Handle the new message for displaying the prompt
                UiMessage::ExtraFilesFound(files) => {
                    println!("UI received ExtraFilesFound: {} files", files.len());
                    if files.is_empty() {
                        // Maybe show a confirmation? For now, just clear.
                        self.extra_files_to_prompt = None;
                    } else {
                        self.extra_files_to_prompt = Some(files);
                    }
                    // Don't change overall sync status here, let the sync task manage it.
                }
                // TriggerManualRefresh messages are meant for the sync manager, not the UI
                UiMessage::TriggerManualRefresh | UiMessage::TriggerFolderVerify | UiMessage::DeleteExtraFiles(_) => {
                    // This shouldn't happen as these messages are sent directly to the sync manager
                    // But handle it just in case
                    println!("UI received Sync Manager Command - forwarding");
                    // Forward the original message
                    if let Err(e) = self.sync_cmd_tx.send(message) {
                        eprintln!("Error forwarding sync command: {}", e);
                    }
                }
            }
        }
        
        // Automatic refresh of current torrent details
        // This ensures we always have fresh torrent stats even if no messages are received
        static mut LAST_REFRESH: Option<std::time::Instant> = None;
        
        let now = std::time::Instant::now();
        let should_refresh = unsafe {
            match LAST_REFRESH {
                Some(last) => now.duration_since(last).as_secs() >= 2, // Refresh every 2 seconds
                None => true
            }
        };
        
        if should_refresh {
            unsafe { LAST_REFRESH = Some(now); }
            self.refresh_current_torrent_stats();
        }

        // Draw the UI elements
        crate::ui::draw_ui(self, ctx);
    }
}

// Add a helper method to MyApp to refresh the current torrent
impl MyApp {
    // Helper method to refresh the current torrent stats
    fn refresh_current_torrent_stats(&self) {
        if let Some((id, _)) = &self.managed_torrent_stats {
            // Only attempt refresh if we have a managed torrent
            let torrent_id = *id;
            // Use api_stats_v1 now
            match self.api.api_stats_v1(torrent_id.into()) {
                Ok(stats) => {
                    // Send the updated stats
                    if let Err(e) = self.ui_tx.send(UiMessage::UpdateManagedTorrent(Some((torrent_id, stats)))) {
                        eprintln!("Failed to send torrent stats update: {}", e);
                    }
                }
                Err(e) => {
                    eprintln!("Failed to refresh torrent stats {}: {}", torrent_id, e);
                }
            }
        }
    }
} 