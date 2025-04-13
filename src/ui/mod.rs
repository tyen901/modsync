use crate::app::MyApp;
use eframe::egui::{self, CentralPanel, Color32, RichText};
use crate::actions; // Import actions module

// Create sub-modules
mod torrent_display;
mod config_panel;
pub mod torrent_file_tree;

// Re-export components
pub use torrent_display::TorrentDisplay;
pub use config_panel::ConfigPanel;

// Enum representing the overall sync task status
#[derive(Debug, Clone, PartialEq)]
pub enum SyncStatus {
    Idle,                  // Not performing any sync operations
    CheckingRemote,        // Checking the remote torrent for updates
    UpdatingTorrent,       // Updating/replacing the managed torrent
    CheckingLocal,         // Verifying local files against torrent manifest
    LocalActive,           // Local torrent is active and seeding/downloading
    RemoteChanged,         // Remote torrent has changed, update available
    Error(String),         // Error in the sync process
}

impl SyncStatus {
    pub fn display_color(&self) -> Color32 {
        match self {
            SyncStatus::Idle => Color32::GRAY,
            SyncStatus::CheckingRemote => Color32::YELLOW,
            SyncStatus::UpdatingTorrent => Color32::BLUE,
            SyncStatus::CheckingLocal => Color32::LIGHT_BLUE,
            SyncStatus::LocalActive => Color32::GREEN,
            SyncStatus::RemoteChanged => Color32::GOLD,
            SyncStatus::Error(_) => Color32::RED,
        }
    }
    
    pub fn display_text(&self) -> String {
        match self {
            SyncStatus::Idle => "Sync: Idle".to_string(),
            SyncStatus::CheckingRemote => "Sync: Checking Remote".to_string(),
            SyncStatus::UpdatingTorrent => "Sync: Updating Torrent".to_string(),
            SyncStatus::CheckingLocal => "Sync: Verifying Local Files".to_string(),
            SyncStatus::LocalActive => "Local: Active & Seeding".to_string(),
            SyncStatus::RemoteChanged => "Remote: Update Available".to_string(),
            SyncStatus::Error(err) => format!("Sync Error: {}", err),
        }
    }
}

// Main function to draw the UI
pub fn draw_ui(app: &mut MyApp, ctx: &egui::Context) {
    CentralPanel::default().show(ctx, |ui| {
        // Use the ConfigPanel component
        ConfigPanel::draw(ui, app);
        
        // Use the TorrentDisplay component
        TorrentDisplay::draw(ui, app);
        
        // Draw the missing files prompt if needed
        draw_missing_files_prompt(ctx, ui, app);
        
        // Draw the extra files prompt if needed
        draw_extra_files_prompt(ctx, ui, app);
        
        // Draw the remote update prompt if needed
        draw_remote_update_prompt(ctx, ui, app);
    });
}

/// Draw the prompt for missing files if any were found
fn draw_missing_files_prompt(ctx: &egui::Context, _ui: &mut egui::Ui, app: &mut MyApp) {
    let mut should_fix = false;
    let mut should_ignore = false;
    
    if let Some(missing_files) = &app.missing_files_to_prompt {
        // Display a modal dialog listing the missing files and asking what to do
        egui::Window::new("Missing Files Detected")
            .id(egui::Id::new("missing_files_prompt")) // Ensure unique ID
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.label(format!("{} expected files are missing from the download folder:", missing_files.len()));
                
                // Create a scrollable area for the files
                egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
                    for file_path in missing_files {
                        ui.label(file_path.display().to_string());
                    }
                });
                
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button("Fix Missing Files").clicked() {
                        // Defer the action by setting a flag
                        should_fix = true;
                    }
                    if ui.button("Ignore").clicked() {
                        // Defer the action by setting a flag
                        should_ignore = true;
                    }
                });
                
                ui.label(RichText::new("Note: Fixing missing files will restart the torrent and re-download any missing files.").italics().small());
            });
    }
    
    // Perform actions *after* the window and the immutable borrow have finished
    if should_fix {
        fix_missing_files(app);
    }

    if should_ignore {
        app.missing_files_to_prompt = None;
    }
}

/// Function to send the fix missing files command
fn fix_missing_files(app: &mut MyApp) {
    println!("Action: Fix missing files requested");
    if let Err(e) = app.sync_cmd_tx.send(crate::sync::SyncCommand::FixMissingFiles) {
        eprintln!("Action: Failed to send FixMissingFiles command: {}", e);
        let _ = app.ui_tx.send(crate::sync::SyncEvent::Error(format!("Failed to send fix command: {}", e)));
    }
    // Clear the prompt after sending the command
    app.missing_files_to_prompt = None;
}

/// Draw the prompt to delete extra files if any were found
fn draw_extra_files_prompt(ctx: &egui::Context, _ui: &mut egui::Ui, app: &mut MyApp) {
    let mut should_delete = false;
    let mut should_ignore = false;
    
    if let Some(extra_files) = &app.extra_files_to_prompt {
        // Display a modal dialog listing the files and asking if they should be deleted
        egui::Window::new("Extra Files Found")
            .id(egui::Id::new("extra_files_prompt")) // Ensure unique ID
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.label(format!("{} files were found in the download folder that are not listed in the torrent:", extra_files.len()));
                
                // Create a scrollable area for the files
                egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
                    for file_path in extra_files {
                        ui.label(file_path.display().to_string());
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
     // Now we can borrow mutably
    if should_delete {
        actions::delete_extra_files(app);
    }

    if should_ignore {
        app.extra_files_to_prompt = None;
    }
}

/// Draw the prompt for a remote torrent update if one is available
fn draw_remote_update_prompt(ctx: &egui::Context, _ui: &mut egui::Ui, app: &mut MyApp) {
    let mut should_update = false;
    let mut should_ignore = false;

    if let Some(_) = &app.remote_update {
        // Set the UI status to show that a remote update is available
        if app.sync_status == SyncStatus::Idle {
            app.sync_status = SyncStatus::RemoteChanged;
        }

        // Use a unique ID for the window
        egui::Window::new("Remote Update Available")
            .id(egui::Id::new("remote_update_prompt")) // Ensure unique ID
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.label("A new version of the remote torrent is available.");
                ui.label("Do you want to update your local copy?");
                ui.label(RichText::new("Note: Files that are not part of the updated torrent will need to be reviewed.").italics().small());
                
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button("Update").clicked() {
                        // Defer the action by setting a flag
                        should_update = true;
                    }
                    if ui.button("Ignore").clicked() {
                        // Defer the action by setting a flag
                        should_ignore = true;
                    }
                });
            });
    }

    // Perform actions *after* the window and the immutable borrow have finished
    if should_update {
        actions::apply_remote_update(app); // Will implement this action
    }
    if should_ignore {
        app.remote_update = None; // Clear the update prompt
        // Reset status to idle if we were showing RemoteChanged
        if app.sync_status == SyncStatus::RemoteChanged {
            app.sync_status = SyncStatus::Idle;
        }
    }
}

// Action helper functions removed. They are now in src/actions.rs. 
// Action helper functions removed. They are now in src/actions.rs. 