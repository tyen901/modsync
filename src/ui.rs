//! Text user interface for `modsync`.
//!
//! This module drives a simple text user interface using the
//! [`ratatui`](https://docs.rs/ratatui) crate with a `crossterm` backend.  A
//! vertical menu on the left allows the user to trigger actions such as
//! synchronising the modpack, validating files, checking for updates and
//! launching the game.  A log window on the right records the progress of
//! operations.

use crate::arma;
use crate::config::Config;
use crate::gitutils;
use crate::modpack;

use anyhow::{Context, Result};
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
    Terminal,
};
use std::io::{self, Stdout};
use tokio::task;

/// Representation of the application state for the UI.
pub struct App {
    /// Current configuration loaded at startup.  Any changes are persisted
    /// when the application exits.
    pub config: Config,
    /// Messages to display in the log window.  This log is appended to
    /// whenever a background action produces output.
    messages: Vec<String>,
    /// Menu entries presented on the left of the screen.
    menu: Vec<&'static str>,
    /// Index of the currently selected menu entry.
    selected: usize,
    /// Sender channel used by background tasks to push log messages into
    /// the application.  Messages sent via this channel will appear in
    /// the log window on the next UI update.
    log_tx: tokio::sync::mpsc::UnboundedSender<String>,
    /// Receiver used by the UI loop to drain messages from background
    /// tasks.
    log_rx: tokio::sync::mpsc::UnboundedReceiver<String>,
}

impl App {
    /// Creates a new `App` instance from an existing configuration.  The
    /// initial menu contains the high level actions supported by the
    /// application.
    pub async fn new(config: Config) -> Result<Self> {
        let (log_tx, log_rx) = tokio::sync::mpsc::unbounded_channel();
        Ok(Self {
            config,
            messages: Vec::new(),
            menu: vec![
                "Sync Modpack",
                "Validate Files",
                "Check Updates",
                "Join Server",
                "Quit",
            ],
            selected: 0,
            log_tx,
            log_rx,
        })
    }

    /// Main event loop for the UI.  Handles rendering and input events
    /// asynchronously.  When the user chooses to quit the configuration
    /// is saved back to disk.
    pub async fn run(&mut self) -> Result<()> {
        enable_raw_mode().context("Failed to enable raw mode")?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen).context("Failed to enter alternate screen")?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend).context("Failed to create terminal backend")?;
        terminal.hide_cursor().context("Failed to hide cursor")?;

        let res = self.run_loop(&mut terminal).await;

        // Restore terminal state.
        disable_raw_mode().ok();
        execute!(terminal.backend_mut(), LeaveAlternateScreen).ok();
        terminal.show_cursor().ok();

        // Configuration is intentionally read-only at runtime; do not write
        // or overwrite the user's `config.txt` here.

        res
    }

    /// Internal loop that draws the UI and processes events.  Separated
    /// out to allow clean up in the outer method.
    async fn run_loop(&mut self, terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> Result<()> {
        loop {
            // Drain any pending log messages sent from background tasks.
            while let Ok(msg) = self.log_rx.try_recv() {
                self.messages.push(msg);
            }

                terminal
                .draw(|f| {
                    let size = f.area();
                    let chunks = Layout::default()
                        .direction(Direction::Horizontal)
                        .constraints([Constraint::Length(20), Constraint::Min(0)].as_ref())
                        .split(size);

                    // Build menu items with highlight on selection.
                    let items: Vec<ListItem> = self
                        .menu
                        .iter()
                        .enumerate()
                        .map(|(i, m)| {
                            let style = if i == self.selected {
                                Style::default()
                                    .fg(Color::Yellow)
                                    .add_modifier(Modifier::BOLD)
                            } else {
                                Style::default()
                            };
                            ListItem::new((*m).to_string()).style(style)
                        })
                        .collect();
                    let menu_list = List::new(items)
                        .block(Block::default().title("Menu").borders(Borders::ALL));
                    f.render_widget(menu_list, chunks[0]);

                    // Split the right-hand area into a Config panel (top)
                    // and a Log panel (bottom).
                    let right_chunks = Layout::default()
                        .direction(Direction::Vertical)
                        .constraints([Constraint::Length(7), Constraint::Min(0)].as_ref())
                        .split(chunks[1]);

                    // Build a textual representation of the current config.
                    let arma_path = match &self.config.arma_executable {
                        Some(p) => format!("{}", p.display()),
                        None => String::from("<not set>"),
                    };
                    let config_lines = vec![
                        format!("Repo URL: {}", self.config.repo_url),
                        format!("Local repo: {}", self.config.repo_cache_path().display()),
                        format!("Target mods: {}", self.config.target_mod_dir.display()),
                        format!("Arma exe: {}", arma_path),
                    ];
                    let config_text = config_lines.join("\n");

                    let config_widget = Paragraph::new(config_text)
                        .block(Block::default().title("Config").borders(Borders::ALL))
                        .wrap(Wrap { trim: true });
                    f.render_widget(config_widget, right_chunks[0]);

                    // Build log messages.
                    let log_text = self.messages.join("\n");
                    let log_widget = Paragraph::new(log_text)
                        .block(Block::default().title("Log").borders(Borders::ALL))
                        .wrap(Wrap { trim: false });
                    f.render_widget(log_widget, right_chunks[1]);
                })
                .context("Failed to draw UI")?;

            // Handle input events.  We don't use a timeout here so the
            // application will block until an event is available.  For a
            // responsive UI you could poll with a timeout instead.
            match event::read().context("Failed to read terminal event")? {
                Event::Key(key) => {
                    // On some platforms/crossterm versions key events are emitted
                    // for both press and release. Only handle "Press" and
                    // repeated key events to avoid processing the same logical
                    // keypress twice.
                    match key.kind {
                        KeyEventKind::Press | KeyEventKind::Repeat => match key.code {
                            KeyCode::Up => {
                                if self.selected > 0 {
                                    self.selected -= 1;
                                }
                            }
                            KeyCode::Down => {
                                if self.selected + 1 < self.menu.len() {
                                    self.selected += 1;
                                }
                            }
                            KeyCode::Enter => {
                                // Clone self.selected to avoid borrow issues in async closure.
                                let idx = self.selected;
                                self.execute_menu(idx).await?;
                            }
                            KeyCode::Char('q') => {
                                break;
                            }
                            _ => {}
                        },
                        _ => {
                            // Ignore KeyEventKind::Release and other kinds.
                        }
                    }
                }
                Event::Resize(_, _) => {
                    // A resize triggers a redraw on the next iteration.
                }
                _ => {}
            }
        }
        Ok(())
    }

    /// Dispatches the selected menu entry.  This method spawns blocking
    /// operations on a threadpool so as not to block the async event loop.
    async fn execute_menu(&mut self, idx: usize) -> Result<()> {
        match self.menu.get(idx).copied() {
            Some("Sync Modpack") => {
                self.log("Starting sync...");
                let config = self.config.clone();
                let log_tx = self.log_tx.clone();
                task::spawn_blocking(move || {
                    let repo_path = config.repo_cache_path();
                    match gitutils::clone_or_open_repo(&config.repo_url, &repo_path) {
                        Ok(repo) => {
                            let _ = gitutils::fetch(&repo);
                            match modpack::sync_modpack(
                                &repo_path,
                                &config.target_mod_dir,
                            ) {
                                Ok(()) => {
                                    let _ = log_tx.send("Sync complete".to_string());
                                }
                                Err(e) => {
                                    let _ = log_tx.send(format!("Sync failed: {e}"));
                                }
                            }
                        }
                        Err(e) => {
                            let _ = log_tx.send(format!("Failed to clone or open repository: {e}"));
                        }
                    }
                });
            }
            Some("Validate Files") => {
                self.log("Validating files...");
                let config = self.config.clone();
                let log_tx = self.log_tx.clone();
                task::spawn_blocking(move || {
                    let repo_path = config.repo_cache_path();
                    match modpack::validate_modpack(&repo_path, &config.target_mod_dir)
                    {
                        Ok(mismatches) => {
                            if mismatches.is_empty() {
                                let _ = log_tx.send("All files are valid".to_string());
                            } else {
                                let msg = format!("{} file(s) need healing", mismatches.len());
                                let _ = log_tx.send(msg);
                                for m in mismatches.iter().take(10) {
                                    let _ = log_tx.send(format!("- {}", m.display()));
                                }
                                if mismatches.len() > 10 {
                                    let _ = log_tx.send("...".to_string());
                                }
                            }
                        }
                        Err(e) => {
                            let _ = log_tx.send(format!("Validation failed: {e}"));
                        }
                    }
                });
            }
            Some("Check Updates") => {
                self.log("Checking for updates...");
                let config = self.config.clone();
                let log_tx = self.log_tx.clone();
                task::spawn_blocking(move || {
                    let repo_path = config.repo_cache_path();
                    match gitutils::clone_or_open_repo(&config.repo_url, &repo_path) {
                        Ok(repo) => {
                            let before = gitutils::head_oid(&repo).ok();
                            let _ = gitutils::fetch(&repo);
                            let after = gitutils::head_oid(&repo).ok();
                            match (before, after) {
                                (Some(b), Some(a)) => {
                                    if b != a {
                                        let _ = log_tx.send("Update available".to_string());
                                    } else {
                                        let _ = log_tx.send("Up to date".to_string());
                                    }
                                }
                                _ => {
                                    let _ = log_tx
                                        .send("Could not determine update status".to_string());
                                }
                            }
                        }
                        Err(e) => {
                            let _ = log_tx.send(format!("Failed to check updates: {e}"));
                        }
                    }
                });
            }
            Some("Join Server") => {
                self.log("Preparing to join server...");
                let config = self.config.clone();
                let log_tx = self.log_tx.clone();
                task::spawn_blocking(move || match config.read_metadata() {
                    Ok(Some(meta)) => {
                        let arma_path = config.arma_executable.or_else(arma::detect_arma_path);
                        match arma_path {
                            Some(path) => match arma::launch_arma(&path, &meta) {
                                Ok(()) => {
                                    let _ = log_tx.send(format!(
                                        "Launched Arma at {} and connected to {}:{}",
                                        path.display(),
                                        meta.address,
                                        meta.port
                                    ));
                                }
                                Err(e) => {
                                    let _ = log_tx.send(format!("Failed to launch Arma: {e}"));
                                }
                            },
                            None => {
                                let _ = log_tx
                                    .send("Could not determine Arma executable path".to_string());
                            }
                        }
                    }
                    Ok(None) => {
                        let _ = log_tx.send("metadata.json not found in repository".to_string());
                    }
                    Err(e) => {
                        let _ = log_tx.send(format!("Failed to read metadata: {e}"));
                    }
                });
            }
            Some("Quit") => {
                // Do not write configuration on quit; exit immediately.
                std::process::exit(0);
            }
            _ => {}
        }
        Ok(())
    }

    /// Appends a line to the log.  Use this for quick messages from the
    /// asynchronous context; more complex actions should use the channel
    /// returned by [`create_logger`] instead.
    fn log(&mut self, msg: &str) {
        self.messages.push(msg.to_string());
    }
}

pub fn attach_downloader_consumer<F>(
    items: Vec<crate::downloader::LfsDownloadItem>,
    cfg: crate::downloader::DownloaderConfig,
    on_event: F,
) -> std::sync::mpsc::Sender<crate::downloader::ControlCommand>
where
    F: Fn(crate::downloader::ProgressEvent) + Send + 'static,
{
    let (progress_rx, control_tx, join_handle) = crate::downloader::start_download_job(items, cfg);

    std::thread::spawn(move || {
        while let Ok(ev) = progress_rx.recv() {
            on_event(ev);
        }
    });

    std::thread::spawn(move || {
        let _ = join_handle.join();
    });

    control_tx
}
