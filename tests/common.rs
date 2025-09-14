use anyhow::Result;
use reqwest::blocking::Client;

// Thin test helper that delegates to the library implementation so tests
// and the core logic share the same code path.
pub fn azure_lfs_batch_download_and_write(
    client: &Client,
    pat: &str,
    repo_base: &str,
    oid: &str,
    size: u64,
    target_path: &std::path::Path,
) -> Result<()> {
    // Forward to the library helper which accepts a blocking client.
    modsync::lfs::azure_lfs_batch_download_and_write_blocking(
        client,
        pat,
        repo_base,
        oid,
        size,
        target_path,
    )
}

