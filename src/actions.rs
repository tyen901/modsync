use crate::app::MyApp;
use crate::config::{self, get_config_path};
use crate::ui::UiMessage;
// Removed unused imports: ApiTorrentListOpts, TorrentDetailsResponse
use std::path::PathBuf;

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

    // Clone the final config to save and send
    let config_to_save_and_send = app.config.clone(); 
    let ui_tx_clone = app.ui_tx.clone();
    // Clone the config update sender
    let config_update_tx_clone = app.config_update_tx.clone();

    // Spawn task to handle file I/O and notify sync task
    tokio::spawn(async move {
        match get_config_path() {
            Ok(config_path) => {
                match config::save_config(&config_to_save_and_send, &config_path) {
                    Ok(_) => {
                        println!("Configuration saved successfully.");
                        let _ = ui_tx_clone.send(UiMessage::Error("Configuration Saved".to_string()));

                        // Send the updated config to the sync manager task
                        println!("Action: Sending config update to sync task.");
                        if let Err(e) = config_update_tx_clone.send(config_to_save_and_send) {
                             eprintln!("Action: Failed to send config update to sync task: {}", e);
                             // Send error to UI as well?
                             let _ = ui_tx_clone.send(UiMessage::Error(format!("Failed to notify sync task: {}", e)));
                        }
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