use eframe::egui;
use egui::{RichText, Color32};

#[derive(Default)]
pub struct Header {}

impl Header {
    pub fn ui(&self, ui: &mut egui::Ui, settings_open: &mut bool) {
        ui.vertical(|ui| {
            ui.add_space(6.0);
            let banner = RichText::new("ModSync").size(34.0).strong().color(Color32::from_rgb(180, 255, 200));
            ui.heading(banner);
            ui.label(RichText::new("Background sync manager").color(Color32::from_rgb(160, 160, 170)));
        });

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.horizontal(|ui| {
                if ui.add(egui::widgets::Button::new("Settings").fill(Color32::from_rgb(80, 140, 120))).clicked() {
                    *settings_open = !*settings_open;
                }
            });
        });
    }
}
