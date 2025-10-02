// src/sync/torrent.rs

use crate::config::AppConfig;
use crate::ui::utils::SyncStatus;
use crate::sync::messages::SyncEvent;
use anyhow::{Context, Result};
use librqbit::{AddTorrent, AddTorrentOptions};
use tokio::sync::mpsc;
use librqbit::limits::LimitsConfig;
use std::num::NonZeroU32;

use super::utils::send_sync_status_event;

pub async fn manage_torrent_task(
    app_config: &AppConfig,
    api: &librqbit::api::Api,
    ui_tx: &mpsc::UnboundedSender<SyncEvent>,
    current_id_to_forget: Option<usize>,
    torrent_content: Vec<u8>,
) -> Result<Option<usize>> {
    println!(
        "Sync: Managing torrent task for URL: {}. Path: {}. Current ID to forget: {:?}",
        app_config.torrent_url,
        app_config.download_path.display(),
        current_id_to_forget
    );

    if let Some(id_to_forget) = current_id_to_forget {
        println!("Sync: Forgetting previous torrent ID: {}", id_to_forget);
        send_sync_status_event(ui_tx, SyncStatus::UpdatingTorrent);
        
        match api
            .api_torrent_action_forget(id_to_forget.into())
            .await
        {
            Ok(_) => println!("Sync: Successfully forgot torrent {}", id_to_forget),
            Err(e) => {
                eprintln!(
                    "Sync: Error forgetting torrent {}: {}. Proceeding to add new one.",
                    id_to_forget,
                    e
                );
                 let _ = ui_tx.send(SyncEvent::Error(format!("Error forgetting old torrent {}: {}", id_to_forget, e)));
            }
        }
    }

    println!(
        "Sync: Adding new torrent content ({} bytes) to path: {}",
        torrent_content.len(),
        app_config.download_path.display()
    );

    if app_config.download_path.as_os_str().is_empty() {
        println!("Sync: Download path is empty, cannot add torrent.");
        let err_msg = "Download path not configured".to_string();
        let _ = ui_tx.send(SyncEvent::Error(err_msg.clone()));
        send_sync_status_event(ui_tx, SyncStatus::Error(err_msg));
        return Ok(None);
    }

    send_sync_status_event(ui_tx, SyncStatus::UpdatingTorrent);

    let add_request = AddTorrent::from_bytes(torrent_content);
    
    let ratelimits = LimitsConfig {
        download_bps: app_config.max_download_speed.and_then(|s| {
            let value = (s * 1024) as u32;
            NonZeroU32::new(value)
        }),
        upload_bps: app_config.max_upload_speed.and_then(|s| {
            let value = (s * 1024) as u32;
            NonZeroU32::new(value)
        }),
    };
    
    let options = AddTorrentOptions {
        output_folder: Some(app_config.download_path.to_string_lossy().into_owned()),
        overwrite: true,
        paused: !app_config.should_seed,
        ratelimits,
        ..Default::default()
    };

    println!(
        "Sync: Applying settings - Seeding: {}, Upload limit: {:?} KB/s, Download limit: {:?} KB/s",
        app_config.should_seed,
        app_config.max_upload_speed,
        app_config.max_download_speed
    );

    let response = api
        .api_add_torrent(add_request, Some(options))
        .await
        .context("Failed to add torrent via librqbit API")?;

    if let Some(id) = response.id {
        println!("Sync: Torrent added successfully with ID: {}", id);
        let _ = ui_tx.send(SyncEvent::TorrentAdded(id));
        send_sync_status_event(ui_tx, SyncStatus::Idle);
        Ok(Some(id))
    } else {
        println!("Sync: Torrent added but no ID returned by API.");
        let err_msg = "Torrent added but API returned no ID".to_string();
        let _ = ui_tx.send(SyncEvent::Error(err_msg.clone()));
        send_sync_status_event(ui_tx, SyncStatus::Error(err_msg));
        Ok(None)
    }
}