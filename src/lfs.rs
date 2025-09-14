use anyhow::{Context, Result};
use reqwest;
use serde_json;
use std::fs;
use std::path::Path;

/// Helper to strip userinfo from URLs (e.g., user@host)
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

/// Downloads an LFS object given its SHA‑256 into the specified destination.
/// This implementation supports Azure-style remotes via the Git LFS batch API.
pub fn download_lfs_object(
    sha: &str,
    dest: &Path,
    repo_remote: Option<&str>,
    size: Option<u64>,
) -> Result<()> {
    if let Some(remote) = repo_remote {
        let sanitized = strip_userinfo(remote);
        let lower = sanitized.to_ascii_lowercase();
        if lower.contains("dev.azure.com")
            || lower.contains("visualstudio.com")
            || lower.contains("azure")
            || lower.contains("visualstudio")
        {
            let batch = format!("{}/info/lfs/objects/batch", sanitized.trim_end_matches('/'));

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
                objects: vec![BatchObj { oid: sha, size: size_val }],
            };

            let client = reqwest::blocking::Client::new();
            let mut req = client
                .post(&batch)
                .header("Accept", "application/vnd.git-lfs+json")
                .header("Content-Type", "application/vnd.git-lfs+json")
                .body(serde_json::to_vec(&req_body)?);

            if let Ok(pat) = std::env::var("AZURE_DEVOPS_PAT") {
                req = req.basic_auth("", Some(pat));
            }

            let resp = req
                .send()
                .with_context(|| format!("Failed to POST LFS batch to {}", batch))?;
            let status = resp.status();
            let resp_bytes = resp
                .bytes()
                .with_context(|| "Failed to read LFS batch response body")?;
            if !status.is_success() {
                let txt = String::from_utf8_lossy(&resp_bytes).to_string();
                return Err(anyhow::anyhow!("LFS batch request failed: HTTP {}: {}", status, txt));
            }
            let v: serde_json::Value = serde_json::from_slice(&resp_bytes)
                .with_context(|| "Failed to parse LFS batch response JSON")?;
            if let Some(obj) = v.get("objects").and_then(|o| o.get(0)) {
                if let Some(actions) = obj.get("actions") {
                    if let Some(download) = actions.get("download") {
                        if let Some(href) = download.get("href").and_then(|h| h.as_str()) {
                            let sanitized_href = strip_userinfo(href);
                            let mut get_req = client.get(&sanitized_href);
                            let mut auth_header_present = false;
                            if let Some(hdrs) = download.get("header").and_then(|h| h.as_object()) {
                                for (k, v) in hdrs.iter() {
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

                            if !auth_header_present {
                                if let Ok(pat) = std::env::var("AZURE_DEVOPS_PAT") {
                                    get_req = get_req.basic_auth("", Some(pat));
                                }
                            }

                            let get_resp = get_req.send().with_context(|| format!("Failed to GET LFS object from {}", href))?;
                            let get_status = get_resp.status();
                            if !get_status.is_success() {
                                let txt = get_resp.text().unwrap_or_else(|_| "<failed to read body>".to_string());
                                return Err(anyhow::anyhow!("Failed to download LFS object {}: HTTP {}: {}", sha, get_status, txt));
                            }
                            let bytes = get_resp.bytes().with_context(|| format!("Failed to read response body from {}", href))?;

                            if let Some(parent) = dest.parent() {
                                fs::create_dir_all(parent).with_context(|| format!("Failed to create directory {}", parent.display()))?;
                            }

                            fs::write(dest, &bytes).with_context(|| format!("Failed to write downloaded LFS object to {}", dest.display()))?;
                            return Ok(());
                        }
                    }
                }
            }
            return Err(anyhow::anyhow!("LFS batch response did not include download action"));
        }
    }

    Err(anyhow::anyhow!("Unsupported or missing remote for LFS download: {:?}", repo_remote))
}

/// Blocking helper used by tests to perform an Azure-style LFS batch download
/// using an existing blocking `reqwest::blocking::Client` and write result to disk.
pub fn azure_lfs_batch_download_and_write_blocking(
    client: &reqwest::blocking::Client,
    pat: &str,
    repo_base: &str,
    oid: &str,
    size: u64,
    target_path: &Path,
) -> Result<()> {
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

    let req_body = BatchReq { operation: "download", objects: vec![BatchObj { oid, size }] };
    let batch_url = format!("{}/info/lfs/objects/batch", repo_base.trim_end_matches('/'));
    let batch_resp = client
        .post(&batch_url)
        .basic_auth("", Some(pat.to_string()))
        .header("Accept", "application/vnd.git-lfs+json")
        .header("Content-Type", "application/vnd.git-lfs+json")
        .body(serde_json::to_vec(&req_body)?)
        .send()?;
    let batch_status = batch_resp.status();
    let batch_text = batch_resp.text().unwrap_or_default();
    if !batch_status.is_success() {
        return Err(anyhow::anyhow!("LFS batch failed for {}: HTTP {}: {}", oid, batch_status, batch_text));
    }
    let batch_json: serde_json::Value = serde_json::from_str(&batch_text)?;
    if let Some(obj) = batch_json.get("objects").and_then(|o| o.get(0)) {
        if let Some(actions) = obj.get("actions") {
            if let Some(download) = actions.get("download") {
                if let Some(href) = download.get("href").and_then(|h| h.as_str()) {
                    let sanitized_href = strip_userinfo(href);
                    let get_req = client.get(&sanitized_href).basic_auth("", Some(pat.to_string())).header("Accept", "application/vnd.git-lfs");
                    let get_resp = get_req.send()?;
                    let get_status = get_resp.status();
                    if !get_status.is_success() {
                        let get_text = get_resp.text().unwrap_or_default();
                        return Err(anyhow::anyhow!("LFS GET failed for {}: HTTP {}: {}", oid, get_status, get_text));
                    }
                    let bytes = get_resp.bytes()?;
                    std::fs::write(target_path, &bytes)?;
                    return Ok(());
                }
            }
        }
    }
    Err(anyhow::anyhow!("LFS batch response did not include download action for {}", oid))
}

/// Request item for async LFS downloader: one oid may have multiple paths.
#[derive(Debug, Clone)]
pub struct LfsRequestItem {
    pub oid: String,
    pub size: Option<u64>,
    pub paths: Vec<std::path::PathBuf>,
    pub repo_remote: Option<String>,
}

/// Summary returned by async LFS downloader
#[derive(Debug, Clone)]
pub struct LfsDownloadSummary {
    pub files_done: usize,
    pub bytes_done: u64,
}

/// Async LFS downloader which uses the async `AzureClient` from `http.rs`.
///
/// This performs a single batch request for all provided OIDs, follows
/// the download actions, honours action headers (safely), falls back to
/// `AZURE_DEVOPS_PAT` basic auth when the action doesn't include an
/// Authorization header, validates size and SHA-256 and writes part files
/// into `out_dir/.tmp` before copying to each requested destination path.
pub async fn download_lfs_objects_async(
    client: &crate::http::AzureClient,
    items: Vec<LfsRequestItem>,
    out_dir: &std::path::Path,
    concurrency: usize,
) -> Result<LfsDownloadSummary> {
    use anyhow::Context;
    use hex;
    use sha2::{Digest as _, Sha256};
    use tokio::fs;
    use tokio::io::AsyncWriteExt;

    // group items by oid (merge paths)
    let mut oid_map: std::collections::HashMap<String, Vec<(std::path::PathBuf, Option<u64>)>> =
        std::collections::HashMap::new();
    for it in items.into_iter() {
        for p in it.paths.into_iter() {
            oid_map.entry(it.oid.clone()).or_default().push((p, it.size));
        }
    }

    let tmp_base = out_dir.join(".tmp");
    fs::create_dir_all(&tmp_base)
        .await
        .with_context(|| format!("creating tmp dir {}", tmp_base.display()))?;

    // prepare batch request
    let mut objs: Vec<crate::http::LfsObject> = Vec::new();
    for (oid, items) in oid_map.iter() {
        let size = items.first().and_then(|(_, s)| *s);
        objs.push(crate::http::LfsObject {
            oid: oid.clone(),
            size,
        });
    }
    if objs.is_empty() {
        return Ok(LfsDownloadSummary {
            files_done: 0,
            bytes_done: 0,
        });
    }

    let batch_req = crate::http::LfsBatchRequest {
        operation: "download".to_string(),
        objects: objs,
    };
    let batch_resp = client
        .lfs_batch(batch_req)
        .await
        .context("lfs batch failed")?;

    // For concurrency we spawn per-oid async tasks but limit parallelism with a semaphore.
    use tokio::sync::Semaphore;
    use std::sync::Arc;

    let sem = Arc::new(Semaphore::new(std::cmp::max(1, concurrency)));
    // Clone client into an Arc so tasks can own it
    let arc_client = Arc::new(crate::http::AzureClient {
        base_url: client.base_url.clone(),
        token: client.token.clone(),
        client: client.client.clone(),
    });

    let mut handles: Vec<(String, tokio::task::JoinHandle<Result<(String, u64, std::path::PathBuf), anyhow::Error>>)> = Vec::new();
    for obj in batch_resp.objects.into_iter() {
        let oid = obj.oid;
        let size = obj.size;
        let action = match obj.actions.and_then(|mut m| m.remove("download")) {
            Some(a) => a,
            None => continue,
        };
        let href_opt = action.href;
        if href_opt.is_none() {
            continue;
        }

        let sem_clone = sem.clone();
        let arc_client = arc_client.clone();
        let tmp_base = tmp_base.clone();
        let headers_val = action.header.clone();

        // keep a clone of oid for the joiner; the task will own its own clone
        let oid_for_join = oid.clone();
        let oid_for_task = oid.clone();

        let handle = tokio::spawn(async move {
            // acquire permit
            let _permit = sem_clone.acquire().await;

            let href = href_opt.unwrap();
            let sanitized_href = strip_userinfo(&href);
            let mut req = arc_client.client.get(&sanitized_href);

            let mut auth_header_present = false;
            if let Some(headers_val) = headers_val {
                if let Some(obj) = headers_val.as_object() {
                    for (k, v) in obj.iter() {
                        let is_safe = k.chars().all(|c| c.is_ascii_alphanumeric() || c == '-');
                        if !is_safe {
                            log::warn!("Skipping unsafe header name from LFS action: {}", k);
                            continue;
                        }
                        if let Some(s) = v.as_str() {
                            if k.eq_ignore_ascii_case("authorization") {
                                auth_header_present = true;
                            }
                            req = req.header(k.as_str(), s);
                        } else {
                            log::warn!("Skipping non-string header value for {}", k);
                        }
                    }
                }
            }

            if !auth_header_present {
                if let Ok(pat) = std::env::var("AZURE_DEVOPS_PAT") {
                    req = req.basic_auth("", Some(pat));
                }
            }

            let resp = req.send().await.with_context(|| format!("lfs get failed for {}", href))?;
            let status = resp.status();
            let bytes = resp.bytes().await.context("read body")?;
            if !status.is_success() {
                return Err(anyhow::anyhow!("lfs GET non-success {}: {}", href, status));
            }
            if let Some(expected) = size {
                if bytes.len() as u64 != expected {
                    return Err(anyhow::anyhow!(
                        "lfs size mismatch for oid {}: expected {}, got {}",
                        oid_for_task,
                        expected,
                        bytes.len()
                    ));
                }
            }
            let mut hasher = Sha256::new();
            hasher.update(&bytes);
            let got = hex::encode(<Sha256 as sha2::Digest>::finalize(hasher));
            if got != oid_for_task {
                return Err(anyhow::anyhow!(
                    "lfs oid mismatch for oid {}: expected {}, got {}",
                    oid_for_task,
                    oid_for_task,
                    got
                ));
            }

            let part = tmp_base.join(format!("{}.part", oid_for_task));
            if let Some(parent) = part.parent() {
                fs::create_dir_all(parent).await.ok();
            }
            let mut f = fs::File::create(&part).await.context("create lfs part")?;
            f.write_all(&bytes).await?;
            f.flush().await.ok();

            // return oid, length and part path
            Ok::<(String, u64, std::path::PathBuf), anyhow::Error>((oid_for_task, bytes.len() as u64, part))
        });
        handles.push((oid_for_join, handle));
    }

    let mut files_done: usize = 0;
    let mut bytes_done: u64 = 0;

    // Collect results and copy part files to other paths
    for (oid_key, handle) in handles {
    let (_oid, got_len, part_path) = handle.await.context("join lfs task")??;

    if let Some(paths) = oid_map.get(&oid_key) {
            for (path, _) in paths {
                let dest_path = out_dir.join(path);
                if let Some(parent) = dest_path.parent() {
                    fs::create_dir_all(parent).await.ok();
                }
                fs::copy(&part_path, &dest_path).await.context("copy")?;
                files_done += 1;
                bytes_done = bytes_done.saturating_add(got_len);
            }
        }
        let _ = fs::remove_file(&part_path).await;
    }

    Ok(LfsDownloadSummary { files_done, bytes_done })
}
