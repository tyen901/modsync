mod actions;
mod app;
mod config;
mod sync;
mod ui;

use anyhow::Context;
use app::MyApp;
use config::{load_config, get_config_path, get_cached_torrent_path, AppConfig};
use librqbit::{Api, Session, SessionOptions, AddTorrent, AddTorrentOptions};
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
    let session_download_path = download_path.clone(); // Clone for session
    let session = Session::new_with_opts(
        session_download_path, // Pass the clone
        SessionOptions {
            disable_dht: true, // Keep DHT disabled for simplicity/focus
            disable_dht_persistence: true,
            persistence: None,
            fastresume: true, // Enable fastresume to speed up checking existing files
            ..Default::default()
        }
    ).await.context("Failed to initialize librqbit session")?;

    let api = Api::new(session.clone(), None);

    // --- Load cached torrent --- 
    let mut initial_torrent_id = None;
    match get_cached_torrent_path() {
        Ok(cached_path) => {
            if cached_path.exists() {
                println!("Main: Found cached torrent at {}", cached_path.display());
                match tokio::fs::read(&cached_path).await {
                    Ok(torrent_bytes) => {
                         println!("Main: Read {} bytes from cached torrent.", torrent_bytes.len());
                        // Add the cached torrent, not paused, ensuring overwrite checks
                        let add_request = AddTorrent::from_bytes(torrent_bytes);
                        let add_options = AddTorrentOptions {
                            output_folder: Some(download_path.to_string_lossy().into_owned()), // Use original download_path here
                            paused: false, // Start unpaused to trigger immediate check/sync
                            overwrite: true, // Ensure files are checked against cache
                            ..Default::default()
                        };
                        match api.api_add_torrent(add_request, Some(add_options)).await {
                            Ok(response) => {
                                if let Some(id) = response.id {
                                    println!("Main: Successfully added cached torrent with ID: {}", id);
                                    initial_torrent_id = Some(id);
                                } else {
                                    eprintln!("Main: Added cached torrent but API returned no ID.");
                                    // Delete potentially broken cache file?
                                    let _ = tokio::fs::remove_file(&cached_path).await;
                                }
                            }
                            Err(e) => {
                                eprintln!("Main: Error adding cached torrent: {}. Deleting cache.", e);
                                let _ = tokio::fs::remove_file(&cached_path).await;
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("Main: Error reading cached torrent file {}: {}. Deleting cache.", cached_path.display(), e);
                        let _ = tokio::fs::remove_file(cached_path).await;
                    }
                }
            } else {
                println!("Main: No cached torrent file found at {}", cached_path.display());
            }
        }
        Err(e) => {
            eprintln!("Main: Error getting cached torrent path: {}", e);
            // Proceed without cache
        }
    }
    // --------------------------

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
            initial_torrent_id, // Pass the initial ID
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
