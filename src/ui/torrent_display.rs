// src/ui/torrent_display.rs
// Component for displaying information about the single active torrent

use eframe::egui::{self, RichText, ProgressBar, Color32, Ui, Layout, Align, CollapsingHeader};
use crate::ui::state::TorrentTab;

/// Represents the torrent display component that shows torrent details
pub struct TorrentDisplay;

impl TorrentDisplay {
    /// Draw the torrent details
    pub fn draw(ui: &mut egui::Ui, ui_state: &mut crate::ui::UiState) -> Option<crate::ui::UiAction> {
        // --- Error Display (if any) ---
        if let Some(error) = &ui_state.last_error {
             ui.label(RichText::new(format!("Error: {}", error)).color(Color32::RED));
             ui.add_space(8.0);
        }

        // --- Use the torrent stats from UiState --- 
        if let Some(stats) = &ui_state.torrent_stats {
            // Extract all needed data up front to avoid borrowing issues
            let progress = stats.progress;
            let state_str = stats.state.clone();
            let down_speed = stats.download_speed;
            let up_speed = stats.upload_speed;
            let torrent_id = stats.id;
            let total_bytes = stats.total_bytes;
            let progress_bytes = stats.progress_bytes;
            let uploaded_bytes = stats.uploaded_bytes;
            let eta = stats.time_remaining.clone();
            
            // Extract file information if available
            let mut file_name = None;
            let mut info_hash = None;
            let mut output_folder = None;
            let mut file_list = Vec::new();
            
            if let Some(file_details) = &ui_state.torrent_files {
                file_name = file_details.name.clone();
                info_hash = file_details.info_hash.clone();
                output_folder = file_details.output_folder.clone();
                file_list = file_details.files.clone();
            }
            
            // Determine if we should animate the progress bar
            let should_animate = state_str == "Downloading" || state_str == "Checking Files";
            
            // Get the color for the state text
            let state_color = match state_str.as_str() {
                "Completed" => Color32::from_rgb(50, 205, 50),  // Lime Green
                "Downloading" => Color32::GREEN,
                "Seeding" => Color32::DARK_GREEN,
                "Paused" => Color32::GRAY,
                "Checking Files" => Color32::YELLOW,
                _ if state_str.starts_with("Error") => Color32::RED,
                _ => Color32::WHITE,
            };
            
            // Main content frame
            egui::Frame::NONE
                .fill(ui.style().visuals.widgets.noninteractive.bg_fill)
                .inner_margin(12.0)
                .outer_margin(0.0)
                .corner_radius(8.0)
                .show(ui, |ui| {
                    
                    // 1. Progress bar as window element
                    ui.add(ProgressBar::new(progress as f32)
                        .show_percentage()
                        .animate(should_animate));
                    
                    // 2. Status bar directly below progress
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                            ui.label("Status: ");
                            ui.label(RichText::new(&state_str).color(state_color).strong());
                        });
                    });
                    
                    // 3. Connection details bar
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        // Note: We don't have peer count in our current stats
                        // Could be added in a future enhancement
                        ui.label("Peers: -");
                        ui.separator();
                        ui.label(format!("Download: {}", crate::ui::utils::format_speed(down_speed)));
                        ui.separator();
                        ui.label(format!("Upload: {}", crate::ui::utils::format_speed(up_speed)));
                    });
                    
                    ui.add_space(8.0);
                    
                    // 4. Tabs - Simple buttons rather than complex widgets
                    ui.horizontal(|ui| {
                        // Read the current tab state directly for comparison
                        let is_details_selected = matches!(ui_state.torrent_tab_state, TorrentTab::Details);
                        let is_files_selected = matches!(ui_state.torrent_tab_state, TorrentTab::Files);
                        
                        // Use selectable_label for tabs, directly updating ui_state
                        if ui.selectable_label(is_details_selected, "Details").clicked() {
                            ui_state.torrent_tab_state = TorrentTab::Details;
                        }
                        
                        if ui.selectable_label(is_files_selected, "Files").clicked() {
                            ui_state.torrent_tab_state = TorrentTab::Files;
                        }
                    });
                    
                    ui.add_space(4.0);
                    ui.separator();
                    ui.add_space(4.0);

                    // Display tab content based on the *current* selected tab state
                    match ui_state.torrent_tab_state {
                        TorrentTab::Details => {
                            // Details tab content
                            Self::draw_details_content(
                                ui, 
                                torrent_id, 
                                total_bytes, 
                                progress, 
                                progress_bytes, 
                                uploaded_bytes, 
                                down_speed, 
                                up_speed, 
                                &eta, 
                                &file_name, 
                                &info_hash, 
                                &output_folder
                            );
                        },
                        TorrentTab::Files => {
                            // Files tab content
                            Self::draw_files_content(ui, ui_state, &file_list);
                        }
                    }
                });

            // Show last updated time
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                ui.label(RichText::new("Last updated: ").small().color(Color32::GRAY));
                if let Some(update_time) = ui_state.time_since_update() {
                    let seconds_ago = update_time.as_secs();
                    ui.label(RichText::new(format!("{} seconds ago", seconds_ago))
                        .small().color(Color32::GRAY));
                } else {
                    ui.label(RichText::new("just now")
                        .small().color(Color32::GRAY));
                }
            });
        } else {
            // Display message when no torrent is active
            Self::draw_no_torrent_message(ui, ui_state);
        }
        
        None // No longer need to return actions for tab changes
    }

    /// Draw the Details tab content
    fn draw_details_content(
        ui: &mut Ui,
        torrent_id: usize,
        total_bytes: u64,
        progress: f64,
        progress_bytes: u64,
        uploaded_bytes: u64,
        download_speed: f64,
        upload_speed: f64,
        eta: &Option<String>,
        file_name: &Option<String>,
        info_hash: &Option<String>,
        output_folder: &Option<String>,
    ) {
        // Basic Information section
        CollapsingHeader::new("Basic Information")
            .default_open(true)
            .show(ui, |ui| {
                Self::info_row(ui, "Torrent ID", &format!("{}", torrent_id));
                if let Some(name) = file_name {
                    Self::info_row(ui, "Name", name);
                }
                if let Some(hash) = info_hash {
                    Self::info_row(ui, "Info Hash", hash);
                }
                Self::info_row(ui, "Size", &crate::ui::utils::format_size(total_bytes));
                Self::info_row(ui, "Progress", &format!("{:.2}%", progress * 100.0));
                if let Some(folder) = output_folder {
                    Self::info_row(ui, "Output Folder", folder);
                }
            });
        
        ui.add_space(4.0);
        
        // Transfer Information section
        CollapsingHeader::new("Transfer Information")
            .default_open(true)
            .show(ui, |ui| {
                Self::info_row(ui, "Downloaded", &crate::ui::utils::format_size(progress_bytes));
                Self::info_row(ui, "Uploaded", &crate::ui::utils::format_size(uploaded_bytes));
                Self::info_row(ui, "Download Speed", &crate::ui::utils::format_speed(download_speed));
                Self::info_row(ui, "Upload Speed", &crate::ui::utils::format_speed(upload_speed));
                
                // Display ETA if available
                if let Some(eta_str) = eta {
                    Self::info_row(ui, "ETA", eta_str);
                }
            });
    }
    
    /// Draw the Files tab content
    fn draw_files_content(ui: &mut Ui, ui_state: &mut crate::ui::UiState, file_list: &[(String, u64)]) {
        if !file_list.is_empty() {
            ui_state.file_tree.ui(ui, file_list);
        } else {
            ui.label("No file information available.");
        }
    }
    
    /// Helper to display a labeled info row
    fn info_row(ui: &mut Ui, label: &str, value: &str) {
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("{}:", label)).strong());
            ui.label(value);
        });
    }
    
    /// Show a message when no torrent is active
    fn draw_no_torrent_message(ui: &mut egui::Ui, ui_state: &crate::ui::UiState) {
        if ui_state.is_config_valid() {
            ui.label("No torrent active or fetching status...");
        } else {
            ui.label("Please configure Remote URL and Local Path.");
        }
    }
} 