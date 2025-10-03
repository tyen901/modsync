use eframe::{egui, App, Frame};
use egui::{Color32, RichText, Vec2};
use std::time::Instant;

use crate::ui::header::Header;
use rfd::FileDialog;

// Layout constants
const PROGRESS_PANEL_HEIGHT: f32 = 140.0;
const MIN_INPUT_WIDTH: f32 = 80.0;
const ACTION_BUTTON_HEIGHT: f32 = 36.0;

// UI-local state
struct UiState {
    url: String,
    folder: String,
}

pub struct ModApp {
    last_update: Instant,
    header: Header,
    ui_state: UiState,
    torrent_progress: crate::ui::torrent_progress::TorrentProgress,
    // Inline settings (moved from the side panel)
    should_seed: bool,
    upload_str: String,
    download_str: String,
    // Demo
    demo_mode: bool,
    demo_percent: f64,
}

impl Default for ModApp {
    fn default() -> Self {
        Self {
            last_update: Instant::now(),
            header: Header::default(),
            ui_state: UiState { url: String::new(), folder: String::from("downloads") },
            torrent_progress: crate::ui::torrent_progress::TorrentProgress::new(),
            should_seed: false,
            upload_str: String::new(),
            download_str: String::new(),
            demo_mode: false,
            demo_percent: 0.0,
        }
    }
}

fn init_style(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();
    style.visuals.widgets.inactive.bg_fill = Color32::from_rgb(24, 24, 26);
    style.visuals.window_fill = Color32::from_rgb(10, 10, 12);
    style.visuals.override_text_color = Some(Color32::from_rgb(235, 235, 235));
    style.spacing.item_spacing = egui::vec2(10.0, 6.0);
    style.spacing.button_padding = egui::vec2(12.0, 8.0);
    style.text_styles.get_mut(&egui::TextStyle::Heading).map(|ts| ts.size = 30.0);
    style.text_styles.get_mut(&egui::TextStyle::Body).map(|ts| ts.size = 15.0);
    ctx.set_style(style);
}

impl App for ModApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut Frame) {
        // timing + style
        self.last_update = Instant::now();
        init_style(ctx);

        // Top controls: header + inputs + actions + inline settings
        egui::TopBottomPanel::top("controls_panel").show(ctx, |ui| {
            // Header
            ui.horizontal(|ui| {
                self.header.ui(ui);
            });

            ui.add_space(6.0);

            // URL row (stretching input)
            ui.horizontal(|ui| {
                let avail = ui.available_width();
                let btn_w = 110.0_f32.min(avail * 0.18);
                let input_w = (avail - btn_w - ui.spacing().item_spacing.x).max(MIN_INPUT_WIDTH);
                ui.add_sized(egui::vec2(input_w, 28.0), egui::widgets::TextEdit::singleline(&mut self.ui_state.url).hint_text("Torrent URL"));
                if ui.add_sized(egui::vec2(btn_w, 28.0), egui::widgets::Button::new("Load").fill(Color32::from_rgb(70,130,180))).clicked() {}
            });

            ui.add_space(6.0);

            // Folder row (stretching input)
            ui.horizontal(|ui| {
                let avail = ui.available_width();
                let btn_w = 110.0_f32.min(avail * 0.18);
                let input_w = (avail - btn_w - ui.spacing().item_spacing.x).max(MIN_INPUT_WIDTH);
                ui.add_sized(egui::vec2(input_w, 28.0), egui::widgets::TextEdit::singleline(&mut self.ui_state.folder).hint_text("Download folder"));
                if ui.add_sized(egui::vec2(btn_w, 28.0), egui::widgets::Button::new("Browse").fill(Color32::from_rgb(100,160,100))).clicked() {
                    if let Some(folder) = FileDialog::new().pick_folder() {
                        self.ui_state.folder = folder.display().to_string();
                    }
                }
            });

            ui.add_space(8.0);

            // Action buttons — evenly distributed and wrapping when necessary
            ui.horizontal_wrapped(|ui| {
                let avail = ui.available_width();
                let spacing = ui.spacing().item_spacing.x;
                let btn_w = (avail - spacing * 3.0) / 4.0;
                ui.add_sized(egui::vec2(btn_w, ACTION_BUTTON_HEIGHT), egui::widgets::Button::new(RichText::new("Check for updates").strong()).fill(Color32::from_rgb(75,135,185)));
                ui.add_sized(egui::vec2(btn_w, ACTION_BUTTON_HEIGHT), egui::widgets::Button::new(RichText::new("Check").strong()).fill(Color32::from_rgb(190,120,90)));
                ui.add_sized(egui::vec2(btn_w, ACTION_BUTTON_HEIGHT), egui::widgets::Button::new(RichText::new("Launch").strong()).fill(Color32::from_rgb(120,200,140)));
                ui.add_sized(egui::vec2(btn_w, ACTION_BUTTON_HEIGHT), egui::widgets::Button::new(RichText::new("Join").strong()).fill(Color32::from_rgb(200,160,80)));
            });

            ui.add_space(6.0);

            // Inline settings (clean, single row)
            ui.horizontal(|ui| {
                ui.checkbox(&mut self.should_seed, "Enable seeding");
                ui.add_space(12.0);
                ui.label("Max upload (KB/s):");
                ui.add(egui::widgets::TextEdit::singleline(&mut self.upload_str).desired_width(80.0));
                ui.add_space(8.0);
                ui.label("Max download (KB/s):");
                ui.add(egui::widgets::TextEdit::singleline(&mut self.download_str).desired_width(80.0));
            });
        });

        // Central content (simple and uncluttered)
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(8.0);
                ui.heading("Main content area");
                ui.add_space(6.0);
                ui.label("Flexible content. Torrent progress is anchored to bottom.");
                ui.add_space(200.0);
            });
        });

        // Bottom fixed torrent progress panel (clean)
        egui::TopBottomPanel::bottom("progress_panel").exact_height(PROGRESS_PANEL_HEIGHT).show(ctx, |ui| {
            let width = ui.available_width();
            let bar_height = (PROGRESS_PANEL_HEIGHT - 26.0).max(12.0);
            let desired = Vec2::new(width, bar_height);

            // demo simulation if enabled
            if self.demo_mode {
                let base = vec![1u64 << 20, 5u64 << 20, 20u64 << 20];
                let total: u64 = base.iter().sum();
                let progress_total = ((total as f64) * self.demo_percent) as u64;
                let mut remaining = progress_total;
                let mut file_progress: Vec<u64> = Vec::with_capacity(base.len());
                for (i, &b) in base.iter().enumerate() {
                    if i + 1 == base.len() { file_progress.push(remaining); break; }
                    let part = ((b as f64 / total as f64) * (progress_total as f64)) as u64;
                    file_progress.push(part);
                    remaining = remaining.saturating_sub(part);
                }
                self.torrent_progress.update_from_simulated(file_progress, progress_total, total);
            }

            ui.centered_and_justified(|ui| {
                self.torrent_progress.ui(ui, desired);
            });
        });

        // keep updating
        ctx.request_repaint();
    }
}

impl ModApp {
    /// Accept managed torrent updates from the sync layer.
    pub fn on_managed_torrent_update(&mut self, stats_opt: Option<(usize, std::sync::Arc<librqbit::TorrentStats>)>) {
        if let Some((_id, stats)) = stats_opt {
            self.torrent_progress.update_from_stats(&stats);
        } else {
            self.torrent_progress = crate::ui::torrent_progress::TorrentProgress::new();
        }
    }
}