//! Entry point for the `modsync` application.
//!
//! This binary orchestrates loading the user's configuration, initialising the
//! user interface and dispatching actions such as synchronising the modpack,
//! validating the local installation and launching Arma 3.  The heavy
//! lifting is delegated to modules like `gitutils`, `modpack`, `config`,
//! `arma` and `ui`.

use anyhow::Result;
use modsync::{config, ui};

/// Asynchronously initialises the application and hands control over to the
/// TUI.  Tokio is used here to allow potentially long‑running operations (for
/// example filesystem scans or network operations) to run without blocking
/// the UI.
#[tokio::main]
async fn main() -> Result<()> {
    // Load or create the user configuration.  If the configuration file
    // doesn't exist yet the `Config::load` call will produce a sensible
    // default.  Should this fail the error will bubble up and be logged on
    // stderr.
    let config = config::Config::load()?;

    // Create and run the TUI.  The UI holds its own copy of the
    // configuration and updates it as the user makes changes.  When
    // returning from `run` the configuration is automatically persisted.
    let mut app = ui::App::new(config).await?;
    app.run().await?;
    Ok(())
}
