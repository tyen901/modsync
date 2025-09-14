use std::cmp;
use std::collections::{HashMap, HashSet, VecDeque};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
    mpsc, Arc, Mutex,
};
use std::thread;
use std::time::{Duration, Instant};

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
        // Compute elapsed time between oldest and newest sample for a more
        // accurate instantaneous bandwidth measurement. If the elapsed time
        // is extremely small fall back to the configured window length.
        let first_time = self.samples.front().unwrap().0;
        let last_time = self.samples.back().unwrap().0;
        let mut elapsed = last_time.duration_since(first_time).as_secs_f64();
        if elapsed <= 0.0 {
            elapsed = self.window.as_secs_f64();
        }
        let total: u64 = self.samples.iter().map(|&(_, b)| b).sum();
        if elapsed == 0.0 {
            0
        } else {
            (total as f64 / elapsed).round() as u64
        }
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
    CancelFile { oid: String },
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

    // Main join handle that spawns a bounded-worker pool and waits for them and the coordinator.
    let coordinator_handle = coordinator;
    let instant_map = instant_map.clone();
    let completed_oids = completed_oids.clone();
    let completed_bytes = completed_bytes.clone();

    let handle = thread::spawn(move || -> DownloadResult {
        // We'll use a task channel and spawn up to `max_workers` worker threads.
        let max_workers = std::cmp::min(4, items.len().max(1));
        let total_items = items.len();

        // (using Condvar-backed task queue instead of channel)

        // Spawn aggregator thread that emits Aggregate events periodically.
        let agg_tx = progress_tx.clone();
        let agg_instant_map = instant_map.clone();
        let agg_completed_oids = completed_oids.clone();
        let agg_completed_bytes = completed_bytes.clone();
        let agg_worker_count = worker_count.clone();
        let agg_interval = Duration::from_millis(cfg.progress_interval_ms.max(100));
        // Capture known total bytes from the items vector
        let known_bytes_total: Option<u64> = {
            let mut acc: Option<u64> = Some(0);
            for it in &items {
                match it.size {
                    Some(s) => {
                        if let Some(a) = acc {
                            acc = Some(a.saturating_add(s));
                        }
                    }
                    None => {
                        acc = None;
                        break;
                    }
                }
            }
            acc
        };

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
                    bytes_total: known_bytes_total,
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
                bytes_total: known_bytes_total,
                bytes_done,
                instant_bps,
            });
        });

        // Spawn worker threads
        let mut worker_handles = Vec::new();
        // Build a Condvar-backed queue so each worker can block independently and be woken fairly.
        struct TaskQueue {
            queue: VecDeque<LfsDownloadItem>,
            closed: bool,
        }
        let task_queue = std::sync::Arc::new((
            std::sync::Mutex::new(TaskQueue {
                queue: VecDeque::new(),
                closed: false,
            }),
            std::sync::Condvar::new(),
        ));
        for _ in 0..max_workers {
            let tx = progress_tx.clone();
            let cancel_all = cancel_all.clone();
            let cancelled_files = cancelled_files.clone();
            let worker_count = worker_count.clone();
            let progress_interval_ms = cfg.progress_interval_ms;
            let coalesce_threshold = cfg.coalesce_threshold_bytes as u64;
            let im = instant_map.clone();
            let coids = completed_oids.clone();
            let cbytes = completed_bytes.clone();
            let task_queue = task_queue.clone();

            let worker = thread::spawn(move || -> (bool, u64) {
                // Each worker pulls tasks until the queue is closed and empty.
                loop {
                    // Attempt to pop a task from the queue, waiting on the condvar when empty.
                    let maybe_item = {
                        let (lock, cvar) = &*task_queue;
                        let mut q = lock.lock().unwrap();
                        while q.queue.is_empty() && !q.closed {
                            q = cvar.wait(q).unwrap();
                        }
                        if q.queue.is_empty() && q.closed {
                            None
                        } else {
                            q.queue.pop_front()
                        }
                    };
                    let item = match maybe_item {
                        Some(it) => it,
                        None => break, // closed and empty
                    };
                    let oid = item.oid.clone();
                    let dest = item.dest.clone();
                    let size_opt = item.size;

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
                            let mut p = dest.clone();
                            let ext = dest.extension().and_then(|e| e.to_str()).unwrap_or("");
                            p.set_extension(format!("{}{}", ext, ".part"));
                            p
                        }
                    };

                    // helper to remove partial file and send cancelled event
                    let remove_partial_and_fail = |tx: &mpsc::Sender<ProgressEvent>,
                                                   oid: &str,
                                                   im: &Arc<Mutex<HashMap<String, u64>>>,
                                                   worker_count: &Arc<AtomicUsize>|
                     -> (bool, u64) {
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
                            let _ = remove_partial_and_fail(&tx, &oid, &im, &worker_count);
                            break;
                        }

                        {
                            let guard = cancelled_files.lock().unwrap();
                            if guard.contains(&oid) {
                                let _ = remove_partial_and_fail(&tx, &oid, &im, &worker_count);
                                break;
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
                        if let Ok(mut f) = OpenOptions::new()
                            .create(true)
                            .append(true)
                            .open(&part_path)
                        {
                            let write_len = chunk as usize;
                            let max_write = 1024 * 1024; // 1 MiB
                            let buf = vec![0u8; std::cmp::min(write_len, max_write)];
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
                            let _ = remove_partial_and_fail(&tx, &oid, &im, &worker_count);
                            break;
                        }
                        {
                            let guard = cancelled_files.lock().unwrap();
                            if guard.contains(&oid) {
                                let _ = remove_partial_and_fail(&tx, &oid, &im, &worker_count);
                                break;
                            }
                        }

                        let interval_limit = Duration::from_millis(progress_interval_ms.max(100));
                        if bytes_accum >= coalesce_threshold
                            || last_send.elapsed() >= interval_limit
                        {
                            let send_bytes = bytes_accum;
                            // check once more before sending progress
                            if cancel_all.load(Ordering::SeqCst) {
                                let _ = remove_partial_and_fail(&tx, &oid, &im, &worker_count);
                                break;
                            }
                            {
                                let guard = cancelled_files.lock().unwrap();
                                if guard.contains(&oid) {
                                    let _ = remove_partial_and_fail(&tx, &oid, &im, &worker_count);
                                    break;
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
                        if cancel_all.load(Ordering::SeqCst) {
                            let _ = remove_partial_and_fail(&tx, &oid, &im, &worker_count);
                        } else {
                            let instant_bps = window.instant_bps();
                            let send_bytes = bytes_accum;
                            let _ = tx.send(ProgressEvent::Progress {
                                oid: oid.clone(),
                                bytes_received,
                                chunk_bytes: send_bytes,
                                total: size_opt,
                                instant_bps,
                            });
                            if let Ok(mut map) = im.lock() {
                                map.insert(oid.clone(), window.instant_bps());
                            }
                        }
                    }

                    // final cancellation check before marking completed
                    if cancel_all.load(Ordering::SeqCst) {
                        let _ = remove_partial_and_fail(&tx, &oid, &im, &worker_count);
                    } else {
                        let guard = cancelled_files.lock().unwrap();
                        if guard.contains(&oid) {
                            let _ = remove_partial_and_fail(&tx, &oid, &im, &worker_count);
                        } else {
                            // Attempt atomic rename of the .part file into the final destination.
                            if part_path.exists() {
                                if let Err(e) = std::fs::rename(&part_path, &dest) {
                                    if let Err(copy_err) = std::fs::copy(&part_path, &dest)
                                        .and_then(|_| std::fs::remove_file(&part_path))
                                    {
                                        let _ = tx.send(ProgressEvent::Failed {
                                            oid: oid.clone(),
                                            error: format!(
                                                "failed to move part file into place: {} / {}",
                                                e, copy_err
                                            ),
                                        });
                                        worker_count.fetch_sub(1, Ordering::SeqCst);
                                        let _ = im.lock().map(|mut m| m.remove(&oid));
                                        continue;
                                    }
                                }
                            }

                            // mark completed in shared state
                            cbytes.fetch_add(total, Ordering::SeqCst);
                            if let Ok(mut set) = coids.lock() {
                                set.insert(oid.clone());
                            }
                            let _ = im.lock().map(|mut m| m.remove(&oid));

                            let elapsed = started_at.elapsed();
                            let _ = tx.send(ProgressEvent::Completed {
                                oid: oid.clone(),
                                path: dest.clone(),
                                total_bytes: total,
                                elapsed,
                            });

                            worker_count.fetch_sub(1, Ordering::SeqCst);
                        }
                    }
                }
                (true, 0)
            });
            worker_handles.push(worker);
        }

        // Enqueue tasks into the shared queue and notify workers.
        {
            let (lock, cvar) = &*task_queue;
            let mut q = lock.lock().unwrap();
            for item in items.into_iter() {
                q.queue.push_back(item);
                cvar.notify_one();
            }
            // Mark closed so workers exit when the queue is empty.
            q.closed = true;
            cvar.notify_all();
        }

        // Drop the original sender so receiver sees only workers' clones.
        drop(progress_tx);

        for wh in worker_handles {
            let _ = wh.join();
        }

        // Wait for aggregator to finish (it will after worker_count reaches 0).
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

pub async fn execute_plan(
   client: &crate::http::AzureClient,
   plan: crate::index::SyncPlan,
   out_dir: &std::path::Path,
) -> Result<Summary, anyhow::Error> {
   // Minimal sequential implementation (KISS): download each blob and LFS object
   // one-by-one, write into out_dir/.tmp then move into place, verify sizes/hashes.

   // Local imports
   use anyhow::Context;
   use hex;
   use sha1::{Digest as Sha1Digest, Sha1};
   use sha2::{Digest as Sha2Digest, Sha256};
   use std::collections::HashMap;
   use tokio::fs;
   use tokio::io::AsyncWriteExt;

   let tmp_base = out_dir.join(".tmp");
   fs::create_dir_all(&tmp_base)
       .await
       .with_context(|| format!("creating tmp dir {}", tmp_base.display()))?;

   let mut blobs_downloaded: usize = 0;
   let mut lfs_downloaded: usize = 0;
   let mut bytes_done: u64 = 0;

   // Blobs (git SHA-1)
   for (path, entry) in plan.blobs.into_iter() {
       let bytes = client
           .get_blob_by_oid(&entry.oid)
           .await
           .with_context(|| format!("failed to download blob {}", entry.oid))?;

       let dest_path = out_dir.join(&path);
       if let Some(parent) = dest_path.parent() {
           fs::create_dir_all(parent).await.ok();
       }

       let part = tmp_base.join(path.with_extension("part"));
       if let Some(parent) = part.parent() {
           fs::create_dir_all(parent).await.ok();
       }
       let mut f = fs::File::create(&part)
           .await
           .with_context(|| format!("create part file {}", part.display()))?;
       f.write_all(&bytes).await?;
       f.flush().await.ok();

       let got_len = bytes.len() as u64;
       if got_len != entry.size {
           return Err(anyhow::anyhow!(
               "blob size mismatch for {}: expected {}, got {}",
               path.display(),
               entry.size,
               got_len
           ));
       }

       let mut hasher = Sha1::new();
       hasher.update(format!("blob {}\u{0}", got_len).as_bytes());
       hasher.update(&bytes);
       let got_oid = hex::encode(hasher.finalize());
       if !entry.is_lfs && got_oid != entry.oid {
           return Err(anyhow::anyhow!(
               "blob oid mismatch for {}: expected {}, got {}",
               path.display(),
               entry.oid,
               got_oid
           ));
       }

       fs::create_dir_all(dest_path.parent().unwrap_or_else(|| std::path::Path::new(".")))
           .await
           .ok();
       fs::rename(&part, &dest_path)
           .await
           .with_context(|| format!("rename {} -> {}", part.display(), dest_path.display()))?;

       blobs_downloaded += 1;
       bytes_done = bytes_done.saturating_add(got_len);
   }

   // LFS objects (group by oid)
   if !plan.lfs.is_empty() {
       let mut oid_map: HashMap<String, Vec<(PathBuf, Option<u64>)>> = HashMap::new();
       for (path, entry) in plan.lfs.into_iter() {
           oid_map
               .entry(entry.oid.clone())
               .or_default()
               .push((path, Some(entry.size)));
       }

       let mut objs: Vec<crate::http::LfsObject> = Vec::new();
       for (oid, items) in oid_map.iter() {
           let size = items.first().and_then(|(_, s)| *s);
           objs.push(crate::http::LfsObject {
               oid: oid.clone(),
               size,
           });
       }

       let batch_req = crate::http::LfsBatchRequest {
           operation: "download".to_string(),
           objects: objs,
       };
       let batch_resp = client
           .lfs_batch(batch_req)
           .await
           .with_context(|| "lfs batch request failed")?;

       for obj in batch_resp.objects.into_iter() {
           let oid = obj.oid;
           let size = obj.size;
           let href_opt = obj
               .actions
               .and_then(|mut acts| acts.remove("download").and_then(|a| a.href));
           if href_opt.is_none() {
               continue;
           }
           let href = href_opt.unwrap();

           let resp = client
               .client
               .get(&href)
               .send()
               .await
               .with_context(|| format!("lfs GET failed for {}", href))?;
           let status = resp.status();
           let bytes = resp
               .bytes()
               .await
               .with_context(|| format!("failed to read lfs body for {}", href))?;
           if !status.is_success() {
               return Err(anyhow::anyhow!("lfs GET non-success {}: {}", href, status));
           }

           if let Some(expected) = size {
               if bytes.len() as u64 != expected {
                   return Err(anyhow::anyhow!(
                       "lfs size mismatch for oid {}: expected {}, got {}",
                       oid,
                       expected,
                       bytes.len()
                   ));
               }
           }

           let mut hasher = Sha256::new();
           hasher.update(&bytes);
           let got = hex::encode(hasher.finalize());
           if got != oid {
               return Err(anyhow::anyhow!(
                   "lfs oid mismatch for oid {}: expected {}, got {}",
                   oid,
                   oid,
                   got
               ));
           }

           let part = tmp_base.join(format!("{}.part", oid));
           if let Some(parent) = part.parent() {
               fs::create_dir_all(parent).await.ok();
           }
           let mut f = fs::File::create(&part)
               .await
               .with_context(|| format!("create lfs part {}", part.display()))?;
           f.write_all(&bytes).await?;
           f.flush().await.ok();

           if let Some(paths) = oid_map.get(&oid) {
               for (path, _) in paths {
                   let dest_path = out_dir.join(path);
                   if let Some(parent) = dest_path.parent() {
                       fs::create_dir_all(parent).await.ok();
                   }
                   fs::copy(&part, &dest_path)
                       .await
                       .with_context(|| format!("copy {} -> {}", part.display(), dest_path.display()))?;
                   lfs_downloaded += 1;
                   bytes_done = bytes_done.saturating_add(bytes.len() as u64);
               }
           }
           let _ = fs::remove_file(&part).await;
       }
   }

   Ok(Summary {
       files_done: blobs_downloaded + lfs_downloaded,
       bytes_done,
   })
}
