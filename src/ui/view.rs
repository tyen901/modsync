//! Rendering logic for the `modsync` terminal user interface.
//!
//! This module contains a single function, [`render`], which takes a
//! mutable reference to the application state and a frame to draw on.
//! The function constructs the layout (menu, config panel and log panel)
//! and renders widgets based on the current state.  Because it does not
//! modify state it can be treated as a pure view function as described
//! in the Elm Architecture.

use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Gauge, List, ListItem, Paragraph, Wrap},
    Frame,
};

use super::state::App;

/// Renders the UI for the given application state.  The provided frame
/// represents the full terminal window and the function is responsible
/// for splitting it into the appropriate regions and rendering
/// widgets into those regions.  The UI consists of a menu on the left
/// and two panels on the right: one showing the current configuration
/// and another showing the log.
pub fn render(f: &mut Frame, app: &App) {
    // Split the full screen horizontally into menu (20 columns) and main area.
    let size = f.area();
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(20), Constraint::Min(0)].as_ref())
        .split(size);

    // Build menu items with highlight on selection.
    let items: Vec<ListItem> = app
        .menu
        .iter()
        .enumerate()
        .map(|(i, m)| {
            // If a task is active, dim the entire menu to indicate it's disabled.
            let base = if app.current_task.is_some() {
                Style::default().fg(Color::Gray).add_modifier(Modifier::DIM)
            } else {
                Style::default()
            };
            let style = if i == app.selected {
                base.fg(Color::Yellow).add_modifier(Modifier::BOLD)
            } else {
                base
            };
            ListItem::new((*m).to_string()).style(style)
        })
        .collect();
    let menu_list = List::new(items).block(Block::default().title("Menu").borders(Borders::ALL));
    f.render_widget(menu_list, chunks[0]);

    // The right-hand area fills the remaining space.  It shows either
    // the current task (with stages) while a task is running, or the
    // modpack state/config information when idle.
    let right = chunks[1];

    if let Some(task) = &app.current_task {
        // Render task name and stages as a vertical list.
        let mut lines = vec![format!("Task: {}", task.name)];
        for (i, stage) in task.stages.iter().enumerate() {
            let status_str: String = match &task.stage_statuses[i] {
                super::state::TaskStageStatus::Pending => "[ ]".to_string(),
                super::state::TaskStageStatus::InProgress => "[~]".to_string(),
                super::state::TaskStageStatus::Done => "[x]".to_string(),
                super::state::TaskStageStatus::Failed(msg) => format!("[!] {}", msg),
            };
            lines.push(format!(" {} {}", status_str, stage));
        }

        // If there are files to show (download phase), render per-file progress
        // below stages. Only show active files (i.e. currently downloading), do
        // not show pending files. We will render either a single paragraph
        // (when no active files) or a vertical layout with stages, overall
        // gauge and per-file gauges when there are active downloads.
        let active_files: Vec<_> = task
            .files
            .iter()
            .filter(|f| {
                // Active = not completed and either we've received bytes or the
                // transfer has started.
                !f.completed && (f.bytes_received > 0 || f.started_at.is_some())
            })
            .collect();
        if active_files.is_empty() {
            // No active files: render the compact stages + file list as a single Paragraph
            let text = lines.join("\n");
            let widget = Paragraph::new(text)
                .block(
                    Block::default()
                        .title("Task Progress")
                        .borders(Borders::ALL),
                )
                .wrap(Wrap { trim: true });
            f.render_widget(widget, right);
        } else {
            // Build a vertical split: top = stages, middle = overall gauge, bottom = active files area.
            // Compute a reasonable stage height (header + stages). Let the layout manage wrapping.
            let stage_height = (1 + task.stages.len()) as u16 + 1; // header + stages + spacer
            let vchunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints(
                    [
                        Constraint::Length(stage_height),
                        Constraint::Length(3),
                        Constraint::Min(0),
                    ]
                    .as_ref(),
                )
                .split(right);

            // Stages in top chunk
            let mut stage_lines = vec![format!("Task: {}", task.name)];
            for (i, stage) in task.stages.iter().enumerate() {
                let status_str: String = match &task.stage_statuses[i] {
                    super::state::TaskStageStatus::Pending => "[ ]".to_string(),
                    super::state::TaskStageStatus::InProgress => "[~]".to_string(),
                    super::state::TaskStageStatus::Done => "[x]".to_string(),
                    super::state::TaskStageStatus::Failed(msg) => format!("[!] {}", msg),
                };
                stage_lines.push(format!(" {} {}", status_str, stage));
            }
            let stage_text = stage_lines.join("\n");
            let stage_widget = Paragraph::new(stage_text)
                .block(
                    Block::default()
                        .title("Task Progress")
                        .borders(Borders::ALL),
                )
                .wrap(Wrap { trim: true });
            f.render_widget(stage_widget, vchunks[0]);

            // Compute overall progress and render in middle chunk (vchunks[1])
            let files_total = task.files.len();
            let files_done = task.files.iter().filter(|ff| ff.completed).count();
            let bytes_total_opt: Option<u64> =
                task.files
                    .iter()
                    .map(|ff| ff.total)
                    .fold(Some(0u64), |acc, v| match (acc, v) {
                        (Some(a), Some(b)) => Some(a.saturating_add(b)),
                        _ => None,
                    });
            let bytes_done: u64 = task.files.iter().map(|ff| ff.bytes_received).sum();
            let overall_ratio = if let Some(total) = bytes_total_opt {
                if total == 0 {
                    0.0
                } else {
                    (bytes_done as f64 / total as f64).clamp(0.0, 1.0)
                }
            } else {
                if files_total == 0 {
                    0.0
                } else {
                    (files_done as f64 / files_total as f64).clamp(0.0, 1.0)
                }
            };
            let speed_suffix = task
                .overall_instant_bps
                .map(|bps| {
                    const KB: f64 = 1024.0;
                    const MB: f64 = KB * 1024.0;
                    let b_f = bps as f64;
                    if b_f >= MB {
                        format!(" ({:.2} MiB/s)", b_f / MB)
                    } else if b_f >= KB {
                        format!(" ({:.1} KiB/s)", b_f / KB)
                    } else {
                        format!(" ({} B/s)", bps)
                    }
                })
                .unwrap_or_default();
            let overall_label = if let Some(total) = bytes_total_opt {
                format!("Overall: {}/{} bytes{}", bytes_done, total, speed_suffix)
            } else {
                format!(
                    "Overall: {}/{} files{}",
                    files_done, files_total, speed_suffix
                )
            };
            let overall_gauge = Gauge::default()
                .block(
                    Block::default()
                        .title("Overall Progress")
                        .borders(Borders::ALL),
                )
                .gauge_style(
                    Style::default()
                        .fg(Color::Green)
                        .bg(Color::Black)
                        .add_modifier(Modifier::BOLD),
                )
                .ratio(overall_ratio)
                .label(overall_label);
            f.render_widget(overall_gauge, vchunks[1]);

            // Bottom chunk for active files: render a compact single-line list
            // of files (filename, short oid, progress / bytes, speed). Using a
            // single List avoids complex nested layouts per file which were
            // causing the overall layout to break when many files are present.
            let bottom = vchunks[2];
            // Build list items up to the available height.
            let avail_lines = bottom.height.saturating_sub(2) as usize; // leave room for borders/title
            let mut items: Vec<ListItem> = Vec::new();
            for fpr in active_files.iter().take(avail_lines.max(1)) {
                let fname = fpr
                    .dest
                    .file_name()
                    .and_then(|s| s.to_str())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| fpr.dest.display().to_string());
                let short_oid = if fpr.oid.len() > 8 {
                    &fpr.oid[..8]
                } else {
                    &fpr.oid
                };
                let progress_str = if let Some(total) = fpr.total {
                    let pct = if total == 0 {
                        0.0
                    } else {
                        (fpr.bytes_received as f64 / total as f64) * 100.0
                    };
                    format!("{:.1}% ({}/{})", pct, fpr.bytes_received, total)
                } else {
                    format!("{} bytes", fpr.bytes_received)
                };
                let speed_str = match fpr.instant_bps {
                    Some(bps) => {
                        const KB: f64 = 1024.0;
                        const MB: f64 = KB * 1024.0;
                        let b_f = bps as f64;
                        if b_f >= MB {
                            format!("{:.2} MiB/s", b_f / MB)
                        } else if b_f >= KB {
                            format!("{:.1} KiB/s", b_f / KB)
                        } else {
                            format!("{} B/s", bps)
                        }
                    }
                    None => String::new(),
                };
                let line = if speed_str.is_empty() {
                    format!("{} ({}) — {}", fname, short_oid, progress_str)
                } else {
                    format!(
                        "{} ({}) — {} — {}",
                        fname, short_oid, progress_str, speed_str
                    )
                };
                items.push(ListItem::new(line));
            }
            let list = List::new(items)
                .block(Block::default().title("Active files").borders(Borders::ALL));
            f.render_widget(list, bottom);
        }
    } else {
        // Idle view: display configuration and current modpack state.
        let arma_path = match &app.config.arma_executable {
            Some(p) => format!("{}", p.display()),
            None => String::from("<not set>"),
        };
        let mut lines = vec![
            format!("Repo URL: {}", app.config.repo_url),
            format!("Target mods: {}", app.config.target_mod_dir.display()),
            format!("Arma exe: {}", arma_path),
            String::new(),
            "Modpack state:".to_string(),
        ];
        lines.extend(app.modpack_state.clone());
        let text = lines.join("\n");
        let widget = Paragraph::new(text)
            .block(
                Block::default()
                    .title("Config / Modpack")
                    .borders(Borders::ALL),
            )
            .wrap(Wrap { trim: true });
        f.render_widget(widget, right);
    }
}
