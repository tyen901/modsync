// src/ui/config_panel.rs
// Component for configuration UI

use eframe::egui::{self, RichText};

/// Component for handling configuration settings
pub struct ConfigPanel;

impl ConfigPanel {
    /// Draw the configuration panel
    pub fn draw(ui: &mut egui::Ui, ui_state: &mut crate::ui::UiState) -> Option<crate::ui::UiAction> {
        let mut action = None;
        
        ui.heading("ModSync Configuration");
        ui.separator();

        // URL input
        ui.horizontal(|ui| {
            ui.label("Remote Torrent URL:");
            ui.text_edit_singleline(&mut ui_state.config_url);
        });
        
        // Path input
        ui.horizontal(|ui| {
            ui.label("Local Download Path:");
            ui.text_edit_singleline(&mut ui_state.config_path);
        });

        ui.separator();

        // Buttons row 1 - Configuration
        ui.horizontal(|ui| {
            // Save config button
            if ui.button("Save Configuration").clicked() {
                action = Some(crate::ui::UiAction::SaveConfig);
            }
            
            // New button to update from remote URL
            if ui.button("Update from Remote").clicked() {
                action = Some(crate::ui::UiAction::UpdateFromRemote);
            }
        });
        
        // Buttons row 2 - Operations
        ui.horizontal(|ui| {
            // Verify button (only enabled when config is valid)
            Self::draw_verify_button(ui, ui_state, &mut action);

            // Open folder button (only enabled if path is set)
            Self::draw_open_folder_button(ui, ui_state, &mut action);
        });

        ui.separator();
        
        // Sync status display
        Self::draw_sync_status(ui, ui_state);
        
        ui.separator();
        
        action
    }
    
    /// Draw the verify local files button
    fn draw_verify_button(ui: &mut egui::Ui, ui_state: &crate::ui::UiState, action: &mut Option<crate::ui::UiAction>) {
        // Enable button only when config is valid
        let is_config_valid = ui_state.is_config_valid();

        if ui.add_enabled(
            is_config_valid,
            egui::Button::new("Verify Local Files")
        ).clicked() {
            *action = Some(crate::ui::UiAction::VerifyLocalFiles);
        }
    }
    
    /// Draw the open download folder button
    fn draw_open_folder_button(ui: &mut egui::Ui, ui_state: &crate::ui::UiState, action: &mut Option<crate::ui::UiAction>) {
        // Enable button only when download path is configured
        let is_path_set = ui_state.is_download_path_set();

        if ui.add_enabled(
            is_path_set,
            egui::Button::new("Open Folder")
        ).clicked() {
            *action = Some(crate::ui::UiAction::OpenDownloadFolder);
        }
    }
    
    /// Draw the sync status display
    fn draw_sync_status(ui: &mut egui::Ui, ui_state: &crate::ui::UiState) {
        ui.horizontal(|ui| {
            ui.label("Sync Status: ");
            ui.label(
                RichText::new(ui_state.sync_status.display_text())
                    .color(ui_state.sync_status.display_color())
                    .strong()
            );
        });
    }
} 