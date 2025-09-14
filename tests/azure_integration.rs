use anyhow::Result;
use reqwest::blocking::Client;
use reqwest::StatusCode;
use std::fs;
use std::io::Write;
use std::time::Duration;
use std::collections::HashSet;
use std::path::PathBuf;
use tempfile::TempDir;

// This integration test talks to a real Azure DevOps repository using the
// REST API and the AZURE_DEVOPS_PAT environment variable. It does not use
// `git clone` and instead lists items and fetches file contents via HTTP.
// The test is skipped when the PAT is not present.

// Import shared test helpers
mod common;

#[test]
fn azure_devops_list_download_compare_and_heal() -> Result<()> {
    // Require PAT for real network test. Skip if missing.
    let pat = match std::env::var("AZURE_DEVOPS_PAT") {
        Ok(v) if !v.is_empty() => v,
        _ => {
            eprintln!("AZURE_DEVOPS_PAT not set; skipping Azure DevOps integration test");
            return Ok(());
        }
    };

    // Repository info (from the user prompt)
    let org = "peanutcommunityarma";
    let project = "pca";
    let repo = "xyi";

    let client = Client::builder()
        .timeout(Duration::from_secs(30))
        .build()?;

    // List items via Azure DevOps REST API (recursively). We request the
    // items metadata and then fetch file contents as octet streams per-file.
    let list_url = format!(
        "https://dev.azure.com/{}/{}/_apis/git/repositories/{}/items?recursionLevel=Full&api-version=6.0",
        org, project, repo
    );

    let list_resp = client
        .get(&list_url)
        .basic_auth("", Some(pat.clone()))
        .send()?;
    if list_resp.status() != StatusCode::OK {
        return Err(anyhow::anyhow!(
            "Failed to list repo items: HTTP {}",
            list_resp.status()
        ));
    }
    let list_json: serde_json::Value = list_resp.json()?;
    let values = list_json
        .get("value")
        .and_then(|v| v.as_array())
        .ok_or_else(|| anyhow::anyhow!("unexpected items json"))?;

    // Create temporary directories for the repo (remote files) and the
    // local target that we will 'heal'.
    let repo_tmp = TempDir::new()?;
    let target_tmp = TempDir::new()?;
    let repo_path = repo_tmp.path();
    let target_path = target_tmp.path();

    // Limit number of processed files to keep test time reasonable. This
    // still demonstrates listing, downloading and healing. The limit can
    // be tuned with the AZURE_INTEGRATION_MAX_FILES environment variable.
    let max_files: usize = match std::env::var("AZURE_INTEGRATION_MAX_FILES") {
        Ok(s) => s.parse().unwrap_or(200usize),
        Err(_) => 200usize,
    };
    let mut processed = 0usize;
    let mut processed_paths: HashSet<PathBuf> = HashSet::new();

    for item in values.iter() {
        if processed >= max_files {
            break;
        }
        let is_folder = item
            .get("isFolder")
            .and_then(|b| b.as_bool())
            .unwrap_or(false);
        if is_folder {
            continue;
        }
        let path = item
            .get("path")
            .and_then(|p| p.as_str())
            .unwrap_or("");
        if path.is_empty() {
            continue;
        }

        // Azure paths start with '/'. Skip special files (git internals)
        if path.starts_with("/.git") || path.ends_with(".gitattributes") || path.ends_with(".gitignore") {
            continue;
        }

        // Fetch raw content for this path
        let get_url = reqwest::Url::parse_with_params(
            &format!(
                "https://dev.azure.com/{}/{}/_apis/git/repositories/{}/items",
                org, project, repo
            ),
            &[("path", path), ("api-version", "6.0"), ("$format", "octetStream")],
        )?;

        let resp = client
            .get(get_url)
            .basic_auth("", Some(pat.clone()))
            .send()?;
        if resp.status() != StatusCode::OK {
            eprintln!("skipping {}: HTTP {}", path, resp.status());
            continue;
        }
        let bytes = resp.bytes()?;

        // Write file into repo_path preserving directory structure
        let rel_path = if path.starts_with('/') { &path[1..] } else { path };
        let dest = repo_path.join(rel_path);
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut f = fs::File::create(&dest)?;
        f.write_all(&bytes)?;

        // For the initial target state, copy non-pointer files directly
        // and create a wrong placeholder for pointer files to simulate a
        // need for healing.
        match modsync::modpack::parse_lfs_pointer_file(&dest) {
            Ok(Some(_ptr)) => {
                // pointer file: leave target missing or write invalid blob to
                // simulate corruption — write a small wrong file
                let target_file = target_path.join(rel_path);
                if let Some(parent) = target_file.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::write(&target_file, b"invalid-blob")?;
            }
            Ok(None) => {
                // normal file: copy as-is to target
                let target_file = target_path.join(rel_path);
                if let Some(parent) = target_file.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::copy(&dest, &target_file)?;
            }
            Err(_) => {
                // treat as normal file
                let target_file = target_path.join(rel_path);
                if let Some(parent) = target_file.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::copy(&dest, &target_file)?;
            }
        }

        processed_paths.insert(PathBuf::from(rel_path));
        processed += 1;
    }

    assert!(processed > 0, "No files processed from Azure repo — check permissions");

    // Validate initial mismatches
    let mismatches_before = modsync::modpack::validate_modpack(repo_path, target_path)?;
    assert!(!mismatches_before.is_empty(), "Expected some mismatches before healing");

    // Heal: for each previously-mismatched path, if it's an LFS pointer
    // use the Azure-style LFS batch API to download the object. For
    // non-pointer files copy from the fetched repo tree. This keeps the
    // test self-contained and avoids calling private helpers.
    let repo_base = format!("https://dev.azure.com/{}/{}/_git/{}", org, project, repo);

    for rel in mismatches_before.iter() {
        let repo_file = repo_path.join(rel);
        let target_file = target_path.join(rel);
        if let Some(parent) = target_file.parent() {
            fs::create_dir_all(parent)?;
        }
        if let Some(ptr) = modsync::modpack::parse_lfs_pointer_file(&repo_file)? {
            // Use helper to perform the Azure-style LFS batch download and write
            // the blob to the target file.
            let size_val = ptr.size.unwrap_or(0);
            common::azure_lfs_batch_download_and_write(&client, &pat, &repo_base, &ptr.oid, size_val, &target_file)?;
            continue;
        } else {
            fs::copy(&repo_file, &target_file)?;
        }
    }

    // Validate again — expect no mismatches for the processed subset
    let mismatches_after = modsync::modpack::validate_modpack(repo_path, target_path)?;

    // Check only the subset of previously-mismatched paths that we
    // actually processed. Other mismatches may remain because we limited
    // the number of files fetched earlier.
    for rel in mismatches_before.iter() {
        if processed_paths.contains(rel) {
            assert!(!mismatches_after.contains(rel), "Path {:?} still mismatched after healing", rel);
        }
    }

    Ok(())
}
