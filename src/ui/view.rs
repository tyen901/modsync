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
            let style = if i == app.selected {
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            ListItem::new((*m).to_string()).style(style)
        })
        .collect();
    let menu_list = List::new(items).block(Block::default().title("Menu").borders(Borders::ALL));
    f.render_widget(menu_list, chunks[0]);

    // Split the right-hand area vertically into a Config panel (top)
    // and a Log panel (bottom).
    let right_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(7), Constraint::Min(0)].as_ref())
        .split(chunks[1]);

    // Build a textual representation of the current config.
    let arma_path = match &app.config.arma_executable {
        Some(p) => format!("{}", p.display()),
        None => String::from("<not set>"),
    };
    let config_lines = vec![
        format!("Repo URL: {}", app.config.repo_url),
        format!("Target mods: {}", app.config.target_mod_dir.display()),
        format!("Arma exe: {}", arma_path),
    ];
    let config_text = config_lines.join("\n");

    let config_widget = Paragraph::new(config_text)
        .block(Block::default().title("Config").borders(Borders::ALL))
        .wrap(Wrap { trim: true });
    f.render_widget(config_widget, right_chunks[0]);

    // Build log messages.
    let log_text = app.messages.join("\n");
    let log_widget = Paragraph::new(log_text)
        .block(Block::default().title("Log").borders(Borders::ALL))
        .wrap(Wrap { trim: false });
    f.render_widget(log_widget, right_chunks[1]);
}