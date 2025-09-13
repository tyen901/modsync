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
use sha2::{Digest, Sha256};
use std::fs;
use std::ffi::OsStr;
use std::io::Read;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;
use git2 as git2_crate;

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
    // contains non-UTF-8 content (which would cause `lines()` to fail).
    let data = fs::read(path)
        .with_context(|| format!("Failed to open potential pointer file {}", path.display()))?;

    // Ensure this file contains the version prefix somewhere.  Git or other
    // tools may insert leading bytes (BOMs, CRLF conversions) so search for
    // the marker anywhere in the file instead of insisting on it at byte 0.
    let version_prefix = b"version https://git-lfs.github.com/spec/";
    if data.windows(version_prefix.len()).position(|w| w == version_prefix).is_none() {
        return Ok(None);
    }

    // Search for the oid sha256 line anywhere in the file.
    let needle = b"oid sha256:";
    let mut found_oid: Option<String> = None;
    if let Some(pos) = data.windows(needle.len()).position(|w| w == needle) {
        let hex_start = pos + needle.len();
        // collect hex bytes until whitespace or newline
        let mut hex_end = hex_start;
        while hex_end < data.len() {
            let c = data[hex_end];
            if c.is_ascii_whitespace() {
                break;
            }
            hex_end += 1;
        }
        let hex_bytes = &data[hex_start..hex_end];
        // Convert using lossy UTF-8 in case of weird bytes; hex should be ASCII.
        let hex_str = String::from_utf8_lossy(hex_bytes).trim().to_string();
        found_oid = Some(hex_str);
    }

    // Try to parse size line if present.
    let size_needle = b"size ";
    let mut found_size: Option<u64> = None;
    if let Some(pos) = data.windows(size_needle.len()).position(|w| w == size_needle) {
        let num_start = pos + size_needle.len();
        let mut num_end = num_start;
        while num_end < data.len() {
            let c = data[num_end];
            if c == b'\n' || c == b'\r' || c.is_ascii_whitespace() {
                break;
            }
            num_end += 1;
        }
        if num_end > num_start {
            if let Ok(s) = String::from_utf8_lossy(&data[num_start..num_end]).trim().parse::<u64>() {
                found_size = Some(s);
            }
        }
    }

    if let Some(oid) = found_oid {
        return Ok(Some(LfsPointer { oid, size: found_size }));
    }
    Ok(None)
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
fn download_lfs_object(sha: &str, dest: &Path, lfs_server: Option<&str>, repo_remote: Option<&str>, size: Option<u64>) -> Result<()> {
    // Ensure the parent directory exists so that we can write into it.
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory {}", parent.display()))?;
    }

    // If an LFS server URL is provided via environment or via the
    // provider metadata, attempt to fetch the object from there. The
    // test harness can set `LFS_SERVER_URL` to point to a local HTTP
    // server which serves blobs named by SHA. Prefer an explicitly
    // provided LFS server (for example the provider may include one in
    // metadata.json).  Fall back to environment when not provided.
    let server_owned: Option<String> = lfs_server
        .map(|s| s.to_string())
        .or_else(|| std::env::var("LFS_SERVER_URL").ok());

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

    if let Some(server) = server_owned.as_deref() {
        let url = format!("{}/{}", strip_userinfo(server).trim_end_matches('/'), sha);
        let resp = reqwest::blocking::get(&url).with_context(|| format!("Failed to GET {}", url))?;
        if !resp.status().is_success() {
            return Err(anyhow::anyhow!("Failed to download LFS object {}: HTTP {}", sha, resp.status()));
        }
        let bytes = resp.bytes().with_context(|| format!("Failed to read response body from {}", url))?;
        fs::write(dest, &bytes).with_context(|| format!("Failed to write downloaded LFS object to {}", dest.display()))?;
        return Ok(());
    }

    // If the remote looks like Azure DevOps, use the Git LFS batch API to
    // request a download action (this avoids relying on system git lfs).
    if let Some(remote) = repo_remote {
        let sanitized = strip_userinfo(remote);
        let lower = sanitized.to_ascii_lowercase();
        if lower.contains("dev.azure.com") || lower.contains("visualstudio.com") {
            // Construct batch endpoint from sanitized repo remote base.
            // For remotes like https://dev.azure.com/ORG/PROJECT/_git/REPO
            // the batch endpoint is at
            // https://dev.azure.com/ORG/PROJECT/_git/REPO/info/lfs/objects/batch
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
            let req_body = BatchReq { operation: "download", objects: vec![BatchObj { oid: sha, size: size_val }] };

            let client = reqwest::blocking::Client::new();
            let mut req = client.post(&batch)
                .header("Accept", "application/vnd.git-lfs+json")
                .header("Content-Type", "application/vnd.git-lfs+json")
                .body(serde_json::to_vec(&req_body)?);

            // Use basic auth with an empty username and PAT as password if
            // PAT is present.
            if let Ok(pat) = std::env::var("AZURE_DEVOPS_PAT") {
                req = req.basic_auth("", Some(pat));
            }

            let resp = req.send().with_context(|| format!("Failed to POST LFS batch to {}", batch))?;
            // Capture status before consuming the response body.
            let status = resp.status();
            let resp_bytes = resp.bytes().with_context(|| "Failed to read LFS batch response body")?;
            if !status.is_success() {
                let txt = String::from_utf8_lossy(&resp_bytes).to_string();
                return Err(anyhow::anyhow!(
                    "LFS batch request failed: HTTP {}: {}",
                    status,
                    txt
                ));
            }
            let v: serde_json::Value = serde_json::from_slice(&resp_bytes).with_context(|| "Failed to parse LFS batch response JSON")?;
            // Extract download href from response
            if let Some(obj) = v.get("objects").and_then(|o| o.get(0)) {
                if let Some(actions) = obj.get("actions") {
                    if let Some(download) = actions.get("download") {
                        if let Some(href) = download.get("href").and_then(|h| h.as_str()) {
                            // Optional headers
                            // Sanitize href to remove any userinfo (user@) before
                            // issuing the GET request — some servers reject
                            // requests where the Host header contains userinfo.
                            let sanitized_href = strip_userinfo(href);
                            let mut get_req = client.get(&sanitized_href);
                            let mut auth_header_present = false;
                            if let Some(hdrs) = download.get("header").and_then(|h| h.as_object()) {
                                for (k, v) in hdrs.iter() {
                                    // Only accept simple ASCII header names consisting
                                    // of letters, digits and hyphen. Reject other
                                    // names to avoid "Invalid Header" responses
                                    // from some servers.
                                    let is_safe = k.chars().all(|c| c.is_ascii_alphanumeric() || c == '-');
                                    if !is_safe {
                                        log::warn!("Skipping unsafe header name from LFS action: {}", k);
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
                            // Debug: show what we're about to request.
                            log::debug!("LFS download href: {}", href);
                            log::debug!("Sanitized LFS download href: {}", sanitized_href);
                            // Show host for clarity
                            if let Ok(url) = reqwest::Url::parse(&sanitized_href) {
                                if let Some(host) = url.host_str() {
                                    log::debug!("LFS download host: {}", host);
                                }
                            }
                            log::debug!("Headers to send:");
                            // We can't inspect get_req headers directly; instead
                            // echo the header map we parsed from the batch action.
                            if let Some(hdrs) = download.get("header").and_then(|h| h.as_object()) {
                                for (k, v) in hdrs.iter() {
                                    if let Some(val) = v.as_str() {
                                        log::debug!("  {}: {}", k, &val[..std::cmp::min(80, val.len())]);
                                    } else {
                                        log::debug!("  {}: <non-string>", k);
                                    }
                                }
                            } else {
                                log::debug!("  <no headers from action>");
                            }
                            // If the download action did not provide an Authorization
                            // header, attach the AZURE_DEVOPS_PAT as basic auth.
                            if !auth_header_present {
                                if let Ok(pat) = std::env::var("AZURE_DEVOPS_PAT") {
                                    log::debug!("Attaching AZURE_DEVOPS_PAT as basic auth for GET");
                                    get_req = get_req.basic_auth("", Some(pat));
                                } else {
                                    log::debug!("No AZURE_DEVOPS_PAT present to attach");
                                }
                            } else {
                                log::debug!("Authorization header provided by action; not attaching PAT");
                            }

                            let get_resp = get_req.send().with_context(|| format!("Failed to GET LFS object from {}", href))?;
                            let get_status = get_resp.status();
                            if !get_status.is_success() {
                                // Try to capture body text for diagnostics.
                                let txt = get_resp.text().unwrap_or_else(|_| "<failed to read body>".to_string());
                                return Err(anyhow::anyhow!("Failed to download LFS object {}: HTTP {}: {}", sha, get_status, txt));
                            }
                            let bytes = get_resp.bytes().with_context(|| format!("Failed to read response body from {}", href))?;
                            fs::write(dest, &bytes).with_context(|| format!("Failed to write downloaded LFS object to {}", dest.display()))?;
                            return Ok(());
                        }
                    }
                }
            }
            return Err(anyhow::anyhow!("LFS batch response did not include download action"));
        }
    }

    // Fallback stub: create an empty placeholder file.
    fs::write(dest, b"").with_context(|| {
        format!(
            "Failed to create placeholder for LFS object {}",
            dest.display()
        )
    })?;
    Ok(())
}

/// Synchronises the contents of the repository at `repo_path` with the
/// directory at `target_path`.  For each pointer file in the repository
/// this function compares the SHA recorded in the pointer against the
/// contents of the corresponding file in the target.  If the file is
/// missing or the hash differs, `download_lfs_object` is invoked to
/// populate the destination.  Non‑pointer files are copied directly to the
/// target if they differ.
pub fn sync_modpack(repo_path: &Path, target_path: &Path) -> Result<()> {
    // Attempt to read metadata.json in the repository root to discover a
    // provider-supplied LFS server URL.  If present the server will be
    // preferred over the environment variable.
    let meta_path = repo_path.join("metadata.json");
    let provider_lfs: Option<String> = if meta_path.exists() {
        match fs::read_to_string(&meta_path) {
            Ok(c) => match serde_json::from_str::<serde_json::Value>(&c) {
                Ok(v) => v.get("lfs_server_url").and_then(|j| j.as_str().map(|s| s.to_string())),
                Err(_) => None,
            },
            Err(_) => None,
        }
    } else {
        None
    };
    // Try to discover the repository's origin remote URL so we can use
    // provider-specific LFS endpoints (for example Azure DevOps) when
    // available.
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

    for entry in WalkDir::new(repo_path).into_iter().filter_map(|e| e.ok()) {
        // Skip directories and any files under a .git directory to avoid
        // copying repository internals into the target.  Also skip common
        // top-level git metadata files such as .gitattributes and
        // .gitignore — these are not needed in the downloaded modpack.
        if entry.path().components().any(|c| c.as_os_str() == OsStr::new(".git")) {
            continue;
        }
        // Skip top-level git metadata files (and any file starting with
        // ".git" to be conservative).
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
            // This is an LFS pointer.  Compare with existing file.
            let needs_download = if target_file_path.exists() {
                let existing_sha = compute_sha256(&target_file_path)?;
                existing_sha != pointer.oid
            } else {
                true
            };
            if needs_download {
                let server_opt = provider_lfs.as_deref();
                download_lfs_object(&pointer.oid, &target_file_path, server_opt, repo_remote.as_deref(), pointer.size)?;
            }
        } else {
            // Not a pointer: copy file directly if it differs.
            let should_copy = if target_file_path.exists() {
                // Compare bytes only if sizes match; otherwise copy.
                let src_meta = fs::metadata(repo_file_path)?;
                let dst_meta = fs::metadata(&target_file_path)?;
                if src_meta.len() != dst_meta.len() {
                    true
                } else {
                    // Compute hashes for small files.  If files are large this
                    // may be expensive; consider using modification times.
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
    }
    Ok(())
}

/// Validates the local mod directory against the repository.  Returns a
/// vector of relative paths that are missing or have mismatching hashes.
pub fn validate_modpack(repo_path: &Path, target_path: &Path) -> Result<Vec<PathBuf>> {
    let mut mismatches = Vec::new();
    for entry in WalkDir::new(repo_path).into_iter().filter_map(|e| e.ok()) {
        // Ignore .git internals and git metadata files when validating the
        // repo against the target; these are not part of the modpack
        // contents.
        if entry.path().components().any(|c| c.as_os_str() == OsStr::new(".git")) {
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
    use tiny_http::{Server, Response};

    #[test]
    fn download_lfs_object_http_roundtrip() {
        // Read the fixture data that the test server will serve.
        let fixture_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("fixtures").join("test_blob.bin");
        let fixture_data = fs::read(&fixture_path).expect("read fixture");

        // Compute the SHA for the fixture using the library helper. We need
        // a temporary file to use compute_sha256 which operates on paths.
        let tmp_blob = TempDir::new().expect("tmp for blob");
        let tmp_blob_path = tmp_blob.path().join("blob.bin");
        fs::write(&tmp_blob_path, &fixture_data).expect("write tmp blob");
        let fixture_sha = compute_sha256(&tmp_blob_path).expect("compute sha");

        // Start a tiny HTTP server bound to an ephemeral port and serve the
        // fixture bytes for any incoming request.
        let server = Server::http("127.0.0.1:0").expect("failed to start test server");
        let server_addr = server.server_addr();
        let server_url = format!("http://{}", server_addr);
        let fixture_for_thread = fixture_data.clone();
        thread::spawn(move || {
            for request in server.incoming_requests() {
                let res = Response::from_data(fixture_for_thread.clone());
                let _ = request.respond(res);
            }
        });


    // Point the downloader at our test server.
    std::env::set_var("LFS_SERVER_URL", &server_url);

        // Destination file for the download.
        let dest_dir = TempDir::new().expect("dest tempdir");
        let dest_path = dest_dir.path().join("downloaded.bin");

    // Perform the download using the private function under test. Pass
    // the server URL explicitly (download_lfs_object will prefer the
    // provided value over the environment variable when given).
    let server = std::env::var("LFS_SERVER_URL").ok();
    // No repository remote in this unit test; pass None for repo_remote and
    // size.
    download_lfs_object(&fixture_sha, &dest_path, server.as_deref(), None, None).expect("download_lfs_object failed");

        // Verify the downloaded file's SHA matches the fixture SHA.
        let downloaded_sha = compute_sha256(&dest_path).expect("compute downloaded sha");
        assert_eq!(downloaded_sha, fixture_sha, "downloaded blob sha mismatch");

        // Clean up env var.
        std::env::remove_var("LFS_SERVER_URL");
    }
}
