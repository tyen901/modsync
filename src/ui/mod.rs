use crate::app::MyApp;
use eframe::egui::{self, CentralPanel};
use crate::actions; // Import actions module

// Create sub-modules
mod torrent_display;
mod config_panel;
pub mod torrent_file_tree;
mod utils;
mod state;
mod modals;

// Re-export components
pub use torrent_display::TorrentDisplay;
pub use config_panel::ConfigPanel;
pub use utils::SyncStatus;
pub use state::{UiState, UiAction, ModalState, TorrentStats, TorrentFileStats};

/// Update the UI state from the application state
fn update_ui_state(app: &MyApp, ui_state: &mut UiState) {
    // Update configuration fields
    ui_state.config_url = app.config_edit_url.clone();
    ui_state.config_path = app.config_edit_path_str.clone();
    ui_state.download_path = app.config.download_path.clone();
    
    // Update error and sync status
    ui_state.last_error = app.last_error.clone();
    ui_state.sync_status = app.sync_status.clone();
    
    // Update torrent stats if available
    if let Some((id, stats)) = &app.managed_torrent_stats {
        let torrent_id = *id;
        
        // Convert librqbit TorrentStats to our UI TorrentStats
        ui_state.torrent_stats = Some(TorrentStats {
            id: torrent_id,
            total_bytes: stats.total_bytes,
            progress_bytes: stats.progress_bytes,
            uploaded_bytes: stats.uploaded_bytes,
            progress: if stats.total_bytes > 0 {
                stats.progress_bytes as f64 / stats.total_bytes as f64
            } else {
                if stats.finished { 1.0 } else { 0.0 }
            },
            download_speed: if let Some(live) = &stats.live {
                live.download_speed.mbps * 125_000.0
            } else {
                0.0
            },
            upload_speed: if let Some(live) = &stats.live {
                live.upload_speed.mbps * 125_000.0
            } else {
                0.0
            },
            state: match stats.state {
                librqbit::TorrentStatsState::Initializing => "Checking Files".to_string(),
                librqbit::TorrentStatsState::Live => {
                    if stats.finished {
                        "Seeding".to_string()
                    } else {
                        "Downloading".to_string()
                    }
                },
                librqbit::TorrentStatsState::Paused => {
                    if stats.finished {
                        "Completed".to_string()
                    } else {
                        "Paused".to_string()
                    }
                },
                librqbit::TorrentStatsState::Error => {
                    format!("Error: {}", stats.error.as_deref().unwrap_or("Unknown"))
                }
            },
            is_finished: stats.finished,
            time_remaining: if let Some(live) = &stats.live {
                live.time_remaining.as_ref().map(|t| t.to_string())
            } else {
                None
            },
        });
        
        // Try to fetch file details
        if let Ok(details) = app.api.api_torrent_details(torrent_id.into()) {
            let file_data: Vec<(String, u64)> = if let Some(files) = &details.files {
                files.iter()
                    .filter(|f| f.included)
                    .map(|f| (f.name.clone(), f.length))
                    .collect()
            } else {
                Vec::new()
            };
            
            ui_state.torrent_files = Some(TorrentFileStats {
                name: details.name,
                info_hash: Some(details.info_hash),
                output_folder: Some(details.output_folder),
                files: file_data,
            });
        }
    } else {
        ui_state.torrent_stats = None;
        ui_state.torrent_files = None;
    }
    
    // Update modal state
    if let Some(files) = &app.missing_files_to_prompt {
        ui_state.modal_state = ModalState::MissingFiles(files.clone());
    } else if let Some(files) = &app.extra_files_to_prompt {
        ui_state.modal_state = ModalState::ExtraFiles(files.clone());
    } else if let Some(data) = &app.remote_update {
        ui_state.modal_state = ModalState::RemoteUpdateAvailable;
    } else {
        ui_state.modal_state = ModalState::None;
    }
    
    // Update file tree
    ui_state.file_tree = app.file_tree.clone();
    
    // Update last update time if needed
    ui_state.last_update = Some(std::time::Instant::now());
}

/// Process a UI action by calling the appropriate action function
fn process_ui_action(action: UiAction, app: &mut MyApp) {
    match action {
        UiAction::SaveConfig => {
            // Validate URL and path
            if app.config_edit_url.trim().is_empty() {
                app.last_error = Some("Remote URL cannot be empty".to_string());
                return;
            }
            
            if app.config_edit_path_str.trim().is_empty() {
                app.last_error = Some("Download path cannot be empty".to_string());
                return;
            }
            
            let _ = actions::save_config_changes(app);
        },
        UiAction::UpdateFromRemote => {
            actions::update_from_remote(app);
        },
        UiAction::VerifyLocalFiles => {
            if let Err(e) = app.sync_cmd_tx.send(crate::sync::SyncCommand::VerifyFolder) {
                eprintln!("UI: Failed to send folder verify request: {}", e);
            }
        },
        UiAction::OpenDownloadFolder => {
            actions::open_download_folder(app);
        },
        UiAction::FixMissingFiles => {
            if let Err(e) = app.sync_cmd_tx.send(crate::sync::SyncCommand::FixMissingFiles) {
                eprintln!("Action: Failed to send FixMissingFiles command: {}", e);
                let _ = app.ui_tx.send(crate::sync::SyncEvent::Error(format!("Failed to send fix command: {}", e)));
            }
            app.missing_files_to_prompt = None;
        },
        UiAction::DeleteExtraFiles => {
            actions::delete_extra_files(app);
        },
        UiAction::ApplyRemoteUpdate => {
            actions::apply_remote_update(app);
        },
        UiAction::DismissMissingFilesModal => {
            app.missing_files_to_prompt = None;
        },
        UiAction::DismissExtraFilesModal => {
            app.extra_files_to_prompt = None;
        },
        UiAction::DismissRemoteUpdateModal => {
            app.remote_update = None;
            // Reset status to idle if we were showing RemoteChanged
            if app.sync_status == SyncStatus::RemoteChanged {
                app.sync_status = SyncStatus::Idle;
            }
        },
        UiAction::None => {},
    }
}

// Main function to draw the UI
pub fn draw_ui(app: &mut MyApp, ctx: &egui::Context) {
    // Create/update UI state from app
    let mut ui_state = UiState::new(
        app.config_edit_url.clone(),
        app.config_edit_path_str.clone(),
    );
    update_ui_state(app, &mut ui_state);
    
    // Variable to store action from UI components
    let mut ui_action = UiAction::None;
    
    // Draw the main UI using components
    CentralPanel::default().show(ctx, |ui| {
        // Use the ConfigPanel component
        if let Some(action) = ConfigPanel::draw(ui, &mut ui_state) {
            ui_action = action;
        }
        
        // Use the TorrentDisplay component
        if let Some(action) = TorrentDisplay::draw(ui, &mut ui_state) {
            // Only override if None
            if matches!(ui_action, UiAction::None) {
                ui_action = action;
            }
        }
    });
    
    // Draw modal dialogs if any
    if let Some(action) = modals::draw_modals(ctx, &ui_state) {
        // Modal actions take priority
        ui_action = action;
    }
    
    // Process action after UI drawing is complete
    if !matches!(ui_action, UiAction::None) {
        process_ui_action(ui_action, app);
    }
} 