use std::path::PathBuf;
use std::time::{Duration, Instant};
use std::collections::{VecDeque, HashSet, HashMap};
use std::sync::{Arc, Mutex, mpsc, atomic::{AtomicBool, AtomicUsize, AtomicU64, Ordering}};
use std::thread;
use std::cmp;
use std::fs::OpenOptions;
use std::io::Write;

struct SlidingWindow {
    window: Duration,
    samples: VecDeque<(Instant, u64)>,
}

impl SlidingWindow {
    fn new(window_ms: u64) -> SlidingWindow {
        SlidingWindow {
            window: Duration::from_millis(window_ms),
            samples: VecDeque::new(),
        }
    }

    fn add(&mut self, bytes: u64) {
        let now = Instant::now();
        self.samples.push_back((now, bytes));
        let cutoff = now - self.window;
        while matches!(self.samples.front(), Some((t, _)) if *t < cutoff) {
            self.samples.pop_front();
        }
    }

    fn instant_bps(&self) -> u64 {
        if self.samples.is_empty() {
            return 0;
        }
        let secs = self.window.as_secs_f64();
        if secs == 0.0 {
            return 0;
        }
        let total: u64 = self.samples.iter().map(|&(_, b)| b).sum();
        (total as f64 / secs).round() as u64
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LfsDownloadItem {
    pub oid: String,
    pub size: Option<u64>,
    pub dest: PathBuf,
    pub repo_remote: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProgressEvent {
    Started {
        oid: String,
        total: Option<u64>,
        started_at: Instant,
    },
    Progress {
        oid: String,
        bytes_received: u64,
        chunk_bytes: u64,
        total: Option<u64>,
        instant_bps: u64,
    },
    Completed {
        oid: String,
        path: PathBuf,
        total_bytes: u64,
        elapsed: Duration,
    },
    Failed {
        oid: String,
        error: String,
    },
    Aggregate {
        files_total: usize,
        files_done: usize,
        bytes_total: Option<u64>,
        bytes_done: u64,
        instant_bps: u64,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ControlCommand {
    CancelAll,
    CancelFile {
        oid: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DownloaderConfig {
    pub progress_interval_ms: u64,
    pub coalesce_threshold_bytes: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Summary {
    pub files_done: usize,
    pub bytes_done: u64,
}
unsafe impl Send for Summary {}
unsafe impl Sync for Summary {}

pub type DownloadResult = Result<Summary, Box<dyn std::error::Error + Send + Sync>>;

pub fn start_download_job(
    items: Vec<LfsDownloadItem>,
    cfg: DownloaderConfig,
) -> (
    std::sync::mpsc::Receiver<ProgressEvent>,
    std::sync::mpsc::Sender<ControlCommand>,
    std::thread::JoinHandle<DownloadResult>,
) {
    let (progress_tx, progress_rx) = mpsc::channel::<ProgressEvent>();
    let (control_tx, control_rx) = mpsc::channel::<ControlCommand>();

    // Shared cancellation state
    let cancel_all = Arc::new(AtomicBool::new(false));
    let cancelled_files = Arc::new(Mutex::new(HashSet::new()));
    let worker_count = Arc::new(AtomicUsize::new(items.len()));
    // Shared aggregator state
    let instant_map = Arc::new(Mutex::new(HashMap::<String, u64>::new()));
    let completed_oids = Arc::new(Mutex::new(HashSet::<String>::new()));
    let completed_bytes = Arc::new(AtomicU64::new(0));

    // Coordinator thread: updates cancel flags based on control commands.
    let coordinator = {
        let cancel_all = cancel_all.clone();
        let cancelled_files = cancelled_files.clone();
        thread::spawn(move || {
            // Process control messages until the sender side is dropped (recv returns Err).
            while let Ok(cmd) = control_rx.recv() {
                match cmd {
                    ControlCommand::CancelAll => {
                        cancel_all.store(true, Ordering::SeqCst);
                    }
                    ControlCommand::CancelFile { oid } => {
                        if let Ok(mut set) = cancelled_files.lock() {
                            set.insert(oid);
                        }
                    }
                }
                // continue processing until control_rx is closed
            }
            // When control_rx is closed, just exit the coordinator.
        })
    };

    // Main join handle that spawns per-file workers and waits for them and the coordinator.
    let coordinator_handle = coordinator;
    let instant_map = instant_map.clone();
    let completed_oids = completed_oids.clone();
    let completed_bytes = completed_bytes.clone();
    let handle = thread::spawn(move || -> DownloadResult {
        let mut worker_handles = Vec::new();

        let total_items = items.len();
        // Spawn aggregator thread that emits Aggregate events periodically.
        let agg_tx = progress_tx.clone();
        let agg_instant_map = instant_map.clone();
        let agg_completed_oids = completed_oids.clone();
        let agg_completed_bytes = completed_bytes.clone();
        let agg_worker_count = worker_count.clone();
        let agg_interval = Duration::from_millis(cfg.progress_interval_ms.max(100));
        let aggregator = thread::spawn(move || {
            while agg_worker_count.load(Ordering::SeqCst) > 0 {
                thread::sleep(agg_interval);
                let files_total = total_items;
                let files_done = agg_completed_oids.lock().map(|s| s.len()).unwrap_or(0);
                let bytes_done = agg_completed_bytes.load(Ordering::SeqCst);
                let instant_bps = {
                    let im = agg_instant_map.lock().unwrap();
                    im.values().copied().sum()
                };
                let _ = agg_tx.send(ProgressEvent::Aggregate {
                    files_total,
                    files_done,
                    bytes_total: None,
                    bytes_done,
                    instant_bps,
                });
            }
            // Final flush after workers finished
            let files_total = total_items;
            let files_done = agg_completed_oids.lock().map(|s| s.len()).unwrap_or(0);
            let bytes_done = agg_completed_bytes.load(Ordering::SeqCst);
            let instant_bps = {
                let im = agg_instant_map.lock().unwrap();
                im.values().copied().sum()
            };
            let _ = agg_tx.send(ProgressEvent::Aggregate {
                files_total,
                files_done,
                bytes_total: None,
                bytes_done,
                instant_bps,
            });
        });
 
        for item in items.into_iter() {
            let tx = progress_tx.clone();
            let cancel_all = cancel_all.clone();
            let cancelled_files = cancelled_files.clone();
            let worker_count = worker_count.clone();
            let progress_interval_ms = cfg.progress_interval_ms;
            let coalesce_threshold = cfg.coalesce_threshold_bytes as u64;
            let oid = item.oid.clone();
            let dest = item.dest.clone();
            let size_opt = item.size;

            let im = instant_map.clone();
            let coids = completed_oids.clone();
            let cbytes = completed_bytes.clone();
            let worker = thread::spawn(move || -> (bool, u64) {
                let started_at = Instant::now();
                let _ = tx.send(ProgressEvent::Started {
                    oid: oid.clone(),
                    total: size_opt,
                    started_at,
                });
 
                let total = size_opt.unwrap_or(256 * 1024);
                let mut remaining = total;
                let mut bytes_received = 0u64;
                let mut window = SlidingWindow::new(progress_interval_ms.max(50));
                let sleep_dur = Duration::from_millis(progress_interval_ms.max(50) / 2);
 
                // Coalescing state
                let mut bytes_accum: u64 = 0;
                let mut last_send = Instant::now();
 
                // Construct a .part path sibling for atomic writes.
                let part_path = match dest.file_name() {
                    Some(fname) => {
                        let mut os = std::ffi::OsString::from(fname);
                        os.push(".part");
                        if let Some(parent) = dest.parent() {
                            parent.join(os)
                        } else {
                            PathBuf::from(os)
                        }
                    }
                    None => {
                        // Fallback: use dest with added .part extension
                        let mut p = dest.clone();
                        let ext = dest.extension().and_then(|e| e.to_str()).unwrap_or("");
                        p.set_extension(format!("{}{}", ext, ".part"));
                        p
                    }
                };

                // helper to remove partial file and send cancelled event
                let remove_partial_and_fail = |tx: &mpsc::Sender<ProgressEvent>, oid: &str, im: &Arc<Mutex<HashMap<String, u64>>>, worker_count: &Arc<AtomicUsize>| -> (bool, u64) {
                    let _ = std::fs::remove_file(&part_path);
                    let _ = tx.send(ProgressEvent::Failed {
                        oid: oid.to_string(),
                        error: "cancelled".into(),
                    });
                    worker_count.fetch_sub(1, Ordering::SeqCst);
                    let _ = im.lock().map(|mut m| m.remove(oid));
                    (false, 0)
                };
 
                while remaining > 0 {
                    // quick cancel checks before any work
                    if cancel_all.load(Ordering::SeqCst) {
                        return remove_partial_and_fail(&tx, &oid, &im, &worker_count);
                    }
 
                    {
                        let guard = cancelled_files.lock().unwrap();
                        if guard.contains(&oid) {
                           return remove_partial_and_fail(&tx, &oid, &im, &worker_count);
                        }
                    }
 
                    let chunk = cmp::min(64 * 1024u64, remaining);
                    thread::sleep(sleep_dur);
                    window.add(chunk);
                    bytes_received += chunk;
                    remaining -= chunk;
                    bytes_accum += chunk;

                    // Ensure parent directory exists for the part file.
                    if let Some(parent) = part_path.parent() {
                        let _ = std::fs::create_dir_all(parent);
                    }

                    // Append the simulated chunk bytes to the .part file so partial
                    // state exists on disk in case of interruption.
                    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(&part_path) {
                        // Write zeroed bytes (simulation). In real downloader this
                        // would be the actual bytes read from the network.
                        let write_len = chunk as usize;
                        // Guard against extremely large writes in tests; cap reasonably.
                        let max_write = 1024 * 1024; // 1 MiB
                        let buf = vec![0u8; std::cmp::min(write_len, max_write)];
                        // If chunk is larger than buf, write in a few iterations.
                        let mut remaining_to_write = write_len;
                        while remaining_to_write > 0 {
                            let cur = std::cmp::min(remaining_to_write, buf.len());
                            let _ = f.write_all(&buf[..cur]);
                            remaining_to_write -= cur;
                        }
                        let _ = f.flush();
                        #[cfg(unix)]
                        let _ = f.sync_all();
                    }
 
                    // always update instant map with latest instant bps
                    let instant_bps = window.instant_bps();
                    {
                        if let Ok(mut map) = im.lock() {
                            map.insert(oid.clone(), instant_bps);
                        }
                    }
 
                    // Check cancellation between sleeps and before sending progress
                    if cancel_all.load(Ordering::SeqCst) {
                        return remove_partial_and_fail(&tx, &oid, &im, &worker_count);
                    }
                    {
                        let guard = cancelled_files.lock().unwrap();
                        if guard.contains(&oid) {
                          return remove_partial_and_fail(&tx, &oid, &im, &worker_count);
                        }
                    }
 
                    let interval_limit = Duration::from_millis(progress_interval_ms.max(100));
                    if bytes_accum >= coalesce_threshold || last_send.elapsed() >= interval_limit {
                        let send_bytes = bytes_accum;
                        // check once more before sending progress
                        if cancel_all.load(Ordering::SeqCst) {
                           return remove_partial_and_fail(&tx, &oid, &im, &worker_count);
                        }
                        {
                            let guard = cancelled_files.lock().unwrap();
                            if guard.contains(&oid) {
                              return remove_partial_and_fail(&tx, &oid, &im, &worker_count);
                            }
                        }
                        let _ = tx.send(ProgressEvent::Progress {
                            oid: oid.clone(),
                            bytes_received,
                            chunk_bytes: send_bytes,
                            total: size_opt,
                            instant_bps,
                        });
                        last_send = Instant::now();
                        bytes_accum = 0;
                    }
                }
 
                // final flush of any accumulated progress
                if bytes_accum > 0 {
                    // check cancellation before final flush
                    if cancel_all.load(Ordering::SeqCst) {
                        return remove_partial_and_fail(&tx, &oid, &im, &worker_count);
                    }
                    {
                        let guard = cancelled_files.lock().unwrap();
                        if guard.contains(&oid) {
                           return remove_partial_and_fail(&tx, &oid, &im, &worker_count);
                        }
                    }
 
                    let instant_bps = window.instant_bps();
                    let send_bytes = bytes_accum;
                    let _ = tx.send(ProgressEvent::Progress {
                        oid: oid.clone(),
                        bytes_received,
                        chunk_bytes: send_bytes,
                        total: size_opt,
                        instant_bps,
                    });
                    // update instant map one last time
                    if let Ok(mut map) = im.lock() {
                        map.insert(oid.clone(), window.instant_bps());
                    }
                }
 
                // final cancellation check before marking completed
                if cancel_all.load(Ordering::SeqCst) {
                    return remove_partial_and_fail(&tx, &oid, &im, &worker_count);
                }
                {
                    let guard = cancelled_files.lock().unwrap();
                    if guard.contains(&oid) {
                        return remove_partial_and_fail(&tx, &oid, &im, &worker_count);
                    }
                }
 
                // Attempt atomic rename of the .part file into the final destination.
                if part_path.exists() {
                    if let Err(e) = std::fs::rename(&part_path, &dest) {
                        // On platforms where rename can't replace an existing file
                        // (Windows), try copy + remove as a fallback.
                        if let Err(copy_err) = std::fs::copy(&part_path, &dest).and_then(|_| std::fs::remove_file(&part_path)) {
                            let _ = tx.send(ProgressEvent::Failed {
                                oid: oid.clone(),
                                error: format!("failed to move part file into place: {} / {}", e, copy_err),
                            });
                            worker_count.fetch_sub(1, Ordering::SeqCst);
                            let _ = im.lock().map(|mut m| m.remove(&oid));
                            return (false, 0);
                        }
                    }
                }

                // mark completed in shared state
                cbytes.fetch_add(total, Ordering::SeqCst);
                if let Ok(mut set) = coids.lock() {
                    set.insert(oid.clone());
                }
                // remove instant entry to avoid stale contribution
                let _ = im.lock().map(|mut m| m.remove(&oid));
 
                let elapsed = started_at.elapsed();
                let _ = tx.send(ProgressEvent::Completed {
                    oid: oid.clone(),
                    path: dest.clone(),
                    total_bytes: total,
                    elapsed,
                });
 
                worker_count.fetch_sub(1, Ordering::SeqCst);
                (true, total)
            });
 
            worker_handles.push(worker);
        }

        // Drop the original sender so receiver sees only workers' clones.
        drop(progress_tx);
 
        for wh in worker_handles {
            let _ = wh.join();
        }
 
        // Wait for aggregator to finish (it will after worker_count reaches 0).
        // aggregator was spawned earlier as `aggregator`.
        // Join aggregator thread by moving its handle out of the scope via shadowing.
        // Note: aggregator variable exists in this scope.
        let _ = aggregator.join();
 
        // Wait for coordinator to exit (it will when worker_count==0).
        let _ = coordinator_handle.join();
 
        // Build summary from shared completed state
        let files_done = completed_oids.lock().map(|s| s.len()).unwrap_or(0);
        let bytes_done = completed_bytes.load(Ordering::SeqCst);
 
        Ok(Summary {
            files_done,
            bytes_done,
        })
    });
 
    (progress_rx, control_tx, handle)
}