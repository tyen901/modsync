use crate::settings::AppSettings;
use eframe::egui;
use egui::{RichText, Color32};

#[derive(Default)]
pub struct SettingsPanel {
    pub open: bool,
    pub url_str: String,
    pub upload_str: String,
    pub download_str: String,
    pub path_str: String,
    pub save_message: Option<String>,
    pub should_seed: bool,
}

impl SettingsPanel {
    pub fn ui(&mut self, ui: &mut egui::Ui) {
        // lazy load if needed
        if self.save_message.is_none() && self.path_str.is_empty() {
            if let Ok(s) = AppSettings::load() {
                self.url_str = s.torrent_url.clone();
                self.upload_str = s.max_upload_speed.map(|v| v.to_string()).unwrap_or_default();
                self.download_str = s.max_download_speed.map(|v| v.to_string()).unwrap_or_default();
                self.path_str = s.download_path.to_string_lossy().to_string();
                self.should_seed = s.should_seed;
            }
        }

        // Side panel friendly layout
        egui::ScrollArea::vertical().show(ui, |ui| {
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.vertical(|ui| {
                    ui.label(RichText::new("Application Settings").heading());
                    ui.add_space(6.0);

                    ui.horizontal(|ui| {
                        ui.label("Torrent URL:");
                        let url_widget = egui::widgets::TextEdit::singleline(&mut self.url_str).desired_width(260.0);
                        ui.add(url_widget);
                    });

                    ui.horizontal(|ui| {
                        ui.label("Download path:");
                        let path_widget = egui::widgets::TextEdit::singleline(&mut self.path_str).desired_width(220.0);
                        ui.add(path_widget);
                    });

                    ui.separator();

                    ui.horizontal(|ui| {
                        ui.label("Seeding:");
                        ui.checkbox(&mut self.should_seed, "Enable seeding");
                    });

                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.label("Max upload (KB/s):");
                            ui.add(egui::widgets::TextEdit::singleline(&mut self.upload_str).desired_width(140.0));
                        });
                        ui.add_space(8.0);
                        ui.vertical(|ui| {
                            ui.label("Max download (KB/s):");
                            ui.add(egui::widgets::TextEdit::singleline(&mut self.download_str).desired_width(140.0));
                        });
                    });

                    ui.add_space(6.0);

                    ui.horizontal(|ui| {
                        if ui.add(egui::widgets::Button::new("Save").fill(Color32::from_rgb(80, 160, 120))).clicked() {
                            let mut settings = AppSettings::load().unwrap_or_default();
                            settings.max_upload_speed = if self.upload_str.trim().is_empty() { None } else { self.upload_str.trim().parse::<u32>().ok() };
                            settings.max_download_speed = if self.download_str.trim().is_empty() { None } else { self.download_str.trim().parse::<u32>().ok() };
                            settings.download_path = std::path::PathBuf::from(self.path_str.clone());
                            settings.torrent_url = self.url_str.clone();
                            settings.should_seed = self.should_seed;
                            match settings.save() {
                                Ok(()) => self.save_message = Some("Settings saved".to_string()),
                                Err(e) => self.save_message = Some(format!("Failed to save settings: {}", e)),
                            }
                        }

                        if ui.add(egui::widgets::Button::new("Reset").fill(Color32::from_rgb(160, 80, 80))).clicked() {
                            match AppSettings::reset() {
                                Ok(()) => {
                                    self.url_str.clear();
                                    self.upload_str.clear();
                                    self.download_str.clear();
                                    self.path_str.clear();
                                    self.should_seed = AppSettings::default().should_seed;
                                    self.save_message = Some("Settings reset to defaults".to_string());
                                }
                                Err(e) => self.save_message = Some(format!("Failed to reset settings: {}", e)),
                            }
                        }

                        if ui.add(egui::widgets::Button::new("Close")).clicked() {
                            self.open = false;
                        }
                    });

                    if let Some(msg) = &self.save_message {
                        ui.colored_label(Color32::from_rgb(210, 180, 140), msg);
                    }
                });
            });
        });
    }
}
