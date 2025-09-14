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
