// This file makes the crate a library and declares modules for use
// by the binary (main.rs) and integration tests.

pub mod actions;
pub mod app;
pub mod config;
pub mod sync;
pub mod ui;

// Re-export librqbit for convenience if needed by tests or binary
pub use librqbit;
