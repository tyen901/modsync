// src/ui/torrent_display.rs
// Component for displaying information about the single active torrent

use eframe::egui::{self, RichText, ProgressBar, Color32, Ui, Layout, Align, SidePanel, CentralPanel};

/// Represents the torrent display component that shows torrent details
pub struct TorrentDisplay;

impl TorrentDisplay {
    /// Draw the torrent details
    pub fn draw(ui: &mut egui::Ui, ui_state: &mut crate::ui::UiState) -> Option<crate::ui::UiAction> {
        ui.heading("Torrent Status");

        // --- Error Display (if any) ---
        if let Some(error) = &ui_state.last_error {
             ui.label(RichText::new(format!("Error: {}", error)).color(Color32::RED));
             ui.add_space(8.0);
        }

        // --- Display sync status ---
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("Sync: {}", ui_state.sync_status.display_text()))
                .color(ui_state.sync_status.display_color()));
        });
        ui.add_space(8.0);

        // --- Use the torrent stats from UiState --- 
        if let Some(stats) = &ui_state.torrent_stats {
            // Main area for torrent details, using a panel layout
            SidePanel::left("file_tree_panel")
                .resizable(true)
                .default_width(250.0)
                .show_inside(ui, |ui| {
                     ui.heading("Files");
                     ui.separator();
                     if let Some(file_stats) = &ui_state.torrent_files {
                        // Check if files field exists and has items
                        if !file_stats.files.is_empty() {
                            ui_state.file_tree.ui(ui, &file_stats.files);
                        } else {
                            ui.label("No file information available.");
                        }
                     } else {
                        ui.label("No file information available.");
                     }
                });

            CentralPanel::default().show_inside(ui, |ui| {
                // Pass stats and file details to the main info drawing function
                Self::draw_torrent_stats_and_details(ui, stats, ui_state.torrent_files.as_ref());
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
             CentralPanel::default().show_inside(ui, |ui| {
                Self::draw_no_torrent_message(ui, ui_state);
             });
        }
        
        None // This component doesn't produce actions
    }
    
    /// Draw detailed information using TorrentStats and TorrentFileStats
    fn draw_torrent_stats_and_details(ui: &mut Ui, stats: &crate::ui::TorrentStats, file_details: Option<&crate::ui::TorrentFileStats>) {
        let progress = stats.progress;
        let state_str = &stats.state;
        let down_speed = stats.download_speed;
        let up_speed = stats.upload_speed;
        
        let total_size = stats.total_bytes;
        let progress_percentage = progress * 100.0;
        
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
        
        egui::Frame::NONE
            .fill(ui.style().visuals.widgets.noninteractive.bg_fill)
            .inner_margin(12.0)
            .outer_margin(0.0)
            .corner_radius(8.0)
            .show(ui, |ui| {
                // Torrent name and state
                ui.horizontal(|ui| {
                    if let Some(file_details) = file_details {
                        ui.heading(file_details.name.as_deref().unwrap_or("Unknown Torrent"));
                    } else {
                        ui.heading(format!("Torrent {}", stats.id));
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(RichText::new(&*state_str).color(state_color).strong());
                        ui.label("Status:");
                    });
                });
                ui.add_space(4.0);
                
                // Progress bar
                ui.add(ProgressBar::new(progress as f32)
                    .show_percentage()
                    .animate(should_animate));
                ui.add_space(4.0);
                
                // Stats in a grid layout
                ui.columns(2, |columns| {
                    // Left column - Basic info
                    columns[0].with_layout(Layout::top_down(Align::LEFT), |ui| {
                        Self::info_section(ui, "Basic Information");
                        Self::info_row(ui, "Torrent ID", &format!("{}", stats.id));
                        if let Some(file_details) = file_details {
                            if let Some(info_hash) = &file_details.info_hash {
                                Self::info_row(ui, "Info Hash", info_hash);
                            }
                        }
                        Self::info_row(ui, "Size", &crate::ui::utils::format_size(total_size));
                        Self::info_row(ui, "Progress", &format!("{:.2}%", progress_percentage));
                        if let Some(file_details) = file_details {
                            if let Some(output_folder) = &file_details.output_folder {
                                Self::info_row(ui, "Output Folder", output_folder);
                            }
                        }
                    });
                    
                    // Right column - Transfer stats
                    columns[1].with_layout(Layout::top_down(Align::LEFT), |ui| {
                        Self::info_section(ui, "Transfer Information");
                        Self::info_row(ui, "Download Speed", &crate::ui::utils::format_speed(down_speed));
                        Self::info_row(ui, "Upload Speed", &crate::ui::utils::format_speed(up_speed));
                        Self::info_row(ui, "Downloaded", &crate::ui::utils::format_size(stats.progress_bytes));
                        Self::info_row(ui, "Uploaded", &crate::ui::utils::format_size(stats.uploaded_bytes));
                        
                        // Display ETA if available
                        if let Some(eta) = &stats.time_remaining {
                            let eta_str = eta.clone();
                            Self::info_row(ui, "ETA", &eta_str);
                        }
                    });
                });
            });
    }
    
    /// Helper to display a section header
    fn info_section(ui: &mut Ui, title: &str) {
        ui.label(RichText::new(title).strong().underline());
        ui.add_space(4.0);
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