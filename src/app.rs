// src/app.rs

use crate::config::AppConfig;
use crate::ui::SyncStatus;
use crate::sync::{SyncCommand, SyncEvent};
use eframe::egui;
use librqbit::api::{Api, TorrentStats};
use tokio::sync::mpsc;
use std::path::PathBuf; // Import PathBuf
use std::sync::Arc;

// Main application struct
pub struct MyApp {
    pub(crate) api: Api, // librqbit API handle
    pub(crate) managed_torrent_stats: Option<(usize, Arc<TorrentStats>)>,
    // Update channels to use our new message types
    pub(crate) ui_rx: mpsc::UnboundedReceiver<SyncEvent>,          // Receive events from sync manager
    pub(crate) sync_cmd_tx: mpsc::UnboundedSender<SyncCommand>,    // Send commands to sync manager
    pub(crate) ui_tx: mpsc::UnboundedSender<SyncEvent>,            // For UI thread to send events
    pub(crate) config: AppConfig,                                     // Current application config
    // Temporary fields for UI input before saving
    pub(crate) config_edit_url: String,       // Temp storage for URL input
    pub(crate) config_edit_path_str: String, // Temp storage for path input
    pub(crate) last_error: Option<String>,  // To display errors in the UI
    pub(crate) sync_status: SyncStatus,     // Current status of the sync task
    pub(crate) extra_files_to_prompt: Option<Vec<PathBuf>>, // Files for delete prompt
    pub(crate) file_tree: crate::ui::torrent_file_tree::TorrentFileTree, // Add state for the file tree
    // New fields for remote update detection
    pub(crate) remote_update: Option<Vec<u8>>, // Torrent content from remote update
    // Time tracking
    last_refresh: Option<std::time::Instant>, // Track when we last refreshed stats
}

impl MyApp {
    // Creates a new instance of the application
    pub fn new(
        api: Api,
        ui_tx: mpsc::UnboundedSender<SyncEvent>,
        ui_rx: mpsc::UnboundedReceiver<SyncEvent>,
        sync_cmd_tx: mpsc::UnboundedSender<SyncCommand>,
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
            sync_cmd_tx,         // Store the sync command sender
            config: initial_config,
            config_edit_url,
            config_edit_path_str,
            last_error: None,
            sync_status: SyncStatus::Idle, // Start in idle state
            extra_files_to_prompt: None, // Initialize prompt state
            file_tree: Default::default(), // Initialize file tree state
            remote_update: None, // Initialize remote update state
            last_refresh: None, // Initialize last refresh state
        }
    }
}

// Implement the eframe::App trait for the main application struct
impl eframe::App for MyApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Schedule a repaint periodically to ensure UI stays fresh
        ctx.request_repaint_after(std::time::Duration::from_secs(1));
        
        // Process any messages received from the sync task via ui_rx
        while let Ok(event) = self.ui_rx.try_recv() {
            match event {
                SyncEvent::ManagedTorrentUpdate(torrent_stats_opt) => {
                    println!("UI received managed torrent stats update: {:?}", torrent_stats_opt.as_ref().map(|(id, _)| id));
                    self.managed_torrent_stats = torrent_stats_opt;
                    self.last_error = None; 
                }
                SyncEvent::TorrentAdded(id) => {
                    println!("UI notified: Torrent {} added/managed", id);
                    self.last_error = None;
                    self.refresh_current_torrent_stats();
                }
                SyncEvent::Error(err_msg) => {
                    eprintln!("UI received error: {}", err_msg);
                    self.last_error = Some(err_msg.clone());
                    self.sync_status = SyncStatus::Error(err_msg);
                }
                SyncEvent::StatusUpdate(status) => {
                    println!("UI received sync status update: {:?}", status);
                    let should_refresh = status == SyncStatus::Idle;
                    self.sync_status = status;
                    if !matches!(self.sync_status, SyncStatus::Error(_)) {
                        self.last_error = None;
                    }
                    if should_refresh {
                        self.refresh_current_torrent_stats();
                    }
                }
                SyncEvent::ExtraFilesFound(files) => {
                    println!("UI received ExtraFilesFound: {} files", files.len());
                    if files.is_empty() {
                        self.extra_files_to_prompt = None;
                    } else {
                        self.extra_files_to_prompt = Some(files);
                    }
                }
                SyncEvent::RemoteUpdateFound(torrent_data) => {
                    println!("UI received RemoteUpdateFound: {} bytes", torrent_data.len());
                    self.remote_update = Some(torrent_data);
                }
            }
        }
        
        // Automatic refresh of current torrent details
        // This ensures we always have fresh torrent stats even if no messages are received
        let now = std::time::Instant::now();
        let should_refresh = match self.last_refresh {
            Some(last) => now.duration_since(last).as_secs() >= 2, // Refresh every 2 seconds
            None => true
        };
        
        if should_refresh {
            self.last_refresh = Some(now);
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
                    // Send the updated stats - wrap in Arc
                    if let Err(e) = self.ui_tx.send(SyncEvent::ManagedTorrentUpdate(Some((torrent_id, Arc::new(stats))))) {
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