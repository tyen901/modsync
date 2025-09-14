use std::cmp;
use std::collections::{HashMap, HashSet, VecDeque};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc, Arc, Mutex,
};
use std::thread;
use std::time::{Duration, Instant};

struct SlidingWindow {
    window: Duration,
    samples: VecDeque<(Instant, u64)>,
}

impl SlidingWindow {
    fn new(window_ms: u64) -> Self {
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
        if self.samples.len() < 2 {
            return 1;
        }
        let first = self.samples.front().unwrap().0;
        let last = self.samples.back().unwrap().0;
        let elapsed = last.duration_since(first).as_secs_f64().max(1e-6);
        let total: u64 = self.samples.iter().map(|&(_, b)| b).sum();
        (total as f64 / elapsed).round() as u64
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
    mpsc::Receiver<ProgressEvent>,
    mpsc::Sender<ControlCommand>,
    thread::JoinHandle<DownloadResult>,
) {
    let (progress_tx, progress_rx) = mpsc::channel::<ProgressEvent>();
    let (control_tx, control_rx) = mpsc::channel::<ControlCommand>();

    let cancel_all = Arc::new(AtomicBool::new(false));
    let cancelled_files = Arc::new(Mutex::new(HashSet::new()));

    let handle = {
        let progress_tx = progress_tx.clone();
        let cancel_all = cancel_all.clone();
        let cancelled_files = cancelled_files.clone();
        thread::spawn(move || -> DownloadResult {
            let mut files_done = 0usize;
            let mut bytes_done = 0u64;

            for item in items.into_iter() {
                // check control messages non-blocking
                while let Ok(cmd) = control_rx.try_recv() {
                    match cmd {
                        ControlCommand::CancelAll => {
                            cancel_all.store(true, Ordering::SeqCst);
                        }
                        ControlCommand::CancelFile { oid } => {
                            if let Ok(mut s) = cancelled_files.lock() {
                                s.insert(oid);
                            }
                        }
                    }
                }

                if cancel_all.load(Ordering::SeqCst) {
                    let _ = progress_tx.send(ProgressEvent::Failed {
                        oid: item.oid.clone(),
                        error: "cancelled".into(),
                    });
                    break;
                }

                let started = Instant::now();
                let _ = progress_tx.send(ProgressEvent::Started {
                    oid: item.oid.clone(),
                    total: item.size,
                    started_at: started,
                });

                let total = item.size.unwrap_or(128 * 1024);
                let mut remaining = total;
                let mut received = 0u64;
                let chunk = cmp::min(64 * 1024, total);
                let sleep_ms = cfg.progress_interval_ms.max(20);
                let mut window = SlidingWindow::new(cfg.progress_interval_ms.max(50));

                let part_path = {
                    let mut p = item.dest.clone();
                    if let Some(fname) = p.file_name().map(|s| s.to_owned()) {
                        let mut os = fname;
                        os.push(".part");
                        if let Some(parent) = item.dest.parent() {
                            parent.join(os)
                        } else {
                            PathBuf::from(os)
                        }
                    } else {
                        p.set_extension("part");
                        p
                    }
                };
                if let Some(parent) = part_path.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                let mut f = OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&part_path)
                    .ok();

                let mut cancelled_this = false;
                while remaining > 0 {
                    // check control messages
                    while let Ok(cmd) = control_rx.try_recv() {
                        match cmd {
                            ControlCommand::CancelAll => {
                                cancel_all.store(true, Ordering::SeqCst);
                            }
                            ControlCommand::CancelFile { oid } => {
                                if let Ok(mut s) = cancelled_files.lock() {
                                    s.insert(oid);
                                }
                            }
                        }
                    }
                    if cancel_all.load(Ordering::SeqCst) {
                        let _ = std::fs::remove_file(&part_path);
                        let _ = progress_tx.send(ProgressEvent::Failed {
                            oid: item.oid.clone(),
                            error: "cancelled".into(),
                        });
                        cancelled_this = true;
                        break;
                    }
                    {
                        let guard = cancelled_files.lock().unwrap();
                        if guard.contains(&item.oid) {
                            let _ = std::fs::remove_file(&part_path);
                            let _ = progress_tx.send(ProgressEvent::Failed {
                                oid: item.oid.clone(),
                                error: "cancelled".into(),
                            });
                            cancelled_this = true;
                            break;
                        }
                    }

                    let this_chunk = cmp::min(chunk as u64, remaining);
                    thread::sleep(Duration::from_millis(sleep_ms / 2));
                    window.add(this_chunk);
                    received += this_chunk;
                    remaining = remaining.saturating_sub(this_chunk);

                    if let Some(ref mut fh) = f {
                        let to_write = std::cmp::min(this_chunk as usize, 64 * 1024);
                        let buf = vec![0u8; to_write];
                        let _ = fh.write_all(&buf);
                        let _ = fh.flush();
                    }

                    let instant_bps = window.instant_bps();
                    let _ = progress_tx.send(ProgressEvent::Progress {
                        oid: item.oid.clone(),
                        bytes_received: received,
                        chunk_bytes: this_chunk,
                        total: Some(total),
                        instant_bps,
                    });
                }

                if cancelled_this {
                    if cancel_all.load(Ordering::SeqCst) {
                        break;
                    } else {
                        continue;
                    }
                }

                // finalize
                if let Some(mut fh) = f {
                    let _ = fh.sync_all();
                }
                if let Err(e) = std::fs::rename(&part_path, &item.dest) {
                    if let Err(_) = std::fs::copy(&part_path, &item.dest)
                        .and_then(|_| std::fs::remove_file(&part_path))
                    {
                        let _ = progress_tx.send(ProgressEvent::Failed {
                            oid: item.oid.clone(),
                            error: format!("failed to move part file into place: {}", e),
                        });
                        continue;
                    }
                }

                let elapsed = started.elapsed();
                let _ = progress_tx.send(ProgressEvent::Completed {
                    oid: item.oid.clone(),
                    path: item.dest.clone(),
                    total_bytes: total,
                    elapsed,
                });

                files_done += 1;
                bytes_done = bytes_done.saturating_add(received);
            }

            Ok(Summary {
                files_done,
                bytes_done,
            })
        })
    };

    (progress_rx, control_tx, handle)
}

pub async fn execute_plan(
    client: &crate::http::AzureClient,
    plan: crate::index::SyncPlan,
    out_dir: &std::path::Path,
) -> Result<Summary, anyhow::Error> {
    use anyhow::Context;
    use hex;
    use sha1::{Digest as Sha1Digest, Sha1};
    use sha2::{Digest as Sha2Digest, Sha256};
    use tokio::fs;
    use tokio::io::AsyncWriteExt;

    let tmp_base = out_dir.join(".tmp");
    fs::create_dir_all(&tmp_base)
        .await
        .with_context(|| format!("creating tmp dir {}", tmp_base.display()))?;

    let mut blobs_downloaded: usize = 0;
    let mut lfs_downloaded: usize = 0;
    let mut bytes_done: u64 = 0;

    for (path, entry) in plan.blobs.into_iter() {
        let bytes = client
            .get_blob_by_oid(&entry.oid)
            .await
            .context("get_blob_by_oid failed")?;
        let dest_path = out_dir.join(&path);
        if let Some(parent) = dest_path.parent() {
            fs::create_dir_all(parent).await.ok();
        }
        let part = tmp_base.join(path.with_extension("part"));
        if let Some(parent) = part.parent() {
            fs::create_dir_all(parent).await.ok();
        }
        let mut f = fs::File::create(&part).await.context("create part")?;
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
        fs::rename(&part, &dest_path).await.context("rename")?;
        blobs_downloaded += 1;
        bytes_done = bytes_done.saturating_add(got_len);
    }

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
            .context("lfs batch failed")?;
        for obj in batch_resp.objects.into_iter() {
            let oid = obj.oid;
            let size = obj.size;
            let href_opt = obj
                .actions
                .and_then(|mut acts| acts.remove("download").and_then(|a| a.href));
            let href = match href_opt {
                Some(h) => h,
                None => continue,
            };
            let resp = client
                .client
                .get(&href)
                .send()
                .await
                .context("lfs get failed")?;
            let status = resp.status();
            let bytes = resp.bytes().await.context("read body")?;
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
            let mut f = fs::File::create(&part).await.context("create lfs part")?;
            f.write_all(&bytes).await?;
            f.flush().await.ok();
            if let Some(paths) = oid_map.get(&oid) {
                for (path, _) in paths {
                    let dest_path = out_dir.join(path);
                    if let Some(parent) = dest_path.parent() {
                        fs::create_dir_all(parent).await.ok();
                    }
                    fs::copy(&part, &dest_path).await.context("copy")?;
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
