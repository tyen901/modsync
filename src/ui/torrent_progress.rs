use eframe::egui;
use egui::{Color32, Vec2, Pos2, Rect, CornerRadius};
use std::time::Instant;
use librqbit::TorrentStats;

/// UI component that renders aggregate + per-file torrent progress.
pub struct TorrentProgress {
    file_progress: Vec<u64>,
    progress_bytes: u64,
    total_bytes: u64,
    last_update: std::time::Instant,
}

impl TorrentProgress {
    /// Create an empty progress widget.
    pub fn new() -> Self {
        Self {
            file_progress: Vec::new(),
            progress_bytes: 0,
            total_bytes: 0,
            last_update: Instant::now(),
        }
    }

    /// Update the widget from canonical stats.
    pub fn update_from_stats(&mut self, stats: &TorrentStats) {
        self.file_progress = stats.file_progress.clone();
        self.progress_bytes = stats.progress_bytes;
        self.total_bytes = stats.total_bytes;
        self.last_update = Instant::now();
    }

    /// Temporary helper used by the UI demo: directly set internal fields from
    /// simulated values without requiring a full `TorrentStats` instance.
    pub fn update_from_simulated(&mut self, file_progress: Vec<u64>, progress_bytes: u64, total_bytes: u64) {
        self.file_progress = file_progress;
        self.progress_bytes = progress_bytes;
        self.total_bytes = total_bytes;
        self.last_update = Instant::now();
    }

    /// Render the widget into the provided `ui` using the requested `desired_size`.
    ///
    /// Behavior:
    /// - Draws a rounded background bar filling `desired_size`.
    /// - Draws one colored segment per file showing its contribution to torrent total.
    /// - On hover over a segment shows a tooltip: "File N: X / total bytes".
    /// - Above the bar renders overall percentage and human-readable bytes.
    pub fn ui(&mut self, ui: &mut egui::Ui, desired_size: egui::Vec2) {
    use egui::Sense;

        // Header: overall percent and human-readable bytes
        let percent = if self.total_bytes > 0 {
            (self.progress_bytes as f64 / self.total_bytes as f64) * 100.0
        } else {
            0.0
        };
        let header_text = format!(
            "{:.2}% — {} / {}",
            percent,
            human_readable_bytes(self.progress_bytes),
            human_readable_bytes(self.total_bytes)
        );

        // Reserve the header area first so the caller can provide the full
        // desired_size (header + bar). Use a small header height to keep layout stable.
        let header_height: f32 = 20.0;
        ui.horizontal(|ui| {
            ui.set_min_size(egui::vec2(desired_size.x, header_height));
            ui.label(header_text);
        });

        // Compute bar area from desired_size minus header
    let bar_height = (desired_size.y - header_height).max(8.0);
    let bar_size = Vec2::new(desired_size.x, bar_height);

    // Allocate exact size for the bar and get the painter
    let (rect, _alloc_resp) = ui.allocate_exact_size(bar_size, egui::Sense::hover());
    let painter = ui.painter();

    // Background bar (rounded corners)
    let bar_radius: u8 = 8;
    let bar_fill = Color32::from_rgb(30, 30, 30);
    painter.rect_filled(rect, CornerRadius::same(bar_radius), bar_fill);

        // Draw nothing else if there are no files (header still shown)
        let files = &self.file_progress;
        let n_files = files.len();
        if n_files == 0 {
            return;
        }

        let total_bytes_f = self.total_bytes as f64;
        let bar_width = rect.width();
        let bar_height = rect.height();

        // Precompute segment widths (single pass)
        let mut seg_widths: Vec<f32> = Vec::with_capacity(n_files);
        if total_bytes_f == 0.0 {
            // Divide evenly if total unknown
            let w = bar_width / (n_files as f32);
            seg_widths.resize(n_files, w);
        } else {
            for &fp in files.iter() {
                let ratio = (fp as f64) / total_bytes_f;
                let w = (ratio * (bar_width as f64)) as f32;
                seg_widths.push(w.max(1.0)); // ensure visible
            }
        }

        // Draw each segment and attach hover for tooltip
        let mut x = rect.left();
        for (i, &w) in seg_widths.iter().enumerate() {
            let seg_rect = Rect::from_min_size(Pos2::new(x, rect.top()), Vec2::new(w, bar_height));

            // Color: greenish if effectively complete, otherwise orange
            let color = if self.total_bytes > 0 {
                let ratio = (files[i] as f64) / (self.total_bytes as f64);
                if ratio >= 0.999 {
                    Color32::from_rgb(100, 200, 120) // greenish
                } else {
                    Color32::from_rgb(240, 150, 60) // orange
                }
            } else {
                Color32::from_rgb(100, 200, 120)
            };

            // Draw segment filled
            painter.rect_filled(seg_rect, CornerRadius::same(bar_radius), color);

            // Interaction: create a hover response for the exact segment rectangle
            let id = ui.id().with((self.last_update.elapsed().as_nanos(), i));
            let response = ui.interact(seg_rect, id, Sense::hover());
            if response.hovered() {
                let file_bytes = files[i];
                let pct = if self.total_bytes > 0 {
                    (file_bytes as f64 / self.total_bytes as f64) * 100.0
                } else {
                    0.0
                };
                let tooltip = format!(
                    "File {}: {} / {} ({:.2}%)",
                    i,
                    human_readable_bytes(file_bytes),
                    human_readable_bytes(self.total_bytes),
                    pct
                );
                response.on_hover_text(tooltip);
            }

            x += w;
        }
    }
}
/// Simple helper to format bytes in KiB/MiB/GiB with two decimal places.
fn human_readable_bytes(b: u64) -> String {
    let b_f = b as f64;
    const KIB: f64 = 1024.0;
    const MIB: f64 = KIB * 1024.0;
    const GIB: f64 = MIB * 1024.0;

    if b_f >= GIB {
        format!("{:.2} GiB", b_f / GIB)
    } else if b_f >= MIB {
        format!("{:.2} MiB", b_f / MIB)
    } else if b_f >= KIB {
        format!("{:.2} KiB", b_f / KIB)
    } else {
        format!("{} B", b)
    }
}