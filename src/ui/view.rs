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
        let text = lines.join("\n");
        let widget = Paragraph::new(text)
            .block(Block::default().title("Task Progress").borders(Borders::ALL))
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
            .block(Block::default().title("Config / Modpack").borders(Borders::ALL))
            .wrap(Wrap { trim: true });
        f.render_widget(widget, right);
    }
}