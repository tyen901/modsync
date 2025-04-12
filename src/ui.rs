// UI Module

use crate::config::{self, Config}; // Import config module and Config struct
use eframe::egui::{self, CentralPanel, ProgressBar};
use librqbit::Api;
use librqbit::api::{ApiTorrentListOpts, TorrentDetailsResponse};
use std::path::PathBuf;
use tokio::sync::mpsc;

// Enum for messages sent back to the UI thread
pub enum UiMessage {
    UpdateTorrents(Vec<(usize, TorrentDetailsResponse, u64)>),
    TorrentAdded(usize),
    Error(String),
}

pub struct MyApp {
    pub(crate) api: Api,
    pub(crate) torrents: Vec<(usize, TorrentDetailsResponse, u64)>,
    pub(crate) ui_tx: mpsc::UnboundedSender<UiMessage>,
    pub(crate) ui_rx: mpsc::UnboundedReceiver<UiMessage>,
    pub(crate) config: Config,
    // Temporary fields for UI input before saving
    pub(crate) config_edit_url: String,
    pub(crate) config_edit_path_str: String,
}

impl MyApp {
    pub fn new(
        api: Api,
        ui_tx: mpsc::UnboundedSender<UiMessage>,
        ui_rx: mpsc::UnboundedReceiver<UiMessage>,
        initial_config: Config,
    ) -> Self {
        let config_edit_url = initial_config.remote_torrent_url.clone();
        let config_edit_path_str = initial_config
            .local_download_path
            .to_string_lossy()
            .into_owned();
        Self {
            api,
            torrents: Vec::new(),
            ui_tx,
            ui_rx,
            config: initial_config,
            config_edit_url,
            config_edit_path_str,
        }
    }

    // Method to refresh torrent list (might be adapted or less relevant)
    fn refresh_torrents(&self) {
        println!("Refresh called (manual)");
        let api_clone = self.api.clone();
        let ui_tx_clone = self.ui_tx.clone();

        tokio::spawn(async move {
            let opts = ApiTorrentListOpts { with_stats: true };
            let list_response = api_clone.api_torrent_list_ext(opts);
            println!(
                "Refreshed torrents: {} torrents found",
                list_response.torrents.len()
            );

            let torrents_data: Vec<(usize, TorrentDetailsResponse, u64)> = list_response
                .torrents
                .into_iter()
                .filter_map(|details| {
                    details.id.map(|id| {
                        let total_size = details
                            .files
                            .as_ref()
                            .map(|files| {
                                files.iter().filter(|f| f.included).map(|f| f.length).sum()
                            })
                            .unwrap_or(0);
                        (id, details, total_size)
                    })
                })
                .collect();

            if let Err(e) = ui_tx_clone.send(UiMessage::UpdateTorrents(torrents_data)) {
                eprintln!("Failed to send torrent update to UI: {}", e);
            }
        });
    }

    // Method to handle saving configuration changes from the UI
    fn save_config_changes(&mut self) {
        if self.config_edit_url.trim().is_empty() {
            println!("Error: Remote URL cannot be empty.");
            // TODO: Show error in UI via channel?
            // let _ = self.ui_tx.send(UiMessage::Error("Remote URL cannot be empty".to_string()));
            return;
        }
        let new_path = PathBuf::from(self.config_edit_path_str.trim());
        if new_path.to_string_lossy().is_empty() {
            println!("Error: Local path cannot be empty.");
            // let _ = self.ui_tx.send(UiMessage::Error("Local path cannot be empty".to_string()));
            return;
        }

        self.config.remote_torrent_url = self.config_edit_url.trim().to_string();
        self.config.local_download_path = new_path;

        match config::save_config(&self.config) {
            // Use config::save_config
            Ok(_) => {
                println!("Configuration saved successfully.");
                // TODO: Trigger the sync process to start with the new config
                // self.start_sync_process();
                // Optionally send a success message?
                // let _ = self.ui_tx.send(UiMessage::Error("Config Saved".to_string()));
            }
            Err(e) => {
                eprintln!("Error saving configuration: {}", e);
                let _ = self
                    .ui_tx
                    .send(UiMessage::Error(format!("Failed to save config: {}", e)));
            }
        }
    }

    // Action methods using usize for ID (might be removed/adapted for sync logic)
    fn pause_torrent(&self, id: usize) {
        println!("UI Request: Pause torrent {}", id);
        let api_clone = self.api.clone();
        let ui_tx_clone = self.ui_tx.clone();
        tokio::spawn(async move {
            match api_clone.api_torrent_action_pause(id.into()).await {
                Ok(_) => {
                    println!("Paused torrent {} successfully", id);
                    let _ = ui_tx_clone.send(UiMessage::Error(format!("Torrent {} Paused", id)));
                }
                Err(e) => {
                    eprintln!("Error pausing torrent {}: {}", id, e);
                    let _ = ui_tx_clone.send(UiMessage::Error(format!(
                        "Error pausing torrent {}: {}",
                        id, e
                    )));
                }
            }
        });
    }
    fn start_torrent(&self, id: usize) {
        println!("UI Request: Start torrent {}", id);
        let api_clone = self.api.clone();
        let ui_tx_clone = self.ui_tx.clone();
        tokio::spawn(async move {
            match api_clone.api_torrent_action_start(id.into()).await {
                Ok(_) => {
                    println!("Started torrent {} successfully", id);
                    let _ = ui_tx_clone
                        .send(UiMessage::Error(format!("Torrent {} Started/Resumed", id)));
                }
                Err(e) => {
                    eprintln!("Error starting torrent {}: {}", id, e);
                    let _ = ui_tx_clone.send(UiMessage::Error(format!(
                        "Error starting torrent {}: {}",
                        id, e
                    )));
                }
            }
        });
    }

    fn cancel_torrent(&self, id: usize) {
        println!("UI Request: Cancel torrent {}", id);
        let api_clone = self.api.clone();
        let ui_tx_clone = self.ui_tx.clone();
        tokio::spawn(async move {
            match api_clone.api_torrent_action_forget(id.into()).await {
                Ok(_) => {
                    println!("Forgot torrent {} successfully", id);
                    let _ = ui_tx_clone.send(UiMessage::Error(format!("Torrent {} Forgotten", id)));
                    // TODO: Refresh list or trigger sync check after forgetting?
                }
                Err(e) => {
                    eprintln!("Error forgetting torrent {}: {}", id, e);
                    let _ = ui_tx_clone.send(UiMessage::Error(format!(
                        "Error forgetting torrent {}: {}",
                        id, e
                    )));
                }
            }
        });
    }
}

impl eframe::App for MyApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Process messages from the channel
        while let Ok(message) = self.ui_rx.try_recv() {
            match message {
                UiMessage::UpdateTorrents(torrents) => {
                    println!("UI received updated torrent list");
                    self.torrents = torrents;
                }
                UiMessage::TorrentAdded(id) => {
                    println!("UI notified: Torrent {} added/managed", id);
                    // TODO: Update UI to reflect this, maybe trigger refresh?
                }
                UiMessage::Error(err_msg) => {
                    println!("UI received error: {}", err_msg);
                    // TODO: Display error to the user (e.g., in a dedicated status area)
                }
            }
        }

        CentralPanel::default().show(ctx, |ui| {
            ui.heading("ModSync Configuration");

            ui.horizontal(|ui| {
                ui.label("Remote .torrent URL:");
                ui.text_edit_singleline(&mut self.config_edit_url);
            });
            ui.horizontal(|ui| {
                ui.label("Local Download Path:");
                ui.text_edit_singleline(&mut self.config_edit_path_str);
            });

            if ui.button("Save Configuration & Start Sync").clicked() {
                self.save_config_changes();
            }

            ui.separator();
            ui.heading("Synchronization Status");

            ui.label(format!(
                "Monitoring URL: {}",
                self.config.remote_torrent_url
            ));
            ui.label(format!(
                "Target Path: {:?}",
                self.config.local_download_path
            ));
            // TODO: Add more status indicators here based on sync progress

            ui.separator();
            ui.heading("Current Torrent State (Debug)");
            if self.torrents.is_empty() {
                ui.label("No active torrent task.");
            } else {
                for (id, details, total_size) in &self.torrents {
                    ui.horizontal(|ui| {
                        let name = details.name.as_deref().unwrap_or("Unknown");
                        ui.label(format!("ID: {}, Name: {}", id, name));

                        if let Some(stats) = &details.stats {
                            let progress = if *total_size > 0 {
                                (stats.progress_bytes as f64 / *total_size as f64) as f32
                            } else {
                                0.0
                            };
                            ui.add(
                                ProgressBar::new(progress)
                                    .text(format!("{:.1}%", progress * 100.0)),
                            );
                            ui.label(format!("Status: {:?}", stats.state));
                            // TODO: Potentially show speeds, peers etc. from stats
                        } else {
                            ui.label("Status: Loading...");
                        }

                        // These buttons might be less relevant now or need context (e.g., only show Pause if running)
                        if ui.button("Pause").clicked() {
                            self.pause_torrent(*id);
                        }
                        if ui.button("Start").clicked() {
                            self.start_torrent(*id);
                        }
                        if ui.button("Cancel").clicked() {
                            self.cancel_torrent(*id);
                        }
                    });
                }
            }
        });

        ctx.request_repaint();
    }
}
