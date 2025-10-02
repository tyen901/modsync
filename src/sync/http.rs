use anyhow::{Context, Result};

pub fn create_http_client() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .build()
        .context("Failed to build HTTP client")
}