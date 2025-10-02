//! Operations related to the remote torrent state

use super::types::SyncConfig;
use reqwest;
use tokio::sync::mpsc;

use crate::sync::status::SyncStatus;

use super::local::refresh_managed_torrent_status_event;
use super::messages::SyncEvent;
use super::types::{LocalTorrentState, RemoteTorrentState, SyncState};
use super::utils::{download_torrent, calculate_torrent_hash, get_local_torrent_hash, send_sync_status_event};
use super::manage_torrent_task;

pub async fn apply_remote_update(
    config: &SyncConfig,
    state: &mut SyncState,
    api: &librqbit::Api,
    ui_tx: &mpsc::UnboundedSender<SyncEvent>,
    torrent_content: Vec<u8>,
) -> bool {
    send_sync_status_event(ui_tx, SyncStatus::UpdatingTorrent);

    let current_id_to_forget = match state.local {
        LocalTorrentState::Active { id } => Some(id),
        LocalTorrentState::NotLoaded => None,
    };

    match manage_torrent_task(
        config,
        api,
        ui_tx,
        current_id_to_forget,
        torrent_content,
    )
    .await
    {
        Ok(new_id) => {
            println!(
                "Sync: Torrent task managed successfully. New ID: {:?}",
                new_id
            );

            state.local = match new_id {
                Some(id) => LocalTorrentState::Active { id },
                None => LocalTorrentState::NotLoaded,
            };

            if let LocalTorrentState::Active { id } = state.local {
                refresh_managed_torrent_status_event(api, ui_tx, id);
            }
            true
        }
        Err(e) => {
            let err_msg = format!("Sync error managing torrent: {}", e);
            eprintln!("Sync: {}", err_msg);
            let _ = ui_tx.send(SyncEvent::Error(err_msg.clone()));
            send_sync_status_event(ui_tx, SyncStatus::Error(err_msg));
            false
        }
    }
}

pub async fn direct_download_and_compare(
    config: &SyncConfig,
    state: &mut SyncState,
    _api: &librqbit::Api,
    ui_tx: &mpsc::UnboundedSender<SyncEvent>,
    http_client: &reqwest::Client,
) {
    if config.torrent_url.is_empty() {
        println!("Sync: No remote URL configured, skipping direct download.");
        send_sync_status_event(ui_tx, SyncStatus::Idle);
        return;
    }

    println!(
        "Sync: Directly downloading torrent from {}...",
        config.torrent_url
    );
    send_sync_status_event(ui_tx, SyncStatus::CheckingRemote);

    match download_torrent(&config.torrent_url, http_client).await {
        Ok(remote_torrent) => {
            println!(
                "Sync: Downloaded remote torrent successfully ({} bytes)",
                remote_torrent.len()
            );

            let remote_hash = calculate_torrent_hash(&remote_torrent);
            println!("Sync: Remote torrent hash: {}", remote_hash);

            let local_hash_result = get_local_torrent_hash(config.cached_torrent_path.clone()).await;

            match local_hash_result {
                Ok(Some(local_hash)) => {
                    println!("Sync: Local torrent hash: {}", local_hash);

                    if remote_hash != local_hash {
                        println!(
                            "Sync: Torrent has changed! Remote hash different from local hash."
                        );

                        if let Some(cache_path) = &config.cached_torrent_path {
                            println!("Sync: Writing downloaded torrent to cache: {}", cache_path.display());
                            if let Err(e) = tokio::fs::write(&cache_path, &remote_torrent).await {
                                eprintln!("Sync: Failed to write cached torrent file: {}", e);
                            }
                        }

                        state.remote = RemoteTorrentState::UpdateAvailable;

                        if let Err(e) = ui_tx.send(SyncEvent::RemoteUpdateFound(remote_torrent)) {
                            let err_msg = format!("Failed to send update notification to UI: {}", e);
                            eprintln!("Sync: {}", err_msg);
                            send_sync_status_event(ui_tx, SyncStatus::Error(err_msg));
                        } else {
                            send_sync_status_event(ui_tx, SyncStatus::RemoteChanged);
                        }
                    } else {
                        println!("Sync: Torrent is unchanged. Local and remote hashes match.");
                        send_sync_status_event(ui_tx, SyncStatus::Idle);
                    }
                }
                Ok(None) => {
                    println!("Sync: No local torrent found. This is a new torrent.");

                    if let Some(cache_path) = &config.cached_torrent_path {
                        println!("Sync: Writing downloaded torrent to cache: {}", cache_path.display());
                        if let Err(e) = tokio::fs::write(&cache_path, &remote_torrent).await {
                            eprintln!("Sync: Failed to write cached torrent file: {}", e);
                        }
                    }

                    state.remote = RemoteTorrentState::UpdateAvailable;

                    if let Err(e) = ui_tx.send(SyncEvent::RemoteUpdateFound(remote_torrent)) {
                        let err_msg = format!("Failed to send update notification to UI: {}", e);
                        eprintln!("Sync: {}", err_msg);
                        send_sync_status_event(ui_tx, SyncStatus::Error(err_msg));
                    } else {
                        send_sync_status_event(ui_tx, SyncStatus::RemoteChanged);
                    }
                }
                Err(e) => {
                    let err_msg = format!("Failed to get local torrent hash: {}", e);
                    eprintln!("Sync: {}", err_msg);
                    let _ = ui_tx.send(SyncEvent::Error(err_msg.clone()));
                    send_sync_status_event(ui_tx, SyncStatus::Error(err_msg));
                }
            }
        }
        Err(e) => {
            let err_msg = format!("Failed to download remote torrent: {}", e);
            eprintln!("Sync: {}", err_msg);
            let _ = ui_tx.send(SyncEvent::Error(err_msg.clone()));
            send_sync_status_event(ui_tx, SyncStatus::Error(err_msg));
        }
    }
}