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
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
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

        // If there are files to show (download phase), render per-file progress below stages.
        if !task.files.is_empty() {
            lines.push(String::new());
            lines.push("Files to download:".to_string());
            for f in task.files.iter() {
                let total_str = match f.total {
                    Some(t) => format!("{}/{} bytes", f.bytes_received, t),
                    None => format!("{} bytes", f.bytes_received),
                };
                // Small helper to format bytes/sec into human readable form.
                fn fmt_bps(b: u64) -> String {
                    const KB: f64 = 1024.0;
                    const MB: f64 = KB * 1024.0;
                    let b_f = b as f64;
                    if b_f >= MB {
                        format!("{:.2} MiB/s", b_f / MB)
                    } else if b_f >= KB {
                        format!("{:.1} KiB/s", b_f / KB)
                    } else {
                        format!("{} B/s", b)
                    }
                }

                let status = if f.completed {
                    match f.elapsed {
                        Some(dur) => format!("done ({:.1}s)", dur.as_secs_f64()),
                        None => "done".to_string(),
                    }
                } else if let Some(err) = &f.error {
                    format!("err: {}", err)
                } else if f.bytes_received == 0 {
                    "pending".to_string()
                } else {
                    match f.instant_bps {
                        Some(bps) => fmt_bps(bps),
                        None => "downloading".to_string(),
                    }
                };
                let short_oid = if f.oid.len() > 8 { &f.oid[..8] } else { &f.oid };
                lines.push(format!(" {} {} — {}", short_oid, total_str, status));
            }
        }

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
