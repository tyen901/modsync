use std::collections::HashSet;
use std::sync::mpsc;
use std::thread::sleep;
use std::time::{Duration, Instant};

use modsync::{ControlCommand, DownloaderConfig, LfsDownloadItem, ProgressEvent};

fn spawn_job_and_channel(
    items: Vec<LfsDownloadItem>,
    cfg: DownloaderConfig,
) -> (std::sync::mpsc::Sender<ControlCommand>, mpsc::Receiver<ProgressEvent>) {
    let (tx, rx) = mpsc::channel::<ProgressEvent>();
    let on_event = move |ev: ProgressEvent| {
        // Best-effort: ignore send failures (test may have stopped receiving).
        let _ = tx.send(ev);
    };
    let control_tx = modsync::ui::attach_downloader_consumer(items, cfg, on_event);
    (control_tx, rx)
}

#[test]
fn test_progress_events_sequence() {
    // Prepare a deterministic single item
    let oid = "test-progress-oid-1".to_string();
    let mut dest = std::env::temp_dir();
    dest.push("modsync_test_progress.bin");
    // ensure no left-over file
    let _ = std::fs::remove_file(&dest);

    let item = LfsDownloadItem {
        oid: oid.clone(),
        size: Some(100_000),
        dest: dest.clone(),
        repo_remote: None,
    };

    let cfg = DownloaderConfig {
        progress_interval_ms: 50,
        coalesce_threshold_bytes: 16 * 1024,
    };

    let (_control_tx, rx) = spawn_job_and_channel(vec![item], cfg);

    // Collect events until we see Completed for our oid or timeout.
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut seen_started = false;
    let mut seen_progress = 0usize;
    let mut seen_completed = false;
    let mut at_least_one_bps_positive = false;
    while Instant::now() < deadline {
        match rx.recv_timeout(Duration::from_millis(200)) {
            Ok(ev) => {
                match &ev {
                    ProgressEvent::Started { oid: eoid, .. } => {
                        if *eoid == oid {
                            seen_started = true;
                        }
                    }
                    ProgressEvent::Progress {
                        oid: eoid,
                        instant_bps,
                        ..
                    } => {
                        if *eoid == oid {
                            seen_progress += 1;
                            if *instant_bps > 0 {
                                at_least_one_bps_positive = true;
                            }
                        }
                    }
                    ProgressEvent::Completed { oid: eoid, .. } => {
                        if *eoid == oid {
                            seen_completed = true;
                            break;
                        }
                    }
                    _ => {}
                }
            }
            Err(_) => {
                // timeout waiting for more events; break out if deadline exceeded
            }
        }
    }

    // Cleanup
    let _ = std::fs::remove_file(&dest);

    assert!(seen_started, "Expected a Started event for oid");
    assert!(seen_progress >= 1, "Expected one or more Progress events");
    assert!(seen_completed, "Expected a Completed event for oid");
    assert!(at_least_one_bps_positive, "Expected at least one Progress event with instant_bps > 0");
}

#[test]
fn test_cancel_all() {
    // Two larger items so cancellation happens mid-flight.
    let oid1 = "cancel-all-oid-1".to_string();
    let oid2 = "cancel-all-oid-2".to_string();
    let mut dest1 = std::env::temp_dir();
    dest1.push("modsync_test_cancel_all_1.bin");
    let mut dest2 = std::env::temp_dir();
    dest2.push("modsync_test_cancel_all_2.bin");
    let _ = std::fs::remove_file(&dest1);
    let _ = std::fs::remove_file(&dest2);

    let item1 = LfsDownloadItem {
        oid: oid1.clone(),
        size: Some(1_000_000),
        dest: dest1.clone(),
        repo_remote: None,
    };
    let item2 = LfsDownloadItem {
        oid: oid2.clone(),
        size: Some(1_000_000),
        dest: dest2.clone(),
        repo_remote: None,
    };

    let cfg = DownloaderConfig {
        progress_interval_ms: 50,
        coalesce_threshold_bytes: 16 * 1024,
    };

    let (control_tx, rx) = spawn_job_and_channel(vec![item1, item2], cfg);

    // Let the job run briefly then cancel all.
    sleep(Duration::from_millis(120));
    let _ = control_tx.send(ControlCommand::CancelAll);

    // Collect events until channel quiets; track failed/completed oids.
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut failed = HashSet::new();
    let mut completed = HashSet::new();
    while Instant::now() < deadline {
        match rx.recv_timeout(Duration::from_millis(250)) {
            Ok(ev) => match ev {
                ProgressEvent::Failed { oid, error } => {
                    if error == "cancelled" {
                        failed.insert(oid);
                    }
                }
                ProgressEvent::Completed { oid, .. } => {
                    completed.insert(oid);
                }
                _ => {}
            },
            Err(_) => {
                // If no events for a short period consider the job finished.
                break;
            }
        }
    }

    // Cleanup
    let _ = std::fs::remove_file(&dest1);
    let _ = std::fs::remove_file(&dest2);

    assert!(
        !failed.is_empty(),
        "Expected at least one Failed event with error 'cancelled'"
    );

    // For any failed oid ensure we didn't also receive a Completed for it.
    for foid in failed.iter() {
        assert!(
            !completed.contains(foid),
            "Received Completed for an oid that also failed: {}",
            foid
        );
    }
}

#[test]
fn test_cancel_file() {
    let oid_a = "cancel-file-oid-a".to_string();
    let oid_b = "cancel-file-oid-b".to_string();

    let mut dest_a = std::env::temp_dir();
    dest_a.push("modsync_test_cancel_file_a.bin");
    let mut dest_b = std::env::temp_dir();
    dest_b.push("modsync_test_cancel_file_b.bin");
    let _ = std::fs::remove_file(&dest_a);
    let _ = std::fs::remove_file(&dest_b);

    let item_a = LfsDownloadItem {
        oid: oid_a.clone(),
        size: Some(1_000_000),
        dest: dest_a.clone(),
        repo_remote: None,
    };
    let item_b = LfsDownloadItem {
        oid: oid_b.clone(),
        size: Some(1_000_000),
        dest: dest_b.clone(),
        repo_remote: None,
    };

    let cfg = DownloaderConfig {
        progress_interval_ms: 50,
        coalesce_threshold_bytes: 16 * 1024,
    };

    let (control_tx, rx) = spawn_job_and_channel(vec![item_a, item_b], cfg);

    // Let workers start and then cancel one file.
    sleep(Duration::from_millis(120));
    let _ = control_tx.send(ControlCommand::CancelFile { oid: oid_a.clone() });

    // Collect events and ensure a Failed "cancelled" for the cancelled oid
    // and that the other file at least progressed or completed.
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut failed = HashSet::new();
    let mut completed = HashSet::new();
    let mut progress_seen = HashSet::new();

    while Instant::now() < deadline {
        match rx.recv_timeout(Duration::from_millis(250)) {
            Ok(ev) => match ev {
                ProgressEvent::Failed { oid, error } => {
                    if error == "cancelled" {
                        failed.insert(oid);
                    }
                }
                ProgressEvent::Completed { oid, .. } => {
                    completed.insert(oid);
                }
                ProgressEvent::Progress { oid, .. } => {
                    progress_seen.insert(oid);
                }
                _ => {}
            },
            Err(_) => {
                break;
            }
        }
    }

    // Cleanup
    let _ = std::fs::remove_file(&dest_a);
    let _ = std::fs::remove_file(&dest_b);

    assert!(
        failed.contains(&oid_a),
        "Expected cancelled file to produce a Failed('cancelled') event"
    );

    // Ensure the other file made progress or completed.
    let other_ok = completed.contains(&oid_b) || progress_seen.contains(&oid_b);
    assert!(
        other_ok,
        "Expected the non-cancelled file to make progress or complete"
    );
}