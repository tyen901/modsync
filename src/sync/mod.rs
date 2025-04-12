// src/sync/mod.rs

// Declare sub-modules for sync logic
pub mod http;
pub mod state;
pub mod torrent;
pub mod cleaner;

// Potentially re-export key functions or structs if needed elsewhere
// pub use torrent::manage_torrent_task; 