use crate::app::MyApp;
use eframe::egui::{self, CentralPanel, Color32, RichText};
use librqbit::api::TorrentStats;
use std::path::PathBuf;
use crate::actions; // Import actions module

// Create sub-modules
mod torrent_display;
mod config_panel;

// Re-export components
pub use torrent_display::TorrentDisplay;
pub use config_panel::ConfigPanel;

// Enum representing the overall sync task status
#[derive(Debug, Clone, PartialEq)]
pub enum SyncStatus {
    Idle,                 // Not performing any sync operations
    CheckingRemote,       // Checking the remote torrent for updates
    UpdatingTorrent,      // Updating/replacing the managed torrent
    CheckingLocal,        // Verifying local files against torrent manifest
    Error(String),        // Error in the sync process
}

impl SyncStatus {
    pub fn display_color(&self) -> Color32 {
        match self {
            SyncStatus::Idle => Color32::GRAY,
            SyncStatus::CheckingRemote => Color32::YELLOW,
            SyncStatus::UpdatingTorrent => Color32::BLUE,
            SyncStatus::CheckingLocal => Color32::LIGHT_BLUE,
            SyncStatus::Error(_) => Color32::RED,
        }
    }
    
    pub fn display_text(&self) -> String {
        match self {
            SyncStatus::Idle => "Sync: Idle".to_string(),
            SyncStatus::CheckingRemote => "Sync: Checking Remote".to_string(),
            SyncStatus::UpdatingTorrent => "Sync: Updating Torrent".to_string(),
            SyncStatus::CheckingLocal => "Sync: Verifying Local Files".to_string(),
            SyncStatus::Error(err) => format!("Sync Error: {}", err),
        }
    }
}

// Enum for messages sent back to the UI thread from background tasks
pub enum UiMessage {
    UpdateManagedTorrent(Option<(usize, TorrentStats)>),
    TorrentAdded(usize),
    Error(String),
    // New variant for updating the sync status
    UpdateSyncStatus(SyncStatus),
    // New variant for triggering a manual refresh
    TriggerManualRefresh,
    // Variants for folder verification/cleaning
    TriggerFolderVerify,            // UI -> Sync Task
    ExtraFilesFound(Vec<PathBuf>),  // Sync Task -> UI
    DeleteExtraFiles(Vec<PathBuf>), // UI -> Sync Task
}

// Main function to draw the UI
pub fn draw_ui(app: &mut MyApp, ctx: &egui::Context) {
    CentralPanel::default().show(ctx, |ui| {
        // Use the ConfigPanel component
        ConfigPanel::draw(ui, app);
        
        // Use the TorrentDisplay component
        TorrentDisplay::draw(ui, app);
        
        // Draw the extra files prompt if needed
        draw_extra_files_prompt(ctx, ui, app);
    });
}

/// Draw the prompt to delete extra files if any were found
fn draw_extra_files_prompt(ctx: &egui::Context, ui: &mut egui::Ui, app: &mut MyApp) {
    let mut should_delete = false;
    let mut should_ignore = false;

    if let Some(files) = &app.extra_files_to_prompt {
        let file_count = files.len();
        let files_clone = files.clone(); // Clone for use inside the closure if needed
        
        // Use a unique ID for the window
        egui::Window::new("Extra Files Found")
            .id(egui::Id::new("extra_files_prompt")) // Ensure unique ID
            .collapsible(false)
            .resizable(true)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.label(format!(
                    "Found {} file(s) in the download directory that are not part of the torrent. \r
                    Do you want to delete them?",
                    file_count
                ));
                ui.separator();
                
                // Scrollable list of files
                egui::ScrollArea::vertical().max_height(200.0).show(ui, |ui| {
                    // Use the cloned list inside the closure
                    for file in &files_clone {
                        ui.label(RichText::new(file.display().to_string()).small());
                    }
                });
                
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button("Delete Extra Files").clicked() {
                        // Defer the action by setting a flag
                        should_delete = true;
                    }
                    if ui.button("Ignore").clicked() {
                        // Defer the action by setting a flag
                        should_ignore = true;
                    }
                });
            });
    }

    // Perform actions *after* the window and the immutable borrow have finished
    if should_delete {
        actions::delete_extra_files(app); // Now we can borrow mutably
    }
    if should_ignore {
        app.extra_files_to_prompt = None; // Now we can borrow mutably
    }
}

// Action helper functions removed. They are now in src/actions.rs. 