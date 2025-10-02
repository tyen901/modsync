use eframe::{egui, App, Frame};
use egui::{RichText, Color32, Stroke};
use std::time::Instant;

pub struct MinimalApp {
    spinner_angle: f32,
    initialized: bool,
    last_update: Instant,
}

impl Default for MinimalApp {
    fn default() -> Self {
        Self {
            spinner_angle: 0.0,
            initialized: false,
            last_update: Instant::now(),
        }
    }
}

impl App for MinimalApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut Frame) {
        // Initialize style once
        if !self.initialized {
            let mut style = (*ctx.style()).clone();
            style.visuals.widgets.inactive.bg_fill = Color32::from_rgb(18, 18, 18);
            style.visuals.window_fill = Color32::from_rgb(12, 12, 14);
            style.visuals.override_text_color = Some(Color32::from_rgb(230, 230, 230));
            style.spacing.item_spacing = egui::vec2(12.0, 8.0);
            style.text_styles.get_mut(&egui::TextStyle::Heading).map(|ts| ts.size = 28.0);
            style.text_styles.get_mut(&egui::TextStyle::Body).map(|ts| ts.size = 16.0);
            if let Some(ts) = style.text_styles.get_mut(&egui::TextStyle::Monospace) {
                ts.size = 14.0;
            }
            ctx.set_style(style);
            self.initialized = true;
        }

        // Advance spinner using stable Instant delta
        let now = Instant::now();
        let dt = now.duration_since(self.last_update).as_secs_f32();
        self.last_update = now;
        // 180 degrees per second rotation speed
        self.spinner_angle += 180.0 * dt;
        if self.spinner_angle > 360.0 {
            self.spinner_angle -= 360.0;
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(12.0);
                let banner = RichText::new("ModSync").size(34.0).strong().color(Color32::from_rgb(180, 255, 200));
                ui.heading(banner);
                ui.add_space(6.0);
                ui.label(RichText::new("Background sync manager").color(Color32::from_rgb(160, 160, 170)));
                ui.add_space(18.0);

                // Spinner
                let (rect, _response) = ui.allocate_exact_size(egui::vec2(64.0, 64.0), egui::Sense::hover());
                let painter = ui.painter_at(rect);

                let center = rect.center();
                let radius = 26.0f32;

                // Draw a faint ring
                painter.circle_stroke(center, radius, Stroke::new(3.0, Color32::from_rgb(50, 50, 60)));

                // Draw a small filled circle moving around the ring as the spinner indicator
                let angle_rad = self.spinner_angle.to_radians();
                let indicator_pos = center + egui::Vec2::new(angle_rad.cos() * radius, angle_rad.sin() * radius);
                painter.circle_filled(indicator_pos, 6.0, Color32::from_rgb(110, 210, 160));

                ui.add_space(12.0);
                ui.label(RichText::new("Syncing...").color(Color32::from_rgb(200, 220, 200)).size(16.0));

                ui.add_space(12.0);
                ui.separator();
                ui.add_space(8.0);
                ui.horizontal_centered(|ui| {
                    ui.label(RichText::new("Minimal UI — no actions implemented").color(Color32::from_rgb(140, 140, 150)));
                });
            });
        });

        // Request repaint for animation
        ctx.request_repaint_after(std::time::Duration::from_millis(16));
    }
}

/// Run the native UI. This is exposed for multiple binaries to reuse.
pub fn run_ui() {
    let native_options = eframe::NativeOptions::default();
    eframe::run_native(
        "ModSync",
        native_options,
        Box::new(|_cc| Ok(Box::new(MinimalApp::default()) as Box<dyn eframe::App>)),
    )
    .expect("Failed to start UI");
}
