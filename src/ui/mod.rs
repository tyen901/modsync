//! Public interface for the `modsync` terminal user interface.
//!
//! The user interface is now broken into several smaller modules.  The
//! [`state`] module defines the `App` structure containing all of the
//! application state.  The [`view`] module contains a pure rendering
//! function that converts the state into terminal widgets.  The
//! [`event`] module handles terminal events and updates the state
//! accordingly.  Finally, the [`actions`] module encapsulates long‑running
//! operations such as synchronising the modpack, validating files,
//! checking for updates and launching Arma.  See each submodule for
//! further details.

pub mod state;
pub mod view;
pub mod event;
pub mod actions;

use anyhow::Result;
use ratatui::{Terminal, backend::CrosstermBackend};
use std::io::Stdout;

/// Convenience re‑export so callers only need to import `modsync::ui::App`.
pub use state::App;

/// Main entry point into the terminal user interface.  This method
/// initialises the application state and then enters an event loop.  The
/// loop repeatedly draws the UI and processes input until the user
/// chooses to quit.  When the loop exits the function returns and
/// drops back to the caller without persisting any state; the caller
/// should persist the configuration if required.
impl App {
    /// Runs the text user interface.  This method enables raw mode,
    /// switches to the alternate screen and hides the cursor before
    /// entering the event loop.  On exit it restores the terminal
    /// settings.  Returning an error from this method will restore the
    /// terminal state before propagating the error.
    pub async fn run(&mut self) -> Result<()> {
    use crossterm::{execute, terminal::{EnterAlternateScreen, LeaveAlternateScreen, enable_raw_mode, disable_raw_mode}};
    use std::io;

        // Enable raw mode and switch to the alternate screen.
        enable_raw_mode().context("Failed to enable raw mode")?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen).context("Failed to enter alternate screen")?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend).context("Failed to create terminal backend")?;
        terminal.hide_cursor().context("Failed to hide cursor")?;

        // Run the internal event loop.  If an error occurs it will be
        // captured and returned after the terminal state has been
        // restored.
        let res = self.run_loop(&mut terminal).await;

        // Restore the terminal state.
        disable_raw_mode().ok();
        execute!(terminal.backend_mut(), LeaveAlternateScreen).ok();
        terminal.show_cursor().ok();

        res
    }

    /// Internal event loop.  Drains incoming log messages, draws the
    /// current state and processes input events.  The loop terminates
    /// when [`event::handle_event`] signals to exit.
    async fn run_loop(&mut self, terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> Result<()> {
        use std::time::Duration;
        use crossterm::event as crossterm_event;

        loop {
            // Drain any pending log messages sent from background tasks.
            while let Ok(msg) = self.log_rx.try_recv() {
                self.messages.push(msg);
            }

            // Draw the current state.
            terminal
                .draw(|f| {
                    view::render(f, self);
                })
                .context("Failed to draw UI")?;

            // Poll for an event with a short timeout.  A timeout allows
            // background messages to be displayed even when the user is
            // inactive.
            let timeout = Duration::from_millis(100);
            if crossterm_event::poll(timeout).context("Failed to poll terminal event")? {
                let ev = crossterm_event::read().context("Failed to read terminal event")?;
                // Handle the event.  If it returns false we break the loop.
                let continue_running = event::handle_event(self, ev).await?;
                if !continue_running {
                    break;
                }
            }
        }
        Ok(())
    }
}

/// Attaches a consumer to a download job.  This helper spawns a thread
/// that listens for progress events on the provided channel and
/// forwards them to the given closure.  It returns a sender for
/// control commands that can be used to pause or cancel the download.
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

// Bring anyhow's Context trait into scope for error messages.
use anyhow::Context;