//! Functions for synchronising and validating a modpack based on a Git
//! repository containing Git LFS pointers.
//!
//! A modpack repository contains small text files (LFS pointer files) that
//! describe the SHA‑256 of the actual mod file stored on an LFS server.
//! Instead of using Git LFS to automatically download these large files we
//! compare the pointer SHA against the contents of the local mod folder.  If
//! the local file is missing or has a different hash we mark it for
//! download.  Downloading the content itself is outside the scope of this
//! example; in a real implementation you might call out to `git lfs fetch`
//! or implement your own download mechanism.

use anyhow::{Context, Result};
use git2 as git2_crate;
use sha2::{Digest, Sha256};
use std::ffi::OsStr;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

/// Parses a Git LFS pointer file and extracts the SHA‑256 of the content it
/// refers to.  A typical pointer file looks like this:
///
/// ```text
/// version https://git-lfs.github.com/spec/v1
/// oid sha256:abcd1234...
/// size 12345
/// ```
///
/// Returns `None` if the file does not appear to be a pointer file.
/// Parsed LFS pointer information (oid and optional size).
pub struct LfsPointer {
    pub oid: String,
    pub size: Option<u64>,
}

pub fn parse_lfs_pointer_file(path: &Path) -> Result<Option<LfsPointer>> {
    // Read raw bytes so that pointer parsing is robust even when the file
    // contains non-UTF-8 content. We'll perform ASCII-lowercasing on a
    // copy of the bytes to allow case-insensitive searches while still
    // extracting the original byte slices for values.
    let data = fs::read(path)
        .with_context(|| format!("Failed to open potential pointer file {}", path.display()))?;

    // Create a lowercase view of the data for case-insensitive matching of
    // ASCII keywords. Non-ASCII bytes are left unchanged.
    let mut lower = data.clone();
    for byte in &mut lower {
        if *byte >= b'A' && *byte <= b'Z' {
            *byte = byte.to_ascii_lowercase();
        }
    }

    // Ensure file contains the version prefix somewhere (case-insensitive).
    let version_prefix = b"version https://git-lfs.github.com/spec/";
    if !lower
        .windows(version_prefix.len())
        .any(|w| w == version_prefix)
    {
        return Ok(None);
    }

    // Search for the "oid sha256:" marker in the lowercase view.
    let needle = b"oid sha256:";
    let mut found_oid: Option<String> = None;
    if let Some(pos) = lower.windows(needle.len()).position(|w| w == needle) {
        let hex_start = pos + needle.len();
        let mut hex_end = hex_start;
        while hex_end < lower.len() {
            let c = lower[hex_end];
            if c.is_ascii_whitespace() {
                break;
            }
            hex_end += 1;
        }
        let hex_bytes = &data[hex_start..hex_end];
        // Hex should be ASCII; normalize to lowercase so comparisons are
        // case-insensitive.
        let hex_str = String::from_utf8_lossy(hex_bytes)
            .trim()
            .to_ascii_lowercase();
        if !hex_str.is_empty() {
            found_oid = Some(hex_str);
        }
    }

    // Try to parse size line if present (case-insensitive search for "size ").
    let size_needle = b"size ";
    let mut found_size: Option<u64> = None;
    if let Some(pos) = lower
        .windows(size_needle.len())
        .position(|w| w == size_needle)
    {
        let num_start = pos + size_needle.len();
        let mut num_end = num_start;
        while num_end < lower.len() {
            let c = lower[num_end];
            if c == b'\n' || c == b'\r' || c.is_ascii_whitespace() {
                break;
            }
            num_end += 1;
        }
        if num_end > num_start {
            if let Ok(s) = String::from_utf8_lossy(&data[num_start..num_end])
                .trim()
                .parse::<u64>()
            {
                found_size = Some(s);
            }
        }
    }

    if let Some(oid) = found_oid {
        return Ok(Some(LfsPointer {
            oid,
            size: found_size,
        }));
    }
    Ok(None)
}

/// Copies all non-pointer files from `repo_path` into `target_path`.
/// This mirrors the previous Phase 1 logic but is exposed so callers
/// (for example the UI) can invoke it independently.
pub fn copy_non_pointer_files(repo_path: &Path, target_path: &Path) -> Result<()> {
    for entry in WalkDir::new(repo_path).into_iter().filter_map(|e| e.ok()) {
        if entry
            .path()
            .components()
            .any(|c| c.as_os_str() == OsStr::new(".git"))
        {
            continue;
        }
        if let Some(name) = entry.path().file_name().and_then(|s| s.to_str()) {
            if name == ".gitattributes" || name == ".gitignore" || name.starts_with(".git") {
                continue;
            }
        }
        if !entry.file_type().is_file() {
            continue;
        }
        let repo_file_path = entry.path();
        // If this is an LFS pointer, skip in phase 1.  If pointer parsing
        // fails for any reason treat the file as a normal file and continue
        // copying it; don't abort the entire sync for a single malformed
        // file.
        match parse_lfs_pointer_file(repo_file_path) {
            Ok(Some(_)) => continue,
            Ok(None) => {}
            Err(e) => {
                log::warn!(
                    "Failed to parse potential LFS pointer {}: {}. Treating as regular file.",
                    repo_file_path.display(),
                    e
                );
            }
        }
        let rel_path = repo_file_path
            .strip_prefix(repo_path)
            .unwrap_or(repo_file_path);
        let target_file_path = target_path.join(rel_path);

        let should_copy = if target_file_path.exists() {
            let src_meta = fs::metadata(repo_file_path)?;
            let dst_meta = fs::metadata(&target_file_path)?;
            if src_meta.len() != dst_meta.len() {
                true
            } else {
                let src_sha = compute_sha256(repo_file_path)?;
                let dst_sha = compute_sha256(&target_file_path)?;
                src_sha != dst_sha
            }
        } else {
            true
        };
        if should_copy {
            if let Some(parent) = target_file_path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(repo_file_path, &target_file_path).with_context(|| {
                format!(
                    "Failed to copy file from {} to {}",
                    repo_file_path.display(),
                    target_file_path.display()
                )
            })?;
        }
    }
    Ok(())
}

/// Computes the SHA‑256 of the file at the given path and returns it as a
/// lowercase hexadecimal string.
pub fn compute_sha256(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path)
        .with_context(|| format!("Failed to open file {} for hashing", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 8192];
    loop {
        let read_bytes = file.read(&mut buffer)?;
        if read_bytes == 0 {
            break;
        }
        hasher.update(&buffer[..read_bytes]);
    }
    let result = hasher.finalize();
    Ok(hex::encode(result))
}

/// Downloads an LFS object given its SHA‑256 into the specified destination.
/// The default implementation is a stub – in a real application you could
/// call `git lfs fetch` or use the GitHub/GitLab LFS API.  This function
/// creates the destination directory if it does not exist.
fn download_lfs_object(
    sha: &str,
    dest: &Path,
    repo_remote: Option<&str>,
    size: Option<u64>,
) -> Result<()> {
    // Helper to remove userinfo from URLs (e.g., user@host) which can
    // produce invalid Host headers for some servers.
    fn strip_userinfo(u: &str) -> String {
        if let Some(scheme_pos) = u.find("://") {
            let after_scheme = &u[scheme_pos + 3..];
            if let Some(at_pos) = after_scheme.find('@') {
                let scheme = &u[..scheme_pos];
                let after_at = &after_scheme[at_pos + 1..];
                return format!("{}://{}", scheme, after_at);
            }
        }
        u.to_string()
    }

    // This implementation only supports downloading LFS objects via
    // the Git LFS batch API exposed by Azure DevOps / VisualStudio style
    // remotes. For any other remote (or when repo_remote is None) we return
    // an error rather than creating a placeholder file.

    if let Some(remote) = repo_remote {
        let sanitized = strip_userinfo(remote);
        let lower = sanitized.to_ascii_lowercase();
        if lower.contains("dev.azure.com")
            || lower.contains("visualstudio.com")
            || lower.contains("azure")
            || lower.contains("visualstudio")
        {
            // Construct batch endpoint from sanitized repo remote base.
            let batch = format!("{}/info/lfs/objects/batch", sanitized.trim_end_matches('/'));

            // Prepare batch request
            #[derive(serde::Serialize)]
            struct BatchObj<'a> {
                oid: &'a str,
                size: u64,
            }
            #[derive(serde::Serialize)]
            struct BatchReq<'a> {
                operation: &'a str,
                objects: Vec<BatchObj<'a>>,
            }

            let size_val = size.unwrap_or(0);
            let req_body = BatchReq {
                operation: "download",
                objects: vec![BatchObj {
                    oid: sha,
                    size: size_val,
                }],
            };

            let client = reqwest::blocking::Client::new();
            let mut req = client
                .post(&batch)
                .header("Accept", "application/vnd.git-lfs+json")
                .header("Content-Type", "application/vnd.git-lfs+json")
                .body(serde_json::to_vec(&req_body)?);

            // Use basic auth with an empty username and PAT as password if
            // PAT is present.
            if let Ok(pat) = std::env::var("AZURE_DEVOPS_PAT") {
                req = req.basic_auth("", Some(pat));
            }

            let resp = req
                .send()
                .with_context(|| format!("Failed to POST LFS batch to {}", batch))?;
            // Capture status before consuming the response body.
            let status = resp.status();
            let resp_bytes = resp
                .bytes()
                .with_context(|| "Failed to read LFS batch response body")?;
            if !status.is_success() {
                let txt = String::from_utf8_lossy(&resp_bytes).to_string();
                return Err(anyhow::anyhow!(
                    "LFS batch request failed: HTTP {}: {}",
                    status,
                    txt
                ));
            }
            let v: serde_json::Value = serde_json::from_slice(&resp_bytes)
                .with_context(|| "Failed to parse LFS batch response JSON")?;
            // Extract download href from response
            if let Some(obj) = v.get("objects").and_then(|o| o.get(0)) {
                if let Some(actions) = obj.get("actions") {
                    if let Some(download) = actions.get("download") {
                        if let Some(href) = download.get("href").and_then(|h| h.as_str()) {
                            // Optional headers
                            let sanitized_href = strip_userinfo(href);
                            let mut get_req = client.get(&sanitized_href);
                            let mut auth_header_present = false;
                            if let Some(hdrs) = download.get("header").and_then(|h| h.as_object()) {
                                for (k, v) in hdrs.iter() {
                                    let is_safe =
                                        k.chars().all(|c| c.is_ascii_alphanumeric() || c == '-');
                                    if !is_safe {
                                        log::warn!(
                                            "Skipping unsafe header name from LFS action: {}",
                                            k
                                        );
                                        continue;
                                    }
                                    if let Some(val) = v.as_str() {
                                        if k.eq_ignore_ascii_case("authorization") {
                                            auth_header_present = true;
                                        }
                                        get_req = get_req.header(k, val);
                                    } else {
                                        log::warn!("Skipping non-string header value for {}", k);
                                    }
                                }
                            }

                            // If the download action did not provide an Authorization
                            // header, attach the AZURE_DEVOPS_PAT as basic auth.
                            if !auth_header_present {
                                if let Ok(pat) = std::env::var("AZURE_DEVOPS_PAT") {
                                    get_req = get_req.basic_auth("", Some(pat));
                                }
                            }

                            let get_resp = get_req.send().with_context(|| {
                                format!("Failed to GET LFS object from {}", href)
                            })?;
                            let get_status = get_resp.status();
                            if !get_status.is_success() {
                                let txt = get_resp
                                    .text()
                                    .unwrap_or_else(|_| "<failed to read body>".to_string());
                                return Err(anyhow::anyhow!(
                                    "Failed to download LFS object {}: HTTP {}: {}",
                                    sha,
                                    get_status,
                                    txt
                                ));
                            }
                            let bytes = get_resp.bytes().with_context(|| {
                                format!("Failed to read response body from {}", href)
                            })?;

                            // Ensure the parent directory exists so that we can write into it.
                            if let Some(parent) = dest.parent() {
                                fs::create_dir_all(parent).with_context(|| {
                                    format!("Failed to create directory {}", parent.display())
                                })?;
                            }

                            fs::write(dest, &bytes).with_context(|| {
                                format!(
                                    "Failed to write downloaded LFS object to {}",
                                    dest.display()
                                )
                            })?;
                            return Ok(());
                        }
                    }
                }
            }
            return Err(anyhow::anyhow!(
                "LFS batch response did not include download action"
            ));
        }
    }

    // If we reach here the remote is not supported by this downloader.
    Err(anyhow::anyhow!(
        "Unsupported or missing remote for LFS download: {:?}",
        repo_remote
    ))
}

/// Synchronises the contents of the repository at `repo_path` with the
/// directory at `target_path`.  For each pointer file in the repository
/// this function compares the SHA recorded in the pointer against the
/// contents of the corresponding file in the target.  If the file is
/// missing or the hash differs, `download_lfs_object` is invoked to
/// populate the destination.  Non‑pointer files are copied directly to the
/// target if they differ.
pub fn sync_modpack(repo_path: &Path, target_path: &Path) -> Result<()> {
    let meta_path = repo_path.join("metadata.json");
    let _ = meta_path; // metadata.json is intentionally ignored; LFS is only served via Azure-style remotes
                       // Try to discover the repository's origin remote URL so we can use
                       // provider-specific LFS endpoints (for example Azure DevOps) when
                       // available.
                       // repo_remote is not required here; `collect_download_items` will
                       // determine the remote for each pointer and downloads are performed
                       // using the per-item repo_remote value.

    // Phase 1: copy all non-pointer files into the target.
    copy_non_pointer_files(repo_path, target_path)?;

    // Phase 2: collect and download LFS objects for pointer files.
    let items = collect_download_items(repo_path, target_path)?;
    for item in items {
        download_lfs_object(
            &item.oid,
            &item.dest,
            item.repo_remote.as_deref(),
            item.size,
        )?;
    }

    Ok(())
}

/// Scans the repository and returns the list of LFS objects that need to
/// be downloaded (oid, optional size, destination path and repo remote).
pub fn collect_download_items(
    repo_path: &Path,
    target_path: &Path,
) -> Result<Vec<crate::downloader::LfsDownloadItem>> {
    let repo_remote: Option<String> = (|| {
        if let Ok(repo) = git2_crate::Repository::open(repo_path) {
            if let Ok(remote) = repo.find_remote("origin") {
                if let Some(url) = remote.url() {
                    return Some(url.to_string());
                }
            }
        }
        None
    })();

    let mut items = Vec::new();
    for entry in WalkDir::new(repo_path).into_iter().filter_map(|e| e.ok()) {
        if entry
            .path()
            .components()
            .any(|c| c.as_os_str() == OsStr::new(".git"))
        {
            continue;
        }
        if let Some(name) = entry.path().file_name().and_then(|s| s.to_str()) {
            if name == ".gitattributes" || name == ".gitignore" || name.starts_with(".git") {
                continue;
            }
        }
        if !entry.file_type().is_file() {
            continue;
        }
        let repo_file_path = entry.path();
        let rel_path = repo_file_path
            .strip_prefix(repo_path)
            .unwrap_or(repo_file_path);
        let target_file_path = target_path.join(rel_path);

        match parse_lfs_pointer_file(repo_file_path) {
            Ok(Some(pointer)) => {
                // This is an LFS pointer.  Compare with existing file.
                let needs_download = if target_file_path.exists() {
                    let existing_sha = compute_sha256(&target_file_path)?;
                    existing_sha != pointer.oid
                } else {
                    true
                };
                if needs_download {
                    items.push(crate::downloader::LfsDownloadItem {
                        oid: pointer.oid,
                        size: pointer.size,
                        dest: target_file_path,
                        repo_remote: repo_remote.clone(),
                    });
                }
            }
            Ok(None) => {
                // Not a pointer; nothing to collect.
            }
            Err(e) => {
                log::warn!(
                    "Failed to parse potential LFS pointer {}: {}. Skipping download collection for this file.",
                    repo_file_path.display(),
                    e
                );
            }
        }
    }
    Ok(items)
}

/// Validates the local mod directory against the repository.  Returns a
/// vector of relative paths that are missing or have mismatching hashes.
pub fn validate_modpack(repo_path: &Path, target_path: &Path) -> Result<Vec<PathBuf>> {
    let mut mismatches = Vec::new();
    for entry in WalkDir::new(repo_path).into_iter().filter_map(|e| e.ok()) {
        // Ignore .git internals and git metadata files when validating the
        // repo against the target; these are not part of the modpack
        // contents.
        if entry
            .path()
            .components()
            .any(|c| c.as_os_str() == OsStr::new(".git"))
        {
            continue;
        }
        if let Some(name) = entry.path().file_name().and_then(|s| s.to_str()) {
            if name == ".gitattributes" || name == ".gitignore" || name.starts_with(".git") {
                continue;
            }
        }
        if !entry.file_type().is_file() {
            continue;
        }
        let repo_file_path = entry.path();
        let rel_path = repo_file_path
            .strip_prefix(repo_path)
            .unwrap_or(repo_file_path);
        let target_file_path = target_path.join(rel_path);

        if let Some(pointer) = parse_lfs_pointer_file(repo_file_path)? {
            // Validate pointer file.
            let invalid = if target_file_path.exists() {
                let existing_sha = compute_sha256(&target_file_path)?;
                existing_sha != pointer.oid
            } else {
                true
            };
            if invalid {
                mismatches.push(rel_path.to_path_buf());
            }
        } else {
            // Validate normal file by comparing hashes.
            let invalid = if target_file_path.exists() {
                let src_sha = compute_sha256(repo_file_path)?;
                let dst_sha = compute_sha256(&target_file_path)?;
                src_sha != dst_sha
            } else {
                true
            };
            if invalid {
                mismatches.push(rel_path.to_path_buf());
            }
        }
    }
    Ok(mismatches)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use std::thread;
    use tempfile::TempDir;
    use tiny_http::{Response, Server};

    #[test]
    fn download_lfs_object_http_roundtrip() {
        // Read the fixture data that the test server will serve.
        let fixture_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join("test_blob.bin");
        let fixture_data = fs::read(&fixture_path).expect("read fixture");

        // Compute the SHA for the fixture using the library helper. We need
        // a temporary file to use compute_sha256 which operates on paths.
        let tmp_blob = TempDir::new().expect("tmp for blob");
        let tmp_blob_path = tmp_blob.path().join("blob.bin");
        fs::write(&tmp_blob_path, &fixture_data).expect("write tmp blob");
        let fixture_sha = compute_sha256(&tmp_blob_path).expect("compute sha");

        // Start a tiny HTTP server bound to an ephemeral port and serve the
        // batch API and blob endpoints used by Azure-style remotes.
        let server = Server::http("127.0.0.1:0").expect("failed to start test server");
        let server_addr = server.server_addr();
        let server_url = format!("http://{}", server_addr);
        let fixture_for_thread = fixture_data.clone();
        let fixture_sha_clone = fixture_sha.clone();
        thread::spawn(move || {
            for request in server.incoming_requests() {
                let url = request.url().to_string();
                let method = request.method().as_str().to_string();
                if method == "POST" && url.ends_with("/info/lfs/objects/batch") {
                    let download_url = format!("{}/download/{}", server_url, fixture_sha_clone);
                    let body = serde_json::json!({
                        "objects": [
                            {
                                "oid": fixture_sha_clone,
                                "size": fixture_for_thread.len(),
                                "actions": {
                                    "download": {
                                        "href": download_url,
                                        "header": {
                                            "Accept": "application/octet-stream"
                                        }
                                    }
                                }
                            }
                        ]
                    });
                    let header = tiny_http::Header::from_bytes(
                        b"Content-Type",
                        b"application/vnd.git-lfs+json",
                    )
                    .unwrap();
                    let resp = Response::from_string(body.to_string()).with_header(header);
                    let _ = request.respond(resp);
                } else if method == "GET" && url.starts_with("/download/") {
                    let res = Response::from_data(fixture_for_thread.clone());
                    let _ = request.respond(res);
                } else {
                    let resp = Response::from_string("not found").with_status_code(404);
                    let _ = request.respond(resp);
                }
            }
        });

        // Destination file for the download.
        let dest_dir = TempDir::new().expect("dest tempdir");
        let dest_path = dest_dir.path().join("downloaded.bin");

        // Construct an Azure-style repo remote URL that points at our test
        // server so the downloader will attempt a batch API call.
        let repo_remote = format!("http://{}/visualstudio.com/my/repo", server_addr);

        // Perform the download using the private function under test. Provide
        // the simulated repo_remote and pointer size.
        download_lfs_object(
            &fixture_sha,
            &dest_path,
            Some(&repo_remote),
            Some(fixture_data.len() as u64),
        )
        .expect("download_lfs_object failed");

        // Verify the downloaded file's SHA matches the fixture SHA.
        let downloaded_sha = compute_sha256(&dest_path).expect("compute downloaded sha");
        assert_eq!(downloaded_sha, fixture_sha, "downloaded blob sha mismatch");

        // no-op
    }
}
