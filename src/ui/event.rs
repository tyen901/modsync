//! Input handling for the terminal user interface.
//!
//! This module converts raw terminal events into updates on the
//! application state.  It exposes a single asynchronous function
//! [`handle_event`] that takes the current state and an event and
//! returns a boolean indicating whether to continue running.  When
//! certain keys are pressed (e.g. the Enter key) it delegates to the
//! [`actions`] module to perform the selected operation.

use anyhow::Result;
use crossterm::event::{Event, KeyCode, KeyEventKind};
use super::state::App;
use super::actions;

/// Processes a single terminal event and updates the application state
/// accordingly.  Returns `Ok(true)` if the event loop should
/// continue running or `Ok(false)` if it should exit.  Errors
/// encountered while executing long‑running actions are returned.
pub async fn handle_event(app: &mut App, ev: Event) -> Result<bool> {
    match ev {
        Event::Key(key) => {
            // On some platforms/crossterm versions key events are emitted
            // for both press and release. Only handle "Press" and
            // repeated key events to avoid processing the same logical
            // keypress twice.
            match key.kind {
                KeyEventKind::Press | KeyEventKind::Repeat => match key.code {
                    KeyCode::Up => {
                        // Only allow menu navigation when no task is running.
                        if app.current_task.is_none() {
                            if app.selected > 0 {
                                app.selected -= 1;
                            }
                        }
                    }
                    KeyCode::Down => {
                        if app.current_task.is_none() {
                            if app.selected + 1 < app.menu.len() {
                                app.selected += 1;
                            }
                        }
                    }
                    KeyCode::Enter => {
                        // Clone the selected index to avoid borrow issues in async context.
                        // Prevent dispatching a new action while one is running.
                        if app.current_task.is_none() {
                            let idx = app.selected;
                            actions::dispatch(app, idx).await?;
                        } else {
                            // ignore enter while a task is active
                        }
                    }
                    KeyCode::Char('q') => {
                        // Exit the application on 'q'.
                        return Ok(false);
                    }
                    _ => {}
                },
                _ => {
                    // Ignore KeyEventKind::Release and other kinds.
                }
            }
        }
        Event::Resize(_, _) => {
            // A resize triggers a redraw on the next iteration; no state change required.
        }
        _ => {}
    }
    Ok(true)
}