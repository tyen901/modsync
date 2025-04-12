// src/ui/torrent_display.rs
// Component for displaying information about the single active torrent

use crate::app::MyApp;
use eframe::egui::{self, RichText, ProgressBar, Color32, Ui, Layout, Align, SidePanel, CentralPanel};
use librqbit::{TorrentStatsState, api::{TorrentDetailsResponse, TorrentStats}};
use chrono;

/// Helper function to format speed in bytes/sec to KB/s or MB/s
fn format_speed(bytes_per_sec: f64) -> String {
    if bytes_per_sec < 1024.0 {
        format!("{:.0} B/s", bytes_per_sec)
    } else if bytes_per_sec < 1024.0 * 1024.0 {
        format!("{:.1} KB/s", bytes_per_sec / 1024.0)
    } else {
        format!("{:.1} MB/s", bytes_per_sec / (1024.0 * 1024.0))
    }
}

/// Helper function to format file size in bytes to human-readable format
fn format_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.2} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}

/// Represents the torrent display component that shows torrent details
pub struct TorrentDisplay;

impl TorrentDisplay {
    /// Draw the torrent details
    pub fn draw(ui: &mut egui::Ui, app: &mut MyApp) {
        ui.heading("Torrent Status");

        // --- Error Display (if any) ---
        if let Some(error) = &app.last_error {
             ui.label(RichText::new(format!("Error: {}", error)).color(Color32::RED));
             ui.add_space(8.0);
        }

        // --- Display sync status ---
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("Sync: {}", app.sync_status.display_text()))
                .color(app.sync_status.display_color()));
        });
        ui.add_space(8.0);

        // --- Use the managed_torrent_stats from MyApp --- 
        if let Some((id, stats)) = &app.managed_torrent_stats {
            // Fetch details separately if needed for name, files etc.
            match app.api.api_torrent_details((*id).into()) {
                Ok(details) => {
                    // Main area for torrent details, using a panel layout
                    SidePanel::left("file_tree_panel")
                        .resizable(true)
                        .default_width(250.0)
                        .show_inside(ui, |ui| {
                             ui.heading("Files");
                             ui.separator();
                             if let Some(files) = &details.files {
                                // Convert to the format expected by TorrentFileTree
                                let file_data: Vec<(String, u64)> = files.iter()
                                    .filter(|f| f.included)
                                    .map(|f| (f.name.clone(), f.length))
                                    .collect();
                                app.file_tree.ui(ui, &file_data);
                             } else {
                                ui.label("No file information available.");
                             }
                        });

                    CentralPanel::default().show_inside(ui, |ui| {
                        // Pass both stats and details to the main info drawing function
                        Self::draw_torrent_stats_and_details(ui, *id, stats, &details);
                    });
                }
                Err(e) => {
                    // If details fail, still attempt to draw basic stats in the central area
                    CentralPanel::default().show_inside(ui, |ui| {
                        ui.label(RichText::new(format!("Warning: Could not fetch full details for torrent {}: {}. Showing basic stats.", id, e)).color(Color32::YELLOW));
                        Self::draw_torrent_stats_only(ui, *id, stats);
                    });
                }
            }

            // Show last updated time (consider moving inside CentralPanel?)
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                ui.label(RichText::new("Last updated: ").small().color(Color32::GRAY));
                ui.label(RichText::new(format!("{}", chrono::Local::now().format("%H:%M:%S")))
                    .small().color(Color32::GRAY));
            });
        } else {
            // Display message when no torrent is active
             CentralPanel::default().show_inside(ui, |ui| {
                Self::draw_no_torrent_message(ui, app);
             });
        }
    }
    
    /// Draw detailed information using both TorrentStats and TorrentDetailsResponse
    fn draw_torrent_stats_and_details(ui: &mut Ui, id: usize, stats: &TorrentStats, details: &TorrentDetailsResponse) {
        let (progress, state_str, down_speed, up_speed) = 
            Self::extract_torrent_stats(stats); // Use stats directly
        
        let total_size = stats.total_bytes;
        let progress_percentage = progress * 100.0;
        
        // Determine if we should animate the progress bar
        let should_animate = state_str == "Downloading" || state_str == "Checking Files";
        
        // Get the color for the state text (logic remains similar)
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
                // Torrent name (from details) and state (from stats)
                ui.horizontal(|ui| {
                    ui.heading(details.name.as_deref().unwrap_or("Unknown Torrent"));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(RichText::new(&state_str).color(state_color).strong());
                        ui.label("Status:");
                    });
                });
                ui.add_space(4.0);
                
                // Progress bar (from stats)
                ui.add(ProgressBar::new(progress as f32)
                    .show_percentage()
                    .animate(should_animate));
                ui.add_space(4.0);
                
                // Stats in a grid layout
                ui.columns(2, |columns| {
                    // Left column - Basic info (mix of details and stats)
                    columns[0].with_layout(Layout::top_down(Align::LEFT), |ui| {
                        Self::info_section(ui, "Basic Information");
                        Self::info_row(ui, "Torrent ID", &format!("{}", id)); // Use passed ID
                        Self::info_row(ui, "Info Hash", &details.info_hash); // From details
                        Self::info_row(ui, "Size", &format_size(total_size)); // From stats
                        Self::info_row(ui, "Progress", &format!("{:.2}%", progress_percentage)); // From stats
                        Self::info_row(ui, "Output Folder", &details.output_folder); // From details
                    });
                    
                    // Right column - Transfer stats (from stats)
                    columns[1].with_layout(Layout::top_down(Align::LEFT), |ui| {
                        Self::info_section(ui, "Transfer Information");
                        Self::info_row(ui, "Download Speed", &format_speed(down_speed));
                        Self::info_row(ui, "Upload Speed", &format_speed(up_speed));
                        
                        // Use stats directly
                        Self::info_row(ui, "Downloaded", &format_size(stats.progress_bytes));
                        Self::info_row(ui, "Uploaded", &format_size(stats.uploaded_bytes));
                        
                        if let Some(live) = &stats.live {
                            if let Some(time) = &live.time_remaining {
                                Self::info_row(ui, "ETA", &format!("{}", time));
                            }
                        }
                    });
                });
            });
    }
    
    /// Draw minimal info using only TorrentStats when details are unavailable
    fn draw_torrent_stats_only(ui: &mut Ui, id: usize, stats: &TorrentStats) {
        let (progress, state_str, down_speed, up_speed) = 
            Self::extract_torrent_stats(stats);
            
        let progress_percentage = progress * 100.0;
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
                 ui.horizontal(|ui| {
                    ui.heading(format!("Torrent {}", id)); // Show ID as name fallback
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(RichText::new(&state_str).color(state_color).strong());
                        ui.label("Status:");
                    });
                });
                ui.add_space(4.0);
                
                ui.add(ProgressBar::new(progress as f32).show_percentage());
                ui.add_space(4.0);
                
                Self::info_row(ui, "Progress", &format!("{:.2}%", progress_percentage));
                Self::info_row(ui, "Downloaded", &format_size(stats.progress_bytes));
                Self::info_row(ui, "Uploaded", &format_size(stats.uploaded_bytes));
                Self::info_row(ui, "Download Speed", &format_speed(down_speed));
                Self::info_row(ui, "Upload Speed", &format_speed(up_speed));
                if let Some(live) = &stats.live {
                    if let Some(time) = &live.time_remaining {
                        Self::info_row(ui, "ETA", &format!("{}", time));
                    }
                }
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
    
    /// Extract stats from the torrent stats struct
    fn extract_torrent_stats(stats: &TorrentStats) -> (f64, String, f64, f64) {
        // Use stats directly - no need for details response
        let progress = if stats.total_bytes > 0 {
            stats.progress_bytes as f64 / stats.total_bytes as f64
        } else {
            if stats.finished { 1.0 } else { 0.0 }
        };
        
        let is_finished = stats.finished;
        
        let state_str = match stats.state {
            TorrentStatsState::Initializing => "Checking Files".to_string(),
            TorrentStatsState::Live => {
                if is_finished {
                    "Seeding".to_string()
                } else {
                    "Downloading".to_string()
                }
            }
            TorrentStatsState::Paused => {
                if is_finished {
                    "Completed".to_string()
                } else {
                    "Paused".to_string()
                }
            }
            TorrentStatsState::Error => format!("Error: {}", stats.error.as_deref().unwrap_or("Unknown")),
        };

        let mut down_speed = 0.0;
        let mut up_speed = 0.0;
        if let Some(live) = &stats.live {
            down_speed = live.download_speed.mbps * 125_000.0;
            up_speed = live.upload_speed.mbps * 125_000.0;
        }
        
        (progress, state_str, down_speed, up_speed)
    }
    
    /// Show a message when no torrent is active
    fn draw_no_torrent_message(ui: &mut egui::Ui, app: &MyApp) {
        // Check config directly as managed_torrent_stats might be None even if config is set
        if !app.config.torrent_url.is_empty() && !app.config.download_path.as_os_str().is_empty() {
            ui.label("No torrent active or fetching status...");
        } else {
            ui.label("Please configure Remote URL and Local Path.");
        }
    }
} 