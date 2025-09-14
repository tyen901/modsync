//! Defines the application state for the text user interface.
//!
//! The `App` structure holds all mutable state required by the UI at
//! runtime, including the configuration loaded at startup, the menu
//! entries and the current selection. Background tasks report progress
//! via the task update channel which is consumed by the event loop.

use crate::config::Config;
use anyhow::Result;
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// Application state for the terminal UI.
#[derive(Debug)]
pub struct App {
    /// Current configuration loaded at startup.  Changes made via the UI
    /// are retained in this copy and can be saved by the caller after
    /// the UI exits.
    pub config: Config,
    /// (Was: log messages) — logging has been removed; keep a
    /// lightweight modpack_state and task channels instead.
    /// Menu entries presented on the left of the screen.
    pub menu: Vec<&'static str>,
    /// Index of the currently selected menu entry.
    pub selected: usize,
    // logging removed
    /// Optional currently executing task and its progress.
    pub current_task: Option<TaskState>,
    /// Channel for background actions to send task progress updates
    /// back to the UI.  The event loop will drain this and apply
    /// updates to `current_task`.
    pub task_tx: tokio::sync::mpsc::UnboundedSender<TaskUpdate>,
    pub task_rx: tokio::sync::mpsc::UnboundedReceiver<TaskUpdate>,
    /// Lightweight representation of the current modpack state to be
    /// rendered in the right-hand panel when no task is running.
    pub modpack_state: Vec<String>,
}

impl App {
    /// Creates a new application state from an existing configuration.
    /// This also allocates an unbounded channel for background tasks to
    /// send log messages to the UI.  The menu contains the high level
    /// actions supported by the application.
    pub async fn new(config: Config) -> Result<Self> {
        let (task_tx, task_rx) = tokio::sync::mpsc::unbounded_channel();
        Ok(Self {
            config,
            // messages/logging removed
            menu: vec![
                "Sync Modpack",
                "Validate Files",
                "Check Updates",
                "Join Server",
                "Quit",
            ],
            selected: 0,
            // logging removed
            current_task: None,
            task_tx,
            task_rx,
            modpack_state: Vec::new(),
        })
    }

    /// Appends a line to the in‑memory log.  Use this for quick
    /// messages that originate from the async context.  More complex
    /// actions should use the `log_tx` channel to send messages back
    /// into the UI.
    // logging removed

    /// Apply an incoming task update to the current task/modpack state.
    pub fn apply_task_update(&mut self, upd: TaskUpdate) {
        match upd {
            TaskUpdate::Start { name, stages } => {
                self.current_task = Some(TaskState::new(name, stages));
            }
            TaskUpdate::SetDownloadList(list) => {
                if let Some(t) = &mut self.current_task {
                    t.files = list
                        .into_iter()
                        .map(|(oid, size, dest)| FileProgress {
                            oid,
                            dest,
                            total: size,
                            bytes_received: 0,
                            started_at: None,
                            completed: false,
                            elapsed: None,
                            instant_bps: None,
                            error: None,
                        })
                        .collect();
                }
            }
            TaskUpdate::DownloaderEvent(ev) => {
                use crate::downloader::ProgressEvent;
                if let Some(t) = &mut self.current_task {
                    match ev {
                        ProgressEvent::Started { oid, total, started_at } => {
                            if let Some(f) = t.files.iter_mut().find(|f| f.oid == oid) {
                                f.total = total;
                                f.bytes_received = 0;
                                f.started_at = Some(started_at);
                                f.completed = false;
                                f.error = None;
                            }
                        }
                        ProgressEvent::Progress { oid, bytes_received, chunk_bytes: _, total: _, instant_bps } => {
                            if let Some(f) = t.files.iter_mut().find(|f| f.oid == oid) {
                                f.bytes_received = bytes_received;
                                f.instant_bps = Some(instant_bps);
                            }
                        }
                        ProgressEvent::Completed { oid, path: _, total_bytes, elapsed } => {
                            if let Some(f) = t.files.iter_mut().find(|f| f.oid == oid) {
                                f.bytes_received = total_bytes;
                                f.completed = true;
                                f.elapsed = Some(elapsed);
                            }
                        }
                        ProgressEvent::Failed { oid, error } => {
                            if let Some(f) = t.files.iter_mut().find(|f| f.oid == oid) {
                                f.error = Some(error);
                                f.completed = false;
                            }
                        }
                        ProgressEvent::Aggregate { .. } => {
                            // ignore here; view may render aggregate info
                        }
                    }
                }
            }
            TaskUpdate::StageStarted(idx) => {
                if let Some(t) = &mut self.current_task {
                    t.current_stage = idx;
                    t.stage_statuses[idx] = TaskStageStatus::InProgress;
                }
            }
            TaskUpdate::StageCompleted(idx) => {
                if let Some(t) = &mut self.current_task {
                    t.stage_statuses[idx] = TaskStageStatus::Done;
                }
            }
            TaskUpdate::StageFailed(idx, msg) => {
                if let Some(t) = &mut self.current_task {
                    t.stage_statuses[idx] = TaskStageStatus::Failed(msg);
                }
            }
            TaskUpdate::Finished(state_lines) => {
                // Replace the modpack state and clear the current task.
                self.modpack_state = state_lines;
                self.current_task = None;
            }
            TaskUpdate::Aborted => {
                self.current_task = None;
            }
        }
    }
}

/// Compact representation of a task stage's status shown in the UI.
#[derive(Debug, Clone)]
pub enum TaskStageStatus {
    Pending,
    InProgress,
    Done,
    Failed(String),
}

/// Current task shown in the UI.  Holds the task name, list of stages
/// and per-stage statuses.
#[derive(Debug, Clone)]
pub struct TaskState {
    pub name: String,
    pub stages: Vec<String>,
    pub current_stage: usize,
    pub stage_statuses: Vec<TaskStageStatus>,
    /// Optional per-file download progress tracked during the sync stage.
    pub files: Vec<FileProgress>,
}

impl TaskState {
    pub fn new(name: String, stages: Vec<String>) -> Self {
        let len = stages.len();
        Self {
            name,
            stages,
            current_stage: 0,
            stage_statuses: vec![TaskStageStatus::Pending; len],
            files: Vec::new(),
        }
    }
}

/// Per-file download progress tracked while a sync task is running.
#[derive(Debug, Clone)]
pub struct FileProgress {
    pub oid: String,
    pub dest: PathBuf,
    pub total: Option<u64>,
    pub bytes_received: u64,
    pub started_at: Option<Instant>,
    pub completed: bool,
    pub elapsed: Option<Duration>,
    pub instant_bps: Option<u64>,
    pub error: Option<String>,
}

/// Updates sent from background tasks to the UI.
#[derive(Debug, Clone)]
pub enum TaskUpdate {
    Start { name: String, stages: Vec<String> },
    StageStarted(usize),
    StageCompleted(usize),
    StageFailed(usize, String),
    Finished(Vec<String>),
    /// Provide the list of files that will be downloaded (oid, size, dest)
    SetDownloadList(Vec<(String, Option<u64>, std::path::PathBuf)>),
    /// Forwarded downloader progress events for per-file updates.
    DownloaderEvent(crate::downloader::ProgressEvent),
    Aborted,
}
