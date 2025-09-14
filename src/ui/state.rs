//! Defines the application state for the text user interface.
//!
//! The `App` structure holds all mutable state required by the UI at
//! runtime, including the configuration loaded at startup, the list of
//! log messages to display, the menu entries and the current selection.
//! It also contains an unbounded channel used by background tasks to
//! send log messages back to the UI.

use crate::config::Config;
use anyhow::Result;

/// Application state for the terminal UI.
#[derive(Debug)]
pub struct App {
    /// Current configuration loaded at startup.  Changes made via the UI
    /// are retained in this copy and can be saved by the caller after
    /// the UI exits.
    pub config: Config,
    /// Messages to display in the log window.  Background tasks push
    /// messages into this vector via the channel below and the event
    /// loop drains them on each iteration.
    pub messages: Vec<String>,
    /// Menu entries presented on the left of the screen.
    pub menu: Vec<&'static str>,
    /// Index of the currently selected menu entry.
    pub selected: usize,
    /// Sender used by background tasks to push log messages back to the
    /// UI.  See [`log_tx`] for sending messages.
    pub log_tx: tokio::sync::mpsc::UnboundedSender<String>,
    /// Receiver used by the event loop to drain messages from
    /// background tasks.
    pub log_rx: tokio::sync::mpsc::UnboundedReceiver<String>,
}

impl App {
    /// Creates a new application state from an existing configuration.
    /// This also allocates an unbounded channel for background tasks to
    /// send log messages to the UI.  The menu contains the high level
    /// actions supported by the application.
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

    /// Appends a line to the in‑memory log.  Use this for quick
    /// messages that originate from the async context.  More complex
    /// actions should use the `log_tx` channel to send messages back
    /// into the UI.
    pub fn log(&mut self, msg: &str) {
        self.messages.push(msg.to_string());
    }
}