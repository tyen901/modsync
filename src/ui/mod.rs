use crate::app::MyApp;
use eframe::egui::{self, CentralPanel};
use crate::actions; // Import actions module
use crate::ui::state::{UiState, UiAction, TorrentStats, TorrentFileStats, ModalState};
use crate::ui::utils::SyncStatus; // Import SyncStatus

// Create sub-modules
mod torrent_display;
mod config_panel;
pub mod torrent_file_tree;
pub mod utils; // Make utils public
pub mod state; // Make state module public
mod modals;

/// Update the mutable UI state based on the immutable App state
fn update_persistent_ui_state(
    // Pass only the needed immutable fields from App
    app_config: &crate::config::AppConfig,
    config_edit_url: &str,
    config_edit_path_str: &str,
    last_error: &Option<String>,
    sync_status: &SyncStatus,
    missing_files_to_prompt: &Option<std::collections::HashSet<std::path::PathBuf>>,
    extra_files_to_prompt: &Option<Vec<std::path::PathBuf>>,
    remote_update: &Option<Vec<u8>>,
    managed_torrent_stats: &Option<(usize, std::sync::Arc<librqbit::TorrentStats>)>, 
    api: &librqbit::Api, // Needed for file details call
    ui_state: &mut UiState, // The state to update
) {
    // Update fields in ui_state that depend on app state
    
    // Update configuration edit fields
    ui_state.config_url = config_edit_url.to_string();
    ui_state.config_path = config_edit_path_str.to_string();
    ui_state.download_path = app_config.download_path.clone();
    
    // Update status/error derived from app
    ui_state.last_error = last_error.clone();
    ui_state.sync_status = sync_status.clone();
    
    // Update modal state based on app prompts
    if let Some(files) = missing_files_to_prompt {
        ui_state.modal_state = ModalState::MissingFiles(files.clone());
    } else if let Some(files) = extra_files_to_prompt {
        ui_state.modal_state = ModalState::ExtraFiles(files.clone());
    } else if remote_update.is_some() { 
        ui_state.modal_state = ModalState::RemoteUpdateAvailable;
    } else {
        ui_state.modal_state = ModalState::None;
    }
    
    // Update torrent stats if available from app state
    if let Some((id, stats)) = managed_torrent_stats {
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
        if let Ok(details) = api.api_torrent_details(torrent_id.into()) {
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
        } else {
            // Clear file details if API call fails
            ui_state.torrent_files = None; 
        }
    } else {
        ui_state.torrent_stats = None;
        ui_state.torrent_files = None;
    }
    
    // Update last update time
    ui_state.last_update = Some(std::time::Instant::now());
}

/// Process a UI action by calling the appropriate action function
fn process_ui_action(action: UiAction, app: &mut MyApp) {
    match action {
        UiAction::SaveConfig => {
            // Validate URL and path (reading from app state)
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
        // UiAction::SetTorrentTab(_) => { /* No-op */ }
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

// Main function to draw the UI - Now takes &mut UiState from MyApp
pub fn draw_ui(app: &mut MyApp, ctx: &egui::Context) {
    // Update the persistent ui_state based on app state
    // Pass only the necessary immutable fields from app
    update_persistent_ui_state(
        &app.config,
        &app.config_edit_url,
        &app.config_edit_path_str,
        &app.last_error,
        &app.sync_status,
        &app.missing_files_to_prompt,
        &app.extra_files_to_prompt,
        &app.remote_update,
        &app.managed_torrent_stats,
        &app.api,
        &mut app.ui_state // Pass mutable ui_state
    );
    
    // Variable to store action from UI components
    let mut ui_action = UiAction::None;
    
    // Draw the main UI using components, passing mutable ui_state
    CentralPanel::default().show(ctx, |ui| {
        // Use the ConfigPanel component - Use full path
        if let Some(action) = config_panel::ConfigPanel::draw(ui, &mut app.ui_state) {
            ui_action = action;
        }
        
        // Use the TorrentDisplay component - Use full path
        if let Some(action) = torrent_display::TorrentDisplay::draw(ui, &mut app.ui_state) {
            // Only override if None
            if matches!(ui_action, UiAction::None) {
                ui_action = action;
            }
        }
    });
    
    // Draw modal dialogs if any - Use full path
    if let Some(action) = modals::draw_modals(ctx, &app.ui_state) {
        // Modal actions take priority
        ui_action = action;
    }
    
    // Process action after UI drawing is complete
    if !matches!(ui_action, UiAction::None) {
        process_ui_action(ui_action, app);
    }
} 