// src/sync/mod.rs

// Declare sub-modules for sync logic
pub mod http;
pub mod cleaner;
pub mod torrent;

// New modular structure
pub mod messages;
pub mod types;
pub mod utils;
pub mod local;
pub mod remote;
pub mod manager;

// Re-export key types and functions for external use
pub use messages::{SyncCommand, SyncEvent};
pub use manager::run_sync_manager;

// Potentially re-export key functions or structs if needed elsewhere
pub use torrent::manage_torrent_task; 