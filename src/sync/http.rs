// src/sync/http.rs

// This module will handle fetching the remote torrent file
// and checking for changes (e.g., using ETag or Last-Modified headers).

use anyhow::{anyhow, Context, Result};
use reqwest::header::{HeaderValue, ETAG, IF_NONE_MATCH};
use reqwest::StatusCode;

// Structure to represent the check result
#[derive(Debug)] // Added Debug for logging
pub struct RemoteCheckResult {
    pub needs_update: bool,
    pub torrent_content: Option<Vec<u8>>, // Content only if update needed
    pub etag: Option<String>,
    // Add other relevant info like last_modified if needed
}

// Function to check the remote URL
pub async fn check_remote_torrent(
    url: &str,
    current_etag: Option<&str>,
    client: &reqwest::Client, // Pass client for potential reuse
) -> Result<RemoteCheckResult> {
    println!(
        "Sync: Checking remote torrent URL: {} (Current ETag: {:?})",
        url,
        current_etag
    );

    let mut request_builder = client.get(url);
    if let Some(etag) = current_etag {
        // Add If-None-Match header if we have a previous ETag
        match etag.parse::<HeaderValue>() {
            Ok(header_value) => {
                request_builder = request_builder.header(IF_NONE_MATCH, header_value);
            }
            Err(e) => {
                eprintln!("Warning: Failed to parse current ETag '{}': {}. Proceeding without If-None-Match.", etag, e);
            }
        }
    }

    let response = request_builder
        .send()
        .await
        .with_context(|| format!("Failed to send request to {}", url))?;

    match response.status() {
        StatusCode::NOT_MODIFIED => {
            println!("Sync: Remote torrent not modified (304).");
            Ok(RemoteCheckResult {
                needs_update: false,
                torrent_content: None,
                etag: current_etag.map(String::from), // Keep the old ETag
            })
        }
        StatusCode::OK => {
            println!("Sync: Remote torrent modified or first check (200).");
            let new_etag = response
                .headers()
                .get(ETAG)
                .and_then(|val| val.to_str().ok())
                .map(String::from);
            println!("Sync: New ETag: {:?}", new_etag);
            let torrent_content = response
                .bytes()
                .await
                .with_context(|| format!("Failed to read response body from {}", url))?;
            Ok(RemoteCheckResult {
                needs_update: true,
                torrent_content: Some(torrent_content.to_vec()),
                etag: new_etag,
            })
        }
        other_status => {
            let error_body = response.text().await.unwrap_or_else(|_| "Failed to read error body".to_string());
            Err(anyhow!(
                "HTTP request to {} failed with status: {}. Body: {}",
                url,
                other_status,
                error_body
            ))
        }
    }
}

// Helper to create a client (could be called once in sync_manager)
pub fn create_http_client() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .build()
        .context("Failed to build HTTP client")
} 