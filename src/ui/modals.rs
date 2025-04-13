// src/ui/modals.rs
// Module for handling modal dialogs

use eframe::egui::{self, Context, Window, RichText};
use crate::ui::state::{UiState, UiAction, ModalState};

/// Draw modal dialogs based on the current UI state
pub fn draw_modals(ctx: &Context, ui_state: &UiState) -> Option<UiAction> {
    match &ui_state.modal_state {
        ModalState::MissingFiles(files) => draw_missing_files_modal(ctx, files),
        ModalState::ExtraFiles(files) => draw_extra_files_modal(ctx, files),
        ModalState::RemoteUpdateAvailable => draw_remote_update_modal(ctx),
        ModalState::None => None,
    }
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