// src/sync/cleaner.rs

use anyhow::Result;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

/// Scans the download directory and returns a list of files
/// that are present locally but not in the expected set.
/// Expected files should be relative to the download_path.
pub fn find_extra_files(
    download_path: &Path,
    expected_files: &HashSet<PathBuf>,
) -> Result<Vec<PathBuf>> {
    println!(
        "Cleaner: Scanning dir '{}' for extra files...",
        download_path.display()
    );
    let mut extra_files = Vec::new();
    let mut local_files = HashSet::new();

    if !download_path.exists() {
        println!("Cleaner: Download path does not exist, nothing to scan.");
        return Ok(extra_files); // No directory, no extra files
    }

    for entry in WalkDir::new(download_path).into_iter().filter_map(|e| e.ok()) {
        let local_path = entry.path();
        // Only consider files, skip directories
        if local_path.is_file() {
            // Get the path relative to the download directory
            if let Ok(relative_path) = local_path.strip_prefix(download_path) {
                let relative_path_buf = relative_path.to_path_buf();
                local_files.insert(relative_path_buf.clone());
                // If this local file is not in the expected set, it's extra
                if !expected_files.contains(&relative_path_buf) {
                    println!(
                        "Cleaner: Found extra file: {}",
                        relative_path.display()
                    );
                    extra_files.push(local_path.to_path_buf()); // Store the full path for deletion
                }
            } else {
                eprintln!(
                    "Cleaner: Warning - could not strip prefix from {}",
                    local_path.display()
                );
            }
        }
    }

    println!(
        "Cleaner: Scan complete. Found {} local files, {} expected files, {} extra files.",
        local_files.len(),
        expected_files.len(),
        extra_files.len()
    );

    Ok(extra_files)
}

// Helper function (potentially moved here from sync::state or utils)
// To parse TorrentDetailsResponse and get expected relative paths
use librqbit::api::TorrentDetailsResponse;

pub fn get_expected_files_from_details(
    details: &TorrentDetailsResponse,
) -> HashSet<PathBuf> {
    let mut expected = HashSet::new();
    if let Some(files) = &details.files {
        let root_dir = PathBuf::from(details.name.as_deref().unwrap_or(""));
        for file_detail in files {
            if file_detail.included {
                // Construct the relative path using components
                let mut current_path = root_dir.clone();
                for component in &file_detail.components {
                    current_path.push(component);
                }
                expected.insert(current_path);
            }
        }
    }
    expected
} 