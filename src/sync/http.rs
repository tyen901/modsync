// src/sync/http.rs

// This module handles HTTP client creation for downloading torrent files

use anyhow::{Context, Result};

// Helper to create a client (called once in sync_manager)
pub fn create_http_client() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .build()
        .context("Failed to build HTTP client")
} 