// src/ui/modals.rs
// Module for handling modal dialogs

use eframe::egui::{self, Context, Window, RichText};
use crate::ui::state::{UiState, UiAction, ModalState};

/// Draw modal dialogs based on the current UI state
pub fn draw_modals(ctx: &Context, ui_state: &mut UiState) -> Option<UiAction> {
    println!("Drawing modals, current modal state: {:?}", std::mem::discriminant(&ui_state.modal_state));
    
    match &ui_state.modal_state {
        ModalState::MissingFiles(files) => {
            println!("Drawing missing files modal");
            let files_copy = files.clone();
            draw_missing_files_modal(ctx, &files_copy)
        },
        ModalState::ExtraFiles(files) => {
            println!("Drawing extra files modal");
            let files_copy = files.clone();
            draw_extra_files_modal(ctx, &files_copy)
        },
        ModalState::RemoteUpdateAvailable => {
            println!("Drawing remote update modal");
            draw_remote_update_modal(ctx)
        },
        ModalState::Settings => {
            println!("Drawing settings modal");
            draw_settings_modal(ctx, ui_state)
        },
        ModalState::None => {
            None
        },
    }
}

/// Draw the settings modal dialog
fn draw_settings_modal(ctx: &Context, ui_state: &mut UiState) -> Option<UiAction> {
    let mut action = None;
    let mut open = true;
    
    Window::new("Settings")
        .id(egui::Id::new("settings_modal"))
        .collapsible(false)
        .resizable(false)
        .min_width(400.0)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .open(&mut open)
        .show(ctx, |ui| {
            println!("Inside settings modal render callback");
            ui.heading("Profile Settings");
            ui.add_space(4.0);
            ui.label("Configure your download and sharing preferences.");
            ui.separator();
            ui.add_space(8.0);
            
            // Create a frame for the settings
            egui::Frame::new()
                .inner_margin(10.0)
                .fill(ui.style().visuals.extreme_bg_color)
                .corner_radius(4.0)
                .show(ui, |ui| {
                    // Should seed checkbox
                    ui.horizontal(|ui| {
                        ui.label("Contribute to seeding:");
                        ui.checkbox(&mut ui_state.should_seed, "");
                        ui.label(RichText::new("Share with others after downloading").weak());
                    });
                    
                    ui.add_space(8.0);
                    
                    // Upload/download section
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            // Max upload speed input (KB/s)
                            ui.horizontal(|ui| {
                                ui.label("Max Upload Speed (KB/s):");
                                ui.add_enabled(
                                    ui_state.should_seed, 
                                    egui::TextEdit::singleline(&mut ui_state.max_upload_speed_str)
                                        .desired_width(80.0)
                                ).on_hover_text("Leave empty for unlimited");
                            });
                            
                            // Max download speed input (KB/s)
                            ui.horizontal(|ui| {
                                ui.label("Max Download Speed (KB/s):");
                                ui.add(
                                    egui::TextEdit::singleline(&mut ui_state.max_download_speed_str)
                                        .desired_width(80.0)
                                ).on_hover_text("Leave empty for unlimited");
                            });
                        });
                        
                        ui.vertical(|ui| {
                            ui.label(RichText::new("Empty = Unlimited").weak());
                            ui.label(RichText::new("1000 KB/s = 1 MB/s").weak());
                        });
                    });
                });
            
            // Update the Option<u64> values based on the string inputs
            ui_state.max_upload_speed = ui_state.parse_speed_limit(&ui_state.max_upload_speed_str);
            ui_state.max_download_speed = ui_state.parse_speed_limit(&ui_state.max_download_speed_str);
            
            ui.add_space(8.0);
            ui.separator();
            
            // Buttons at the bottom
            ui.add_space(4.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::RIGHT), |ui| {
                if ui.button("Save").clicked() {
                    println!("Settings Save button clicked");
                    action = Some(UiAction::SaveSettingsAndDismiss);
                }
                if ui.button("Cancel").clicked() {
                    println!("Settings Cancel button clicked");
                    action = Some(UiAction::DismissSettingsModal);
                }
            });
        });
    
    action
}

/// Draw the missing files modal dialog
fn draw_missing_files_modal(ctx: &Context, missing_files: &std::collections::HashSet<std::path::PathBuf>) -> Option<UiAction> {
    let mut action = None;
    
    Window::new("Missing Files Detected")
        .id(egui::Id::new("missing_files_prompt"))
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
                    action = Some(UiAction::FixMissingFiles);
                }
                if ui.button("Ignore").clicked() {
                    action = Some(UiAction::DismissMissingFilesModal);
                }
            });
            
            ui.label(RichText::new("Note: Fixing missing files will restart the torrent and re-download any missing files.").italics().small());
        });
    
    action
}

/// Draw the extra files modal dialog
fn draw_extra_files_modal(ctx: &Context, extra_files: &[std::path::PathBuf]) -> Option<UiAction> {
    let mut action = None;
    
    Window::new("Extra Files Found")
        .id(egui::Id::new("extra_files_prompt"))
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
                    action = Some(UiAction::DeleteExtraFiles);
                }
                if ui.button("Ignore").clicked() {
                    action = Some(UiAction::DismissExtraFilesModal);
                }
            });
        });
    
    action
}

/// Draw the remote update modal dialog
fn draw_remote_update_modal(ctx: &Context) -> Option<UiAction> {
    let mut action = None;
    
    Window::new("Remote Update Available")
        .id(egui::Id::new("remote_update_prompt"))
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
                    action = Some(UiAction::ApplyRemoteUpdate);
                }
                if ui.button("Ignore").clicked() {
                    action = Some(UiAction::DismissRemoteUpdateModal);
                }
            });
        });
    
    action
} 