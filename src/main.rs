mod actions;
mod app;
mod config;
mod sync;
mod ui;

use anyhow::Context;
use app::MyApp;
use config::{load_config, get_config_path, AppConfig};
use librqbit::{Api, Session, SessionOptions};
use tokio::sync::mpsc;
use ui::UiMessage;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Get config path and load initial configuration
    let config_path = get_config_path().context("Failed to determine config path")?;
    let initial_config = load_config(&config_path).context("Failed to load initial configuration")?;

    let options = eframe::NativeOptions::default();

    // Ensure download path exists
    let download_path = initial_config.download_path.clone(); // Renamed field
    if !download_path.as_os_str().is_empty() {
        tokio::fs::create_dir_all(&download_path)
            .await
            .with_context(|| format!("Failed to create download directory: {:?}", download_path))?;
    }

    // Setup librqbit session
    let session = Session::new_with_opts(
        download_path, // Use the potentially empty path if not configured
        SessionOptions {
            disable_dht: true, // Keep DHT disabled for simplicity/focus
            disable_dht_persistence: true,
            persistence: None,
            fastresume: true, // Enable fastresume to speed up checking existing files
            ..Default::default()
        }
    ).await.context("Failed to initialize librqbit session")?;

    let api = Api::new(session.clone(), None);

    // Create channel for UI communication
    let (ui_tx, ui_rx) = mpsc::unbounded_channel::<UiMessage>();
    // Create channel for messages from UI to sync manager
    let (sync_cmd_tx, sync_cmd_rx) = mpsc::unbounded_channel::<UiMessage>();
    // Create channel for Config updates
    let (config_update_tx, config_update_rx) = mpsc::unbounded_channel::<AppConfig>();

    // Spawn the sync manager task
    let sync_api = api.clone();
    let sync_config = initial_config.clone();
    let sync_ui_tx = ui_tx.clone();
    tokio::spawn(async move {
        println!("Sync manager task started.");
        if let Err(e) = sync::state::run_sync_manager(
            sync_config,
            sync_api,
            sync_ui_tx,
            config_update_rx, // Pass config receiver
            sync_cmd_rx,      // Pass UI command receiver
        )
        .await
        {
            eprintln!("Sync manager task exited with error: {}", e);
        }
        println!("Sync manager task finished.");
    });

    // Run the eframe UI
    eframe::run_native(
        "ModSync",
        options,
        Box::new(move |_cc| {
            // Pass initial config and channels to MyApp
            Ok(Box::new(MyApp::new(
                api,
                ui_tx,
                ui_rx,
                config_update_tx, // Pass config sender
                sync_cmd_tx,      // Pass sync command sender
                initial_config,
            ))
                as Box<dyn eframe::App>)
        }),
    )
    .map_err(|e| anyhow::anyhow!("eframe error: {}", e))?;

    Ok(())
}
