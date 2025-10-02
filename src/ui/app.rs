use eframe::{egui, App, Frame};
use egui::{Color32, RichText};
use std::time::Instant;


use crate::ui::header::Header;
use crate::ui::settings_panel::SettingsPanel;
use crate::ui::file_graph::FileGraph;
use rfd::FileDialog;

// graph libs

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
    // graph data
    file_graph: FileGraph,
}

impl Default for ModApp {
    fn default() -> Self {
        Self {
            last_update: Instant::now(),
            settings_panel: SettingsPanel::default(),
            header: Header::default(),
            ui_state: UiState { url: String::new(), folder: String::from("downloads") },
            settings_open: false,
            file_graph: FileGraph::new(),
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

        egui::CentralPanel::default().show(ctx, |ui| {
            // header row
            ui.horizontal(|ui| {
                self.header.ui(ui, &mut self.settings_open);
            });

            ui.add_space(10.0);

            // Main content: left is main area, right is optional settings side panel
            ui.horizontal(|ui| {
                // Left: primary area - takes most space
                ui.vertical(|ui| {
                            // Layout the input row as three columns with equal flex
                            ui.horizontal(|ui| {
                                let avail = ui.available_width();
                                // three equal parts: label, input, button
                                let part = (avail - 24.0) / 3.0; // small padding
                                ui.add_sized(egui::vec2(part, 24.0), egui::Label::new("Torrent URL:"));
                                ui.add(egui::widgets::TextEdit::singleline(&mut self.ui_state.url).desired_width(part));
                                if ui.add_sized(egui::vec2(part, 30.0), egui::widgets::Button::new("Load").fill(Color32::from_rgb(70,130,180))).clicked() {
                                }
                            });

                    ui.add_space(6.0);

                    ui.horizontal(|ui| {
                        let avail = ui.available_width();
                        let part = (avail - 24.0) / 3.0;
                        ui.add_sized(egui::vec2(part, 24.0), egui::Label::new("Download Folder:"));
                        ui.add(egui::widgets::TextEdit::singleline(&mut self.ui_state.folder).desired_width(part));
                        if ui.add_sized(egui::vec2(part, 30.0), egui::widgets::Button::new("Browse").fill(Color32::from_rgb(100,160,100))).clicked() {
                            if let Some(folder) = FileDialog::new().pick_folder() {
                                self.ui_state.folder = folder.display().to_string();
                                self.file_graph.build_from_path(std::path::Path::new(&self.ui_state.folder));
                            }
                        }
                    });

                    ui.add_space(12.0);

                    // Four wide action buttons, evenly spaced
                    // Use egui Columns to ensure buttons are evenly sized and respect padding.
                    let cols = ui.columns(4, |columns| {
                        columns[0].add(egui::widgets::Button::new(RichText::new("Check for updates").strong()).fill(Color32::from_rgb(75,135,185)).min_size(egui::vec2(0.0,36.0)));
                        columns[1].add(egui::widgets::Button::new(RichText::new("Check").strong()).fill(Color32::from_rgb(190,120,90)).min_size(egui::vec2(0.0,36.0)));
                        columns[2].add(egui::widgets::Button::new(RichText::new("Launch").strong()).fill(Color32::from_rgb(120,200,140)).min_size(egui::vec2(0.0,36.0)));
                        columns[3].add(egui::widgets::Button::new(RichText::new("Join").strong()).fill(Color32::from_rgb(200,160,80)).min_size(egui::vec2(0.0,36.0)));
                    });

                    // Trigger clicks from columns (each returns response)
                    // We intentionally don't over-engineer actions here; keep as no-op placeholders.

                    ui.add_space(12.0);

                    // Graph area: reserved and renders file graph
                    egui::Frame::group(ui.style()).show(ui, |ui| {
                        let remaining = ui.available_height();
                        ui.set_min_height(remaining.max(220.0));

                        // draw graph via FileGraph
                        self.file_graph.ui(ui);
                    });
                });

                // Right: settings side panel as a popout
                if self.settings_open {
                    ui.add_space(8.0);
                    egui::SidePanel::right("settings_side").default_width(360.0).show_inside(ui, |ui| {
                        ui.vertical_centered(|ui| {
                            ui.heading("Settings");
                        });
                        self.settings_panel.ui(ui);
                    });
                }
            });
        });

        ctx.request_repaint_after(std::time::Duration::from_millis(16));
    }
}

impl ModApp {}
