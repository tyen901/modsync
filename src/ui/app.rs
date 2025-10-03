use eframe::{egui, App, Frame};
use egui::{Color32, RichText};
use std::time::Instant;

use crate::ui::header::Header;
use crate::ui::settings_panel::SettingsPanel;
use rfd::FileDialog;

// UI-local state
struct UiState {
    url: String,
    folder: String,
}

pub struct ModApp {
    last_update: Instant,
    settings_panel: SettingsPanel,
    header: Header,
    ui_state: UiState,
    settings_open: bool,
}

impl Default for ModApp {
    fn default() -> Self {
        Self {
            last_update: Instant::now(),
            settings_panel: SettingsPanel::default(),
            header: Header::default(),
            ui_state: UiState { url: String::new(), folder: String::from("downloads") },
            settings_open: false,
        }
    }
}

impl App for ModApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut Frame) {
        // update timing
        self.last_update = Instant::now();

        // modern style
        let mut style = (*ctx.style()).clone();
        style.visuals.widgets.inactive.bg_fill = Color32::from_rgb(24, 24, 26);
        style.visuals.window_fill = Color32::from_rgb(10, 10, 12);
        style.visuals.override_text_color = Some(Color32::from_rgb(235, 235, 235));
        style.spacing.item_spacing = egui::vec2(10.0, 6.0);
        style.spacing.button_padding = egui::vec2(12.0, 8.0);
        style.text_styles.get_mut(&egui::TextStyle::Heading).map(|ts| ts.size = 30.0);
        style.text_styles.get_mut(&egui::TextStyle::Body).map(|ts| ts.size = 15.0);
        ctx.set_style(style);

        // Combined header + controls panel (fixed height, auto sized by contents)
        egui::TopBottomPanel::top("controls_panel").show(ctx, |ui| {
            ui.horizontal(|ui| { self.header.ui(ui, &mut self.settings_open); });
            ui.add_space(4.0);

            // URL row
            ui.horizontal(|ui| {
                let avail = ui.available_width();
                let spacing = ui.spacing().item_spacing.x;
                let label_w = (avail * 0.20).min(160.0).max(80.0);
                let btn_w = (avail * 0.18).min(120.0).max(80.0);
                let input_w = (avail - label_w - btn_w - spacing * 2.0).max(80.0);
                ui.add_sized(egui::vec2(label_w, 24.0), egui::Label::new("Torrent URL:"));
                ui.add_sized(egui::vec2(input_w, 24.0), egui::widgets::TextEdit::singleline(&mut self.ui_state.url));
                if ui.add_sized(egui::vec2(btn_w, 30.0), egui::widgets::Button::new("Load").fill(Color32::from_rgb(70,130,180))).clicked() {}
            });

            ui.add_space(4.0);
            // Folder row
            ui.horizontal(|ui| {
                let avail = ui.available_width();
                let spacing = ui.spacing().item_spacing.x;
                let label_w = (avail * 0.20).min(160.0).max(80.0);
                let btn_w = (avail * 0.18).min(120.0).max(80.0);
                let input_w = (avail - label_w - btn_w - spacing * 2.0).max(80.0);
                ui.add_sized(egui::vec2(label_w, 24.0), egui::Label::new("Download Folder:"));
                ui.add_sized(egui::vec2(input_w, 24.0), egui::widgets::TextEdit::singleline(&mut self.ui_state.folder));
                if ui.add_sized(egui::vec2(btn_w, 30.0), egui::widgets::Button::new("Browse").fill(Color32::from_rgb(100,160,100))).clicked() {
                    if let Some(folder) = FileDialog::new().pick_folder() {
                        self.ui_state.folder = folder.display().to_string();
                    }
                }
            });

            ui.add_space(8.0);
            // Action buttons
            let avail_btn_w = ui.available_width();
            let spacing = ui.spacing().item_spacing.x.max(6.0);
            let btn_w = (avail_btn_w - spacing * 3.0).max(0.0) / 4.0;
            ui.horizontal(|ui| {
                ui.add_sized(egui::vec2(btn_w, 36.0), egui::widgets::Button::new(RichText::new("Check for updates").strong()).fill(Color32::from_rgb(75,135,185)));
                ui.add_space(spacing);
                ui.add_sized(egui::vec2(btn_w, 36.0), egui::widgets::Button::new(RichText::new("Check").strong()).fill(Color32::from_rgb(190,120,90)));
                ui.add_space(spacing);
                ui.add_sized(egui::vec2(btn_w, 36.0), egui::widgets::Button::new(RichText::new("Launch").strong()).fill(Color32::from_rgb(120,200,140)));
                ui.add_space(spacing);
                ui.add_sized(egui::vec2(btn_w, 36.0), egui::widgets::Button::new(RichText::new("Join").strong()).fill(Color32::from_rgb(200,160,80)));
            });
            ui.add_space(4.0);
            ui.separator();
        });

        // Optional settings side panel (does not overlap central graph)
        if self.settings_open {
            egui::SidePanel::right("settings_side").resizable(true).default_width(360.0).show(ctx, |ui| {
                ui.vertical_centered(|ui| { ui.heading("Settings"); });
                ui.separator();
                self.settings_panel.ui(ui);
            });
        }

        // Central panel exclusively for the graph (auto fills remaining space)
        egui::CentralPanel::default().show(ctx, |ui| {
            let avail = ui.available_size();
            ui.set_min_size(avail); // ensure we claim all
            egui::Frame::group(ui.style()).show(ui, |ui| {
                let inner = ui.available_size();
                ui.set_min_size(inner);
                ui.centered_and_justified(|ui| {
                    ui.label("File graph removed");
                });
            });
        });

        // Request continuous repaint (immediate) so update() is called repeatedly.
        ctx.request_repaint();
    }
}

impl ModApp {}
