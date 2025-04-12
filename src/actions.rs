use crate::app::MyApp;
use crate::config::{self, get_config_path};
use crate::ui::UiMessage;
// Removed unused imports: ApiTorrentListOpts, TorrentDetailsResponse
use std::path::PathBuf;
use opener; // Add use statement for opener crate

// --- Action Helper Functions --- 
// These functions are called by the UI to perform actions, often involving
// interaction with librqbit API and spawning async tasks.

// Removed refresh_torrents function

// Helper function to handle saving configuration changes
pub(crate) fn save_config_changes(app: &mut MyApp) {
    println!("Action: Save config triggered");
    if app.config_edit_url.trim().is_empty() {
        println!("Error: Remote URL cannot be empty.");
        let _ = app
            .ui_tx
            .send(UiMessage::Error("Remote URL cannot be empty".to_string()));
        return;
    }
    let new_path = PathBuf::from(app.config_edit_path_str.trim());
    if new_path.to_string_lossy().is_empty() {
        println!("Error: Local path cannot be empty.");
        let _ = app
            .ui_tx
            .send(UiMessage::Error("Local path cannot be empty".to_string()));
        return;
    }

    // Update the config in MyApp state first
    app.config.torrent_url = app.config_edit_url.trim().to_string();
    app.config.download_path = new_path;

    // Clone the final config to save
    let config_to_save = app.config.clone();
    let ui_tx_clone = app.ui_tx.clone();

    // Spawn task to handle file I/O only, without notifying sync task
    tokio::spawn(async move {
        match get_config_path() {
            Ok(config_path) => {
                match config::save_config(&config_to_save, &config_path) {
                    Ok(_) => {
                        println!("Configuration saved successfully.");
                        let _ = ui_tx_clone.send(UiMessage::Error("Configuration Saved".to_string()));
                        // We no longer notify the sync task here
                    }
                    Err(e) => {
                        eprintln!("Error saving configuration: {}", e);
                        let _ = ui_tx_clone
                            .send(UiMessage::Error(format!("Failed to save config: {}", e)));
                    }
                }
            }
            Err(e) => {
                eprintln!("Error getting config path: {}", e);
                let _ = ui_tx_clone
                    .send(UiMessage::Error(format!("Failed to get config path: {}", e)));
            }
        }
    });
}

// Action to delete extra files found during verification
pub(crate) fn delete_extra_files(app: &mut MyApp) {
    if let Some(files) = app.extra_files_to_prompt.take() { // Take ownership and clear prompt
        println!("Action: Delete {} extra files requested", files.len());
        if let Err(e) = app.sync_cmd_tx.send(UiMessage::DeleteExtraFiles(files)) {
            eprintln!("Action: Failed to send DeleteExtraFiles command: {}", e);
            // Restore prompt state on error?
            // app.extra_files_to_prompt = Some(files); // Or handle error differently
            let _ = app.ui_tx.send(UiMessage::Error(format!("Failed to send delete command: {}", e)));
        }
    } else {
        println!("Action: delete_extra_files called but no files in prompt state.");
    }
}

// Action to apply a remote torrent update
pub(crate) fn apply_remote_update(app: &mut MyApp) {
    if let Some((torrent_data, etag)) = app.remote_update.take() { // Take ownership and clear prompt
        println!("Action: Apply remote update requested ({} bytes, ETag: {})", torrent_data.len(), etag);
        // Send the update to the sync task for processing
        if let Err(e) = app.sync_cmd_tx.send(UiMessage::ApplyRemoteUpdate(torrent_data, etag)) {
            eprintln!("Action: Failed to send ApplyRemoteUpdate command: {}", e);
            let _ = app.ui_tx.send(UiMessage::Error(format!("Failed to apply update: {}", e)));
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
        let _ = app.ui_tx.send(UiMessage::Error("Download path not configured".to_string()));
        return;
    }
    
    // Ensure the path exists before trying to open it
    if !path_to_open.exists() {
         println!("Action: Download path does not exist: {}", path_to_open.display());
         let _ = app.ui_tx.send(UiMessage::Error(format!("Directory does not exist: {}", path_to_open.display())));
         return;
    }

    println!("Action: Attempting to open folder: {}", path_to_open.display());
    // Use opener crate to open the path
    match opener::open(&path_to_open) {
        Ok(_) => {
            println!("Action: Successfully requested to open folder.");
            // Optionally send a success message to UI?
            // let _ = app.ui_tx.send(UiMessage::Error("Opened folder".to_string()));
        }
        Err(e) => {
            let err_msg = format!("Failed to open folder {}: {}", path_to_open.display(), e);
            eprintln!("Action: {}", err_msg);
            let _ = app.ui_tx.send(UiMessage::Error(err_msg));
        }
    }
}