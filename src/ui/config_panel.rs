// src/ui/config_panel.rs
// Component for configuration UI

use crate::app::MyApp;
use crate::actions::{self, save_config_changes, update_from_remote};
use crate::ui::UiMessage;
use eframe::egui::{self, RichText};

/// Component for handling configuration settings
pub struct ConfigPanel;

impl ConfigPanel {
    /// Draw the configuration panel
    pub fn draw(ui: &mut egui::Ui, app: &mut MyApp) {
        ui.heading("ModSync Configuration");
        ui.separator();

        // URL input
        ui.horizontal(|ui| {
            ui.label("Remote Torrent URL:");
            ui.text_edit_singleline(&mut app.config_edit_url);
        });
        
        // Path input
        ui.horizontal(|ui| {
            ui.label("Local Download Path:");
            ui.text_edit_singleline(&mut app.config_edit_path_str);
        });

        // Buttons row 1 - Configuration
        ui.horizontal(|ui| {
            // Save config button
            if ui.button("Save Configuration").clicked() {
                save_config_changes(app);
            }
            
            // New button to update from remote URL
            if ui.button("Update from Remote").clicked() {
                update_from_remote(app);
            }
        });
        
        // Buttons row 2 - Operations
        ui.horizontal(|ui| {
            // Verify button (only enabled when config is valid)
            Self::draw_verify_button(ui, app);

            // Open folder button (only enabled if path is set)
            Self::draw_open_folder_button(ui, app);
        });

        ui.separator();
        
        // Sync status display
        Self::draw_sync_status(ui, app);
        
        ui.separator();
    }
    
    /// Draw the verify local files button
    fn draw_verify_button(ui: &mut egui::Ui, app: &mut MyApp) {
        // Enable button only when config is valid
        let is_config_valid = !app.config.torrent_url.is_empty() && 
                             !app.config.download_path.as_os_str().is_empty();

        if ui.add_enabled(
            is_config_valid,
            egui::Button::new("Verify Local Files")
        ).clicked() {
            println!("UI: Verify local files requested");
            if let Err(e) = app.sync_cmd_tx.send(UiMessage::TriggerFolderVerify) {
                eprintln!("UI: Failed to send folder verify request: {}", e);
            }
        }
    }
    
    /// Draw the open download folder button
    fn draw_open_folder_button(ui: &mut egui::Ui, app: &mut MyApp) {
        // Enable button only when download path is configured
        let is_path_set = !app.config.download_path.as_os_str().is_empty();

        if ui.add_enabled(
            is_path_set,
            egui::Button::new("Open Folder")
        ).clicked() {
             println!("UI: Open folder requested for: {}", app.config.download_path.display());
             // Call the action function (to be created)
             actions::open_download_folder(app);
        }
    }
    
    /// Draw the sync status display
    fn draw_sync_status(ui: &mut egui::Ui, app: &MyApp) {
        ui.horizontal(|ui| {
            ui.label("Sync Status: ");
            ui.label(
                RichText::new(app.sync_status.display_text())
                    .color(app.sync_status.display_color())
                    .strong()
            );
        });
    }
} 