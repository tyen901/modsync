use crate::app::MyApp;
use crate::config::{self, get_config_path};
use crate::sync::{SyncCommand, SyncEvent}; // Import our new types
// Removed unused imports: ApiTorrentListOpts, TorrentDetailsResponse
use std::path::PathBuf;
use opener; // Add use statement for opener crate
use anyhow::{anyhow, Result}; // Import anyhow properly

// --- Action Helper Functions --- 
// These functions are called by the UI to perform actions, often involving
// interaction with librqbit API and spawning async tasks.

// Removed refresh_torrents function

// Helper function to handle saving configuration changes
pub fn save_config_changes(app: &mut MyApp) -> Result<()> {
    println!("Action: Saving configuration changes");

    // Update the application config with input values
    app.config.torrent_url = app.config_edit_url.clone();
    let path_str = app.config_edit_path_str.clone();
    app.config.download_path = PathBuf::from(path_str);
    
    // Update the profile settings
    app.config.should_seed = app.config_edit_should_seed;
    
    // Parse the speed limits
    app.config.max_upload_speed = if app.config_edit_max_upload_speed_str.trim().is_empty() {
        None
    } else {
        match app.config_edit_max_upload_speed_str.parse::<u64>() {
            Ok(speed) => Some(speed),
            Err(e) => {
                let err_msg = format!("Invalid upload speed: {}", e);
                println!("Action: {}", err_msg);
                app.last_error = Some(err_msg);
                return Err(anyhow!("Invalid upload speed value"));
            }
        }
    };
    
    app.config.max_download_speed = if app.config_edit_max_download_speed_str.trim().is_empty() {
        None
    } else {
        match app.config_edit_max_download_speed_str.parse::<u64>() {
            Ok(speed) => Some(speed),
            Err(e) => {
                let err_msg = format!("Invalid download speed: {}", e);
                println!("Action: {}", err_msg);
                app.last_error = Some(err_msg);
                return Err(anyhow!("Invalid download speed value"));
            }
        }
    };

    // Clone for async block
    let config_clone = app.config.clone();
    let ui_tx_clone = app.ui_tx.clone();
    let _sync_cmd_tx_clone = app.sync_cmd_tx.clone();

    // First notify the sync manager about the config update
    if let Err(e) = app.sync_cmd_tx.send(SyncCommand::UpdateConfig(app.config.clone())) {
        let err_msg = format!("Failed to send config update to sync manager: {}", e);
        println!("Action: {}", err_msg);
        app.last_error = Some(err_msg);
        return Err(anyhow!("Failed to send config update"));
    }

    // Spawn task to handle file I/O only, without notifying sync task
    tokio::spawn(async move {
        match get_config_path() {
            Ok(config_path) => {
                match config::save_config(&config_clone, &config_path) {
                    Ok(_) => {
                        println!("Configuration saved successfully.");
                        let _ = ui_tx_clone.send(SyncEvent::Error("Configuration Saved".to_string()));
                        // We no longer notify the sync task here
                    }
                    Err(e) => {
                        eprintln!("Error saving configuration: {}", e);
                        let _ = ui_tx_clone
                            .send(SyncEvent::Error(format!("Failed to save config: {}", e)));
                    }
                }
            }
            Err(e) => {
                eprintln!("Error getting config path: {}", e);
                let _ = ui_tx_clone
                    .send(SyncEvent::Error(format!("Failed to get config path: {}", e)));
            }
        }
    });

    Ok(())
}

// Action to delete extra files found during verification
pub(crate) fn delete_extra_files(app: &mut MyApp) {
    if let Some(files) = app.extra_files_to_prompt.take() { // Take ownership and clear prompt
        println!("Action: Delete {} extra files requested", files.len());
        if let Err(e) = app.sync_cmd_tx.send(SyncCommand::DeleteFiles(files)) {
            eprintln!("Action: Failed to send DeleteFiles command: {}", e);
            // Restore prompt state on error?
            // app.extra_files_to_prompt = Some(files); // Or handle error differently
            let _ = app.ui_tx.send(SyncEvent::Error(format!("Failed to send delete command: {}", e)));
        }
    } else {
        println!("Action: delete_extra_files called but no files in prompt state.");
    }
}

// Action to apply a remote torrent update
pub(crate) fn apply_remote_update(app: &mut MyApp) {
    if let Some(torrent_data) = app.remote_update.take() { // Take ownership and clear prompt
        println!("Action: Apply remote update requested ({} bytes)", torrent_data.len());
        // Send the update to the sync task for processing
        if let Err(e) = app.sync_cmd_tx.send(SyncCommand::ApplyUpdate(torrent_data)) {
            eprintln!("Action: Failed to send ApplyUpdate command: {}", e);
            let _ = app.ui_tx.send(SyncEvent::Error(format!("Failed to apply update: {}", e)));
        }
    } else {
        println!("Action: apply_remote_update called but no remote update data available.");
    }
}

// Action to open the download folder in the system file explorer
pub(crate) fn open_download_folder(app: &MyApp) {
    let path_to_open = app.config.download_path.clone();
    if path_to_open.as_os_str().is_empty() {
        println!("Action: Cannot open folder, path is not set.");
        let _ = app.ui_tx.send(SyncEvent::Error("Download path not configured".to_string()));
        return;
    }
    
    // Ensure the path exists before trying to open it
    if !path_to_open.exists() {
         println!("Action: Download path does not exist: {}", path_to_open.display());
         let _ = app.ui_tx.send(SyncEvent::Error(format!("Directory does not exist: {}", path_to_open.display())));
         return;
    }

    println!("Action: Attempting to open folder: {}", path_to_open.display());
    // Use opener crate to open the path
    match opener::open(&path_to_open) {
        Ok(_) => {
            println!("Action: Successfully requested to open folder.");
            // Optionally send a success message to UI?
            // let _ = app.ui_tx.send(SyncEvent::Error("Opened folder".to_string()));
        }
        Err(e) => {
            let err_msg = format!("Failed to open folder {}: {}", path_to_open.display(), e);
            eprintln!("Action: {}", err_msg);
            let _ = app.ui_tx.send(SyncEvent::Error(err_msg));
        }
    }
}

// Action to update from remote URL using the latest configuration values
pub(crate) fn update_from_remote(app: &mut MyApp) {
    println!("Action: Update from remote URL requested");
    
    // First, update the configuration from the UI fields
    let new_url = app.config_edit_url.trim().to_string();
    let new_path = PathBuf::from(app.config_edit_path_str.trim());
    
    // Validate input
    if new_url.is_empty() {
        println!("Error: Remote URL cannot be empty for update.");
        let _ = app.ui_tx.send(SyncEvent::Error("Remote URL cannot be empty".to_string()));
        return;
    }
    
    if new_path.to_string_lossy().is_empty() {
        println!("Error: Local path cannot be empty for update.");
        let _ = app.ui_tx.send(SyncEvent::Error("Local path cannot be empty".to_string()));
        return;
    }
    
    // Update the config in MyApp state
    app.config.torrent_url = new_url.clone();
    app.config.download_path = new_path;
    
    // Update profile settings
    app.config.should_seed = app.config_edit_should_seed;
    
    // Parse speed limits
    app.config.max_upload_speed = if app.config_edit_max_upload_speed_str.trim().is_empty() {
        None
    } else {
        match app.config_edit_max_upload_speed_str.parse::<u64>() {
            Ok(speed) => Some(speed),
            Err(e) => {
                let err_msg = format!("Invalid upload speed: {}", e);
                let _ = app.ui_tx.send(SyncEvent::Error(err_msg.clone()));
                app.last_error = Some(err_msg);
                return;
            }
        }
    };
    
    app.config.max_download_speed = if app.config_edit_max_download_speed_str.trim().is_empty() {
        None
    } else {
        match app.config_edit_max_download_speed_str.parse::<u64>() {
            Ok(speed) => Some(speed),
            Err(e) => {
                let err_msg = format!("Invalid download speed: {}", e);
                let _ = app.ui_tx.send(SyncEvent::Error(err_msg.clone()));
                app.last_error = Some(err_msg);
                return;
            }
        }
    };
    
    // Clone the config to save
    let config_to_save = app.config.clone();
    let ui_tx_clone = app.ui_tx.clone();
    let sync_cmd_tx_clone = app.sync_cmd_tx.clone();
    
    // Spawn task to handle file I/O and trigger force download and compare
    tokio::spawn(async move {
        match get_config_path() {
            Ok(config_path) => {
                match config::save_config(&config_to_save, &config_path) {
                    Ok(_) => {
                        println!("Configuration saved successfully, triggering direct torrent download and comparison.");
                        let _ = ui_tx_clone.send(SyncEvent::Error("Configuration Updated".to_string()));
                        
                        // Instead of TriggerManualRefresh, use our new message
                        if let Err(e) = sync_cmd_tx_clone.send(SyncCommand::DownloadAndCompare(new_url)) {
                            eprintln!("Failed to trigger direct download: {}", e);
                            let _ = ui_tx_clone.send(SyncEvent::Error(format!("Failed to trigger direct download: {}", e)));
                        }
                    }
                    Err(e) => {
                        eprintln!("Error saving configuration: {}", e);
                        let _ = ui_tx_clone.send(SyncEvent::Error(format!("Failed to save config: {}", e)));
                    }
                }
            }
            Err(e) => {
                eprintln!("Error getting config path: {}", e);
                let _ = ui_tx_clone.send(SyncEvent::Error(format!("Failed to get config path: {}", e)));
            }
        }
    });
}