use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use modsync::{DownloaderConfig, LfsDownloadItem, ProgressEvent};

#[test]
fn e2e_downloads_reports_aggregate_and_summary() {
    // Prepare three distinct items in the temp dir.
    let mut dests = Vec::new();
    let pid = std::process::id();
    for i in 0..3 {
        let mut p = std::env::temp_dir();
        p.push(format!("modsync_e2e_{}_{}.bin", pid, i));
        // Ensure clean state
        let _ = std::fs::remove_file(&p);
        dests.push(p);
    }

    let sizes = vec![150_000u64, 200_000u64, 250_000u64];
    let expected_bytes: u64 = sizes.iter().copied().sum();

    let items: Vec<LfsDownloadItem> = sizes
        .iter()
        .enumerate()
        .map(|(i, s)| LfsDownloadItem {
            oid: format!("e2e-oid-{}", i),
            size: Some(*s),
            dest: dests[i].clone(),
            repo_remote: None,
        })
        .collect();

    let cfg = DownloaderConfig {
        progress_interval_ms: 100,
        coalesce_threshold_bytes: 32 * 1024,
    };

    // Use the public UI helper which internally starts the downloader and
    // forwards events to the provided closure.
    let (tx, rx) = std::sync::mpsc::channel::<ProgressEvent>();
    let on_event = move |ev: ProgressEvent| {
        let _ = tx.send(ev);
    };
    // This returns a control sender we don't need for this test.
    let _control = modsync::ui::attach_downloader_consumer(items, cfg, on_event);

    // Shared collection state captured by the collector thread.
    let agg_seen = Arc::new(Mutex::new(false));
    let agg_positive_bps = Arc::new(Mutex::new(false));
    let final_agg_bytes = Arc::new(Mutex::new(0u64));
    let completed_oids = Arc::new(Mutex::new(HashSet::new()));

    // Spawn a thread to collect events.
    let collector_handle = {
        let agg_seen = agg_seen.clone();
        let agg_positive_bps = agg_positive_bps.clone();
        let final_agg_bytes = final_agg_bytes.clone();
        let completed_oids = completed_oids.clone();
        thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(10);
            while Instant::now() < deadline {
                match rx.recv_timeout(Duration::from_millis(250)) {
                    Ok(ev) => match ev {
                        ProgressEvent::Aggregate { files_done, bytes_done, instant_bps, .. } => {
                            let mut s = agg_seen.lock().unwrap();
                            *s = true;
                            if instant_bps > 0 {
                                let mut p = agg_positive_bps.lock().unwrap();
                                *p = true;
                            }
                            if files_done == 3 {
                                let mut fb = final_agg_bytes.lock().unwrap();
                                *fb = bytes_done;
                                // If final aggregate reports all bytes, we can stop.
                                if *fb == expected_bytes {
                                    break;
                                }
                            }
                        }
                        ProgressEvent::Completed { oid, .. } => {
                            let mut co = completed_oids.lock().unwrap();
                            co.insert(oid);
                            if co.len() >= 3 {
                                // Continue a short while to collect final aggregate then exit.
                                // Small sleep to allow aggregator to emit final flush.
                                thread::sleep(Duration::from_millis(150));
                                break;
                            }
                        }
                        _ => {}
                    },
                    Err(_) => {
                        // timeout, loop again until deadline
                    }
                }
            }

            // Drain remaining non-blocking events.
            while let Ok(ev) = rx.try_recv() {
                match ev {
                    ProgressEvent::Aggregate { files_done, bytes_done, instant_bps, .. } => {
                        let mut s = agg_seen.lock().unwrap();
                        *s = true;
                        if instant_bps > 0 {
                            let mut p = agg_positive_bps.lock().unwrap();
                            *p = true;
                        }
                        if files_done == 3 {
                            let mut fb = final_agg_bytes.lock().unwrap();
                            *fb = bytes_done;
                        }
                    }
                    ProgressEvent::Completed { oid, .. } => {
                        let mut co = completed_oids.lock().unwrap();
                        co.insert(oid);
                    }
                    _ => {}
                }
            }
        })
    };

    // Wait for the collector to finish (it will stop after observing completion/final aggregate or timeout).
    let _ = collector_handle.join();

    // Allow a tiny grace period for any background cleanup.
    thread::sleep(Duration::from_millis(50));

    // Gather results from collector state.
    let saw_agg = *agg_seen.lock().unwrap();
    let saw_positive = *agg_positive_bps.lock().unwrap();
    let completed_count = completed_oids.lock().unwrap().len();
    let agg_bytes = *final_agg_bytes.lock().unwrap();

    // Cleanup temp files and any .part siblings.
    for p in dests.iter() {
        let _ = std::fs::remove_file(p);
        if let Some(fname) = p.file_name() {
            let mut os = std::ffi::OsString::from(fname);
            os.push(".part");
            if let Some(parent) = p.parent() {
                let part_path = parent.join(os);
                let _ = std::fs::remove_file(part_path);
            }
        }
    }

    assert!(saw_agg, "Expected at least one Aggregate event");
    assert!(saw_positive, "Expected an Aggregate with instant_bps > 0");
    assert_eq!(completed_count, 3, "Expected three Completed events");
    assert_eq!(agg_bytes, expected_bytes, "Final aggregate bytes_done mismatch");
}