//! Defines the application state for the text user interface.
//!
//! The `App` structure holds all mutable state required by the UI at
//! runtime, including the configuration loaded at startup, the menu
//! entries and the current selection. Background tasks report progress
//! via the task update channel which is consumed by the event loop.

use crate::config::Config;
use anyhow::Result;

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
}

impl TaskState {
    pub fn new(name: String, stages: Vec<String>) -> Self {
        let len = stages.len();
        Self {
            name,
            stages,
            current_stage: 0,
            stage_statuses: vec![TaskStageStatus::Pending; len],
        }
    }
}

/// Updates sent from background tasks to the UI.
#[derive(Debug, Clone)]
pub enum TaskUpdate {
    Start { name: String, stages: Vec<String> },
    StageStarted(usize),
    StageCompleted(usize),
    StageFailed(usize, String),
    Finished(Vec<String>),
    Aborted,
}
