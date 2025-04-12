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
        for file_detail in files {
            if file_detail.included {
                // Construct the relative path directly using components
                let mut current_path = PathBuf::new(); // Start with an empty path
                for component in &file_detail.components {
                    current_path.push(component);
                }
                println!("Cleaner: Adding expected relative path: {}", current_path.display());
                expected.insert(current_path);
            }
        }
    }
    expected
}

/// Get expected files from raw torrent content
/// This replaces the previous function that used Torrent struct
pub fn get_expected_files_from_bytes(
    torrent_bytes: &[u8],
) -> Result<HashSet<PathBuf>> {
    use anyhow::Context;
    
    // Use the librqbit API to add the torrent temporarily to get details
    let mut expected = HashSet::new();
    
    // Create a temporary session to parse the torrent
    let session = match tokio::runtime::Handle::try_current() {
        Ok(handle) => {
            // We're in a Tokio runtime already
            handle.block_on(async {
                // Create a temporary in-memory session
                librqbit::Session::new_with_opts(
                    std::env::temp_dir(), // Use temp dir - we won't actually download
                    librqbit::SessionOptions {
                        disable_dht: true,
                        disable_dht_persistence: true,
                        persistence: None,
                        ..Default::default()
                    }
                ).await
            }).context("Failed to create temporary session")?
        },
        Err(_) => {
            // Not in a Tokio runtime, can't parse torrent here
            eprintln!("Cleaner: Cannot parse torrent bytes outside of tokio runtime");
            return Ok(expected); // Return empty set
        }
    };
    
    // Create API
    let api = librqbit::Api::new(session, None);
    
    // Add torrent (we'll forget it right after)
    let add_request = librqbit::AddTorrent::from_bytes(torrent_bytes.to_vec());
    let options = librqbit::AddTorrentOptions { 
        paused: true,  // Don't start downloading
        ..Default::default()
    };
    
    let response = match tokio::runtime::Handle::try_current() {
        Ok(handle) => {
            handle.block_on(async {
                api.api_add_torrent(add_request, Some(options)).await
            }).context("Failed to add torrent to temporary session")?
        },
        Err(_) => return Ok(expected),
    };
    
    // Get the details
    if let Some(id) = response.id {
        let details = match tokio::runtime::Handle::try_current() {
            Ok(handle) => {
                handle.block_on(async {
                    let details = api.api_torrent_details(id.into()).context("Failed to get torrent details")?;
                    
                    // Clean up - forget the torrent right away
                    let _ = api.api_torrent_action_forget(id.into()).await;
                    
                    Ok::<_, anyhow::Error>(details)
                }).context("Failed to get torrent details")?
            },
            Err(_) => return Ok(expected),
        };
        
        // Use our existing function to extract file paths
        expected = get_expected_files_from_details(&details);
    }
    
    Ok(expected)
}

#[cfg(test)]
mod tests {
    use super::*; // Import functions from outer module
    use std::collections::HashSet;
    use std::fs::{self, File};
    use std::io::Write;
    use std::path::PathBuf;
    use tempfile::tempdir;

    // Import necessary structs from librqbit for mocking details
    // Use the crate name directly as it's an external dependency
    use librqbit::{api::{TorrentDetailsResponse, TorrentDetailsResponseFile}, FileDetailsAttrs};

    // Helper to create a dummy TorrentDetailsResponseFile
    fn create_dummy_file_detail(
        components: Vec<&str>,
        length: u64,
        included: bool,
    ) -> TorrentDetailsResponseFile {
        TorrentDetailsResponseFile {
            name: components.last().cloned().unwrap_or("").to_string(), // Simple name extraction
            components: components.into_iter().map(String::from).collect(),
            length,
            included,
            attributes: FileDetailsAttrs::default(), // Default attributes are fine for this test
        }
    }

    #[test]
    fn test_get_expected_files_simple() {
        let details = TorrentDetailsResponse {
            id: Some(1),
            info_hash: "dummy_hash".to_string(),
            name: Some("test_torrent_different_name".to_string()),
            output_folder: "/downloads".to_string(),
            files: Some(vec![
                create_dummy_file_detail(vec!["file1.txt"], 100, true),
                create_dummy_file_detail(vec!["subdir", "file2.dat"], 200, true),
            ]),
            stats: None,
        };

        let expected = get_expected_files_from_details(&details);
        let mut expected_set = HashSet::new();
        expected_set.insert(PathBuf::from("file1.txt"));
        expected_set.insert(PathBuf::from("subdir/file2.dat"));

        assert_eq!(expected, expected_set);
    }

    #[test]
    fn test_get_expected_files_excluded() {
        let details = TorrentDetailsResponse {
            id: Some(1),
            info_hash: "dummy_hash".to_string(),
            name: Some("test_torrent".to_string()),
            output_folder: "/downloads".to_string(),
            files: Some(vec![
                create_dummy_file_detail(vec!["file1.txt"], 100, true),
                create_dummy_file_detail(vec!["file_excluded.dat"], 200, false), // Excluded
                create_dummy_file_detail(vec!["subdir", "file3.log"], 300, true),
            ]),
            stats: None,
        };

        let expected = get_expected_files_from_details(&details);
        let mut expected_set = HashSet::new();
        expected_set.insert(PathBuf::from("file1.txt"));
        expected_set.insert(PathBuf::from("subdir/file3.log"));

        assert_eq!(expected.len(), 2);
        assert_eq!(expected, expected_set);
    }
    
    #[test]
    fn test_get_expected_files_no_name() {
        let details = TorrentDetailsResponse {
            id: Some(1),
            info_hash: "dummy_hash".to_string(),
            name: None,
            output_folder: "/downloads".to_string(),
            files: Some(vec![
                create_dummy_file_detail(vec!["file1.txt"], 100, true),
            ]),
            stats: None,
        };

        let expected = get_expected_files_from_details(&details);
        let mut expected_set = HashSet::new();
        expected_set.insert(PathBuf::from("file1.txt"));
        assert_eq!(expected, expected_set);
    }

    // --- Tests for find_extra_files --- 

    // Helper to setup a test directory with specified files
    fn setup_test_dir(files_to_create: &[&str]) -> Result<tempfile::TempDir, std::io::Error> {
        let dir = tempdir()?;
        for relative_path in files_to_create {
            let full_path = dir.path().join(relative_path);
            if let Some(parent) = full_path.parent() {
                fs::create_dir_all(parent)?;
            }
            let mut file = File::create(full_path)?;
            writeln!(file, "dummy content for {}", relative_path)?;
        }
        Ok(dir)
    }

    #[test]
    fn test_find_extra_files_none_extra() -> Result<()> {
        let expected_files_rel: HashSet<PathBuf> = [
            PathBuf::from("file1.txt"),
            PathBuf::from("subdir/file2.dat"),
        ]
        .iter()
        .cloned()
        .collect();

        let dir = setup_test_dir(&["file1.txt", "subdir/file2.dat"])?;
        let extra = find_extra_files(dir.path(), &expected_files_rel)?;
        assert!(extra.is_empty());
        Ok(())
    }

    #[test]
    fn test_find_extra_files_one_extra_root() -> Result<()> {
        let expected_files_rel: HashSet<PathBuf> = [
            PathBuf::from("file1.txt"),
        ]
        .iter()
        .cloned()
        .collect();

        let dir = setup_test_dir(&["file1.txt", "extra_file.log"])?;
        let extra = find_extra_files(dir.path(), &expected_files_rel)?;
        
        assert_eq!(extra.len(), 1);
        assert!(extra[0].ends_with("extra_file.log"));
        assert_eq!(extra[0], dir.path().join("extra_file.log"));
        Ok(())
    }

    #[test]
    fn test_find_extra_files_one_extra_subdir() -> Result<()> {
         let expected_files_rel: HashSet<PathBuf> = [
            PathBuf::from("file1.txt"),
        ]
        .iter()
        .cloned()
        .collect();

        let dir = setup_test_dir(&["file1.txt", "subdir/extra.tmp"])?;
        let extra = find_extra_files(dir.path(), &expected_files_rel)?;
        
        assert_eq!(extra.len(), 1);
        assert!(extra[0].ends_with("subdir/extra.tmp"));
        assert_eq!(extra[0], dir.path().join("subdir/extra.tmp"));
        Ok(())
    }

    #[test]
    fn test_find_extra_files_multiple_extra() -> Result<()> {
        let expected_files_rel: HashSet<PathBuf> = [
            PathBuf::from("data/file.dat"),
        ]
        .iter()
        .cloned()
        .collect();

        let dir = setup_test_dir(&["data/file.dat", "extra1.txt", "other/extra2.log"])?;
        let mut extra = find_extra_files(dir.path(), &expected_files_rel)?;
        extra.sort(); // Sort for consistent assertion
        
        assert_eq!(extra.len(), 2);
        assert_eq!(extra[0], dir.path().join("extra1.txt"));
        assert_eq!(extra[1], dir.path().join("other/extra2.log"));
        Ok(())
    }
    
    #[test]
    fn test_find_extra_files_missing_expected() -> Result<()> {
        // Expected has file2.txt, but it's missing locally
        let expected_files_rel: HashSet<PathBuf> = [
            PathBuf::from("file1.txt"),
            PathBuf::from("file2.txt"), 
        ]
        .iter()
        .cloned()
        .collect();

        // Only create file1.txt locally, NO extra files
        let dir = setup_test_dir(&["file1.txt"])?;
        let extra = find_extra_files(dir.path(), &expected_files_rel)?;
        
        // Should find no *extra* files
        assert!(extra.is_empty());
        Ok(())
    }
    
    #[test]
    fn test_find_extra_files_empty_dir() -> Result<()> {
        let expected_files_rel: HashSet<PathBuf> = HashSet::new();
        let dir = setup_test_dir(&[])?; // Empty dir
        let extra = find_extra_files(dir.path(), &expected_files_rel)?;
        assert!(extra.is_empty());
        Ok(())
    }
    
    #[test]
    fn test_find_extra_files_non_existent_dir() -> Result<()> {
        let expected_files_rel: HashSet<PathBuf> = HashSet::new();
        let non_existent_path = PathBuf::from("surely_this_does_not_exist_12345");
        let extra = find_extra_files(&non_existent_path, &expected_files_rel)?;
        assert!(extra.is_empty());
        Ok(())
    }
} 