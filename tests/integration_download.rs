use tempfile::tempdir;
use std::time::Duration;
use anyhow::Result;

#[tokio::test]
async fn integration_download_and_cache_hash_matches() -> Result<()> {
    // Use the raw.githubusercontent.com URL so we get the actual torrent bytes
    let url = "https://raw.githubusercontent.com/tyen-customs-a3/modsync_test_source/main/test_mpeg.torrent";

    // Create an HTTP client with a timeout and explicit user-agent for reliability
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .user_agent("modsync-integration-test/0.1")
        .build()
        .map_err(|e| anyhow::anyhow!(e))?;

    // Download the torrent bytes with an overall timeout so CI/tests don't hang
    let download_future = modsync::sync::utils::download_torrent(url, &client);
    let data = match tokio::time::timeout(Duration::from_secs(30), download_future).await {
        Ok(Ok(d)) => d,
        Ok(Err(e)) => return Err(anyhow::anyhow!(e)),
        Err(_) => return Err(anyhow::anyhow!("Timed out downloading torrent")),
    };

    println!("Integration test: downloaded {} bytes", data.len());

    // Basic sanity: we should have non-trivial content
    assert!(data.len() > 50, "Downloaded torrent is unexpectedly small");

    // Compute the hash
    let remote_hash = modsync::sync::utils::calculate_torrent_hash(&data);
    println!("Integration test: remote hash = {}", remote_hash);

    // Write to a temporary cache file and verify get_local_torrent_hash matches
    let dir = tempdir()?;
    let cache_path = dir.path().join("test_cache.torrent");

    // Write bytes
    tokio::fs::write(&cache_path, &data).await.map_err(|e| anyhow::anyhow!(e))?;

    // Read via helper
    let local_hash = modsync::sync::utils::get_local_torrent_hash(Some(cache_path)).await.map_err(|e| anyhow::anyhow!(e))?;

    match local_hash {
        Some(h) => {
            println!("Integration test: local hash = {}", h);
            assert_eq!(h, remote_hash, "Local cache hash should match downloaded data")
        }
        None => panic!("Expected a local hash but get_local_torrent_hash returned None"),
    }

    Ok(())
}
