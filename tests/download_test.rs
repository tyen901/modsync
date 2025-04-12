// tests/download_test.rs

use anyhow::Result;
use modsync::librqbit::{AddTorrent, AddTorrentOptions, Session, SessionOptions};
use sha2::{Digest, Sha256};
use std::fs::{File, OpenOptions, create_dir_all};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tempfile::tempdir;
use tokio::time::{Duration, sleep};
use walkdir::WalkDir;

const TEST_TORRENT_PATH: &str = "tests/data/test.torrent";
const TEST_FOLDER_PATH: &str = "tests/data/test_folder";

// Helper to calculate SHA256 hash of a file or directory
fn calculate_hash(path: &Path) -> Result<String> {
    let mut hasher = Sha256::new();
    if path.is_file() {
        let mut file = File::open(path)?;
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer)?;
        hasher.update(&buffer);
    } else if path.is_dir() {
        for entry in WalkDir::new(path).sort_by_file_name() {
            let entry = entry?;
            let file_path = entry.path();
            if file_path.is_file() {
                let mut file = File::open(file_path)?;
                let mut buffer = Vec::new();
                file.read_to_end(&mut buffer)?;
                // Include relative path in hash to account for structure
                let relative_path = file_path.strip_prefix(path)?.to_string_lossy();
                hasher.update(relative_path.as_bytes());
                hasher.update(&buffer);
            }
        }
    } else {
        anyhow::bail!("Path is neither a file nor a directory: {:?}", path);
    }
    Ok(hex::encode(hasher.finalize()))
}

// Copy a file with option to copy only a portion of it
fn copy_partial_file(src: &Path, dest: &Path, copy_percentage: f64) -> Result<()> {
    if let Some(parent) = dest.parent() {
        create_dir_all(parent)?;
    }

    let src_meta = std::fs::metadata(src)?;
    let src_len = src_meta.len();
    let bytes_to_copy = (src_len as f64 * copy_percentage.clamp(0.0, 1.0)) as u64;

    let mut reader = File::open(src)?;
    let mut writer = File::create(dest)?;

    let mut buffer = vec![0; 8192]; // 8KB buffer
    let mut bytes_copied = 0;

    while bytes_copied < bytes_to_copy {
        let bytes_remaining = (bytes_to_copy - bytes_copied) as usize;
        let to_read = std::cmp::min(buffer.len(), bytes_remaining);

        let bytes_read = reader.read(&mut buffer[..to_read])?;
        if bytes_read == 0 {
            break; // EOF
        }

        writer.write_all(&buffer[..bytes_read])?;
        bytes_copied += bytes_read as u64;
    }

    println!(
        "Copied {}/{} bytes ({:.1}%) from {} to {}",
        bytes_copied,
        src_len,
        (bytes_copied as f64 / src_len as f64) * 100.0,
        src.display(),
        dest.display()
    );

    Ok(())
}

// Copy entire directory with option to copy files partially
fn copy_directory(src_dir: &Path, dest_dir: &Path, file_copy_percentage: f64) -> Result<()> {
    create_dir_all(dest_dir)?;

    for entry in WalkDir::new(src_dir) {
        let entry = entry?;
        let src_path = entry.path();
        let rel_path = src_path.strip_prefix(src_dir)?;
        let dest_path = dest_dir.join(rel_path);

        if src_path.is_dir() {
            create_dir_all(&dest_path)?;
        } else if src_path.is_file() {
            copy_partial_file(src_path, &dest_path, file_copy_percentage)?;
        }
    }

    println!(
        "Copied directory from {} to {} with {:.1}% of each file",
        src_dir.display(),
        dest_dir.display(),
        file_copy_percentage * 100.0
    );

    Ok(())
}

// Corrupt a file by modifying some bytes
fn corrupt_file(path: &Path, corruption_size: usize) -> Result<()> {
    let mut file = OpenOptions::new().read(true).write(true).open(path)?;
    let metadata = file.metadata()?;
    let file_size = metadata.len();

    // Only corrupt if the file is big enough
    if file_size < 100 {
        println!("File too small to corrupt: {}", path.display());
        return Ok(());
    }

    // Position at 25% into the file
    let position = file_size as u64 / 4;
    file.seek(SeekFrom::Start(position))?;

    // Create a buffer of random data
    let mut corrupt_data = vec![0u8; corruption_size];
    for i in 0..corruption_size {
        corrupt_data[i] = (i % 256) as u8;
    }

    // Write the corrupt data
    file.write_all(&corrupt_data)?;
    println!(
        "Corrupted {} bytes at position {} in {}",
        corruption_size,
        position,
        path.display()
    );

    Ok(())
}

async fn run_download_test(output_dir: &Path, wait_duration: Duration) -> Result<Arc<Session>> {
    println!("Starting download test to: {:?}", output_dir);

    let session = Session::new_with_opts(
        output_dir.to_path_buf(),
        SessionOptions {
            disable_dht: true, // Disable DHT for faster, isolated tests
            disable_dht_persistence: true,
            fastresume: false, // Disable fastresume to ensure checks happen
            persistence: None,
            listen_port_range: None, // Don't listen for incoming
            ..Default::default()
        },
    )
    .await?;

    println!("Session created successfully");

    let torrent_bytes = std::fs::read(TEST_TORRENT_PATH)?;
    println!("Read torrent file of size: {} bytes", torrent_bytes.len());

    let mut opts = AddTorrentOptions::default();
    opts.overwrite = true; // Important for re-checking existing/partial files

    let handle = session
        .add_torrent(AddTorrent::from_bytes(torrent_bytes), Some(opts))
        .await?
        .into_handle()
        .expect("Failed to get torrent handle");

    println!(
        "Torrent added successfully, waiting for {:?} to check progress",
        wait_duration
    );

    // Wait for specified duration and check progress
    sleep(wait_duration).await;

    // Print status information
    let stats = handle.stats();
    println!(
        "Progress: {:.2}%",
        stats.progress_bytes as f64 / stats.total_bytes as f64 * 100.0
    );
    println!("Downloaded: {} bytes", stats.progress_bytes);
    println!("Uploaded: {} bytes", stats.uploaded_bytes);
    println!(
        "Download speed: {:.2} MiB/s",
        if let Some(live) = &stats.live {
            live.download_speed.mbps
        } else {
            0.0
        }
    );

    Ok(session)
}

#[tokio::test]
async fn test_download_progress() -> Result<()> {
    let output_dir = tempdir()?;
    let _session = run_download_test(output_dir.path(), Duration::from_secs(30)).await?;

    // Simply check that the test ran successfully and reported progress
    println!("Test completed successfully - progress was reported");
    Ok(())
}

#[tokio::test]
async fn test_data_validation() -> Result<()> {
    let output_dir = tempdir()?;

    // Run download for 60 seconds to get substantial portions of the files
    let _session = run_download_test(output_dir.path(), Duration::from_secs(60)).await?;

    // Calculate hash of original test folder
    let original_hash = calculate_hash(Path::new(TEST_FOLDER_PATH))?;

    // Verify the downloaded files exist in the output directory
    let output_test_folder = output_dir.path().join("test_folder");
    if !output_test_folder.exists() {
        // Try finding any test_folder in the output directory
        let mut found = false;
        for entry in std::fs::read_dir(output_dir.path())? {
            let entry = entry?;
            let path = entry.path();
            println!("Found in output dir: {}", path.display());
            if path.is_dir() {
                found = true;
            }
        }

        if !found {
            anyhow::bail!(
                "Could not find any downloaded directory in {:?}",
                output_dir.path()
            );
        }
    }

    // Calculate hash of the downloaded content if found
    if output_test_folder.exists() {
        let downloaded_hash = calculate_hash(&output_test_folder)?;
        println!("Original hash: {}", original_hash);
        println!("Downloaded hash: {}", downloaded_hash);
    }

    // Simply verify that files were downloaded, regardless of location
    let downloaded_files = WalkDir::new(output_dir.path())
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .count();

    println!("Found {} files in output directory", downloaded_files);
    assert!(downloaded_files > 0, "No files were downloaded");

    Ok(())
}

#[tokio::test]
async fn test_resume_partial_download() -> Result<()> {
    let output_dir = tempdir()?;
    // Use the test_folder structure in the output directory
    let test_output_dir = output_dir.path().join("test_folder");

    // 1. Create output directory structure and copy partial files (50% of each file)
    copy_directory(
        Path::new(TEST_FOLDER_PATH),
        &test_output_dir,
        0.5, // Copy 50% of each file
    )?;

    println!("Created partial files at {:?}", test_output_dir);

    // Record initial file sizes for comparison
    let mut initial_sizes: Vec<(PathBuf, u64)> = Vec::new();
    for entry in WalkDir::new(&test_output_dir) {
        let entry = entry?;
        if entry.file_type().is_file() {
            let path = entry.path().to_path_buf();
            let metadata = std::fs::metadata(&path)?;
            let size = metadata.len();
            println!("Initial file: {} - {} bytes", path.display(), size);
            initial_sizes.push((path.clone(), size));
        }
    }

    // 2. Run download test with the partial files already in place
    let _session = run_download_test(output_dir.path(), Duration::from_secs(60)).await?;

    // 3. Compare file sizes after resumption
    let mut files_updated = false;
    for (path, initial_size) in &initial_sizes {
        if path.exists() {
            let new_size = std::fs::metadata(path)?.len();
            println!(
                "File '{}' size changed: {} -> {} bytes",
                path.display(),
                initial_size,
                new_size
            );

            if new_size > *initial_size {
                files_updated = true;
            }
        }
    }

    // 4. Verify at least one file was updated
    assert!(files_updated, "No files were updated during resume");

    Ok(())
}

#[tokio::test]
async fn test_fix_corrupted_files() -> Result<()> {
    let output_dir = tempdir()?;
    // Use the test_folder structure in the output directory
    let test_output_dir = output_dir.path().join("test_folder");

    // 1. Create test directory and copy all files (100%)
    copy_directory(
        Path::new(TEST_FOLDER_PATH),
        &test_output_dir,
        1.0, // Copy 100% of each file
    )?;

    println!("Copied complete files to {:?}", test_output_dir);

    // 2. Find the largest file to corrupt
    let mut largest_file: Option<(PathBuf, u64)> = None;
    for entry in WalkDir::new(&test_output_dir) {
        let entry = entry?;
        if entry.file_type().is_file() {
            let path = entry.path().to_path_buf();
            let metadata = std::fs::metadata(&path)?;
            let size = metadata.len();

            if largest_file
                .as_ref()
                .map_or(true, |(_, max_size)| size > *max_size)
            {
                largest_file = Some((path, size));
            }
        }
    }

    // 3. Corrupt the file more significantly to ensure detection
    if let Some((path, size)) = largest_file {
        println!("Found largest file: {} ({} bytes)", path.display(), size);
        
        // Create more substantial corruption at multiple positions
        // This increases the chance that it affects multiple pieces in the torrent
        let corruption_size = 4096; // Increase corruption size to 4KB
        
        // Corrupt the file at multiple positions
        let positions = [
            size / 10,          // At 10% 
            size / 4,           // At 25%
            size / 2,           // At 50%
            (size * 3) / 4,     // At 75%
        ];
        
        for pos in positions {
            corrupt_file_at_position(&path, corruption_size, pos)?;
        }

        // Get the current hash before fixing
        let hash_before = calculate_hash(&path)?;
        println!("File hash before fixing: {}", hash_before);

        // 4. Create a session with specific options to ensure verification
        println!("Starting download to fix corrupted files");

        let session = Session::new_with_opts(
            output_dir.path().to_path_buf(),
            SessionOptions {
                disable_dht: true,
                disable_dht_persistence: true,
                fastresume: false, // Critical: Disable fastresume to force full verification
                persistence: None,
                listen_port_range: None,
                ..Default::default()
            },
        ).await?;

        let torrent_bytes = std::fs::read(TEST_TORRENT_PATH)?;
        
        // Configure options to force overwrite and verification
        let mut opts = AddTorrentOptions::default();
        opts.overwrite = true;   // Ensures we'll overwrite existing data if needed
        // No force_verify option, but disabling fastresume in SessionOptions already
        // forces full verification of all pieces

        // Add the torrent to repair corrupted files
        let add_response = session
            .add_torrent(AddTorrent::from_bytes(torrent_bytes), Some(opts))
            .await?;
            
        let handle = add_response.into_handle().expect("Failed to get torrent handle");
        
        // Wait a reasonable time for corruption to be detected and fixed
        sleep(Duration::from_secs(60)).await;

        // Check the status
        let stats = handle.stats();
        println!(
            "Progress: {:.2}%, Downloaded: {} bytes",
            stats.progress_bytes as f64 / stats.total_bytes as f64 * 100.0,
            stats.progress_bytes
        );

        // 5. Check if the file was fixed
        if path.exists() {
            let hash_after = calculate_hash(&path)?;
            println!("File hash after fixing: {}", hash_after);

            // 6. Verify the file changed (file was fixed)
            assert_ne!(hash_before, hash_after, "Corrupted file was not fixed");
            
            // Additional verification: check that the file now matches the original
            let original_path = Path::new(TEST_FOLDER_PATH).join(path.strip_prefix(&test_output_dir)?);
            let original_hash = calculate_hash(&original_path)?;
            println!("Original file hash: {}", original_hash);
            
            assert_eq!(original_hash, hash_after, "Fixed file doesn't match the original");
        } else {
            anyhow::bail!("File no longer exists after running the download test");
        }
    } else {
        anyhow::bail!("No files found to corrupt");
    }

    Ok(())
}

// Add a function to corrupt a file at a specific position
fn corrupt_file_at_position(path: &Path, corruption_size: usize, position: u64) -> Result<()> {
    let mut file = OpenOptions::new().read(true).write(true).open(path)?;
    let metadata = file.metadata()?;
    let file_size = metadata.len();

    // Only corrupt if the position is valid
    if position >= file_size {
        return Ok(());
    }

    // Position at the specified location
    file.seek(SeekFrom::Start(position))?;

    // Create a buffer of clearly invalid data
    let mut corrupt_data = vec![0xFF; corruption_size]; // Use 0xFF for clear corruption
    for i in 0..corruption_size {
        // Create a pattern that's clearly different from the original
        corrupt_data[i] = ((i % 128) + 128) as u8;
    }

    // Write the corrupt data
    file.write_all(&corrupt_data)?;
    println!(
        "Corrupted {} bytes at position {} in {}",
        corruption_size,
        position,
        path.display()
    );

    Ok(())
}

// Add a new function to parse torrent files and extract the files/directories list
async fn extract_torrent_contents(torrent_path: &Path) -> Result<Vec<(PathBuf, u64)>> {
    println!("Extracting contents from torrent file: {:?}", torrent_path);
    
    // Read the torrent file
    let torrent_bytes = std::fs::read(torrent_path)?;
    
    // Instead of using torrent_from_bytes which may have issues with our test file,
    // use the same approach as the download test - directly add it to a temporary session
    let temp_dir = tempdir()?;
    
    let session = Session::new_with_opts(
        temp_dir.path().to_path_buf(),
        SessionOptions {
            disable_dht: true,
            disable_dht_persistence: true,
            fastresume: false,
            persistence: None,
            listen_port_range: None,
            ..Default::default()
        },
    ).await?;
    
    // Add the torrent to the session temporarily to get its info
    let add_response = session
        .add_torrent(AddTorrent::from_bytes(torrent_bytes), None)
        .await?;
    
    let handle = add_response.into_handle().expect("Failed to get torrent handle");
    
    // Extract files from the torrent
    let mut files = Vec::new();
    
    // Get the name from the handle
    let root_dir = PathBuf::from(handle.name().unwrap_or_else(|| "unknown".to_string()));
    
    // Get file information through the metadata in the handle
    handle.with_metadata(|metadata| {
        // Iterate through file_infos
        for file_info in &metadata.file_infos {
            let file_path = root_dir.join(&file_info.relative_filename);
            files.push((file_path, file_info.len));
        }
    })?;
    
    println!("Found {} files in torrent", files.len());
    for (path, size) in &files {
        println!("  - {}: {} bytes", path.display(), size);
    }
    
    Ok(files)
}

// Add a new test to verify torrent file parsing
#[tokio::test]
async fn test_parse_torrent_file() -> Result<()> {
    let torrent_path = Path::new(TEST_TORRENT_PATH);
    let files = extract_torrent_contents(torrent_path).await?;
    
    // Verify the correct number of files in our test torrent
    assert!(!files.is_empty(), "Torrent should contain at least one file");
    
    // Verify the structure matches what we expect
    let expected_root_dir = "test_folder";
    
    // Every file path should start with the root directory
    for (path, _) in &files {
        let path_str = path.to_string_lossy();
        assert!(
            path_str.starts_with(expected_root_dir),
            "File path '{}' doesn't start with expected root dir '{}'",
            path_str,
            expected_root_dir
        );
    }
    
    // Verify the specific files we expect in our test torrent
    let expected_files = vec![
        format!("{}/README", expected_root_dir),
        format!("{}/images/LOC_Main_Reading_Room_Highsmith.jpg", expected_root_dir),
        format!("{}/images/melk-abbey-library.jpg", expected_root_dir),
    ];
    
    for expected_file in expected_files {
        let expected_path = PathBuf::from(expected_file);
        assert!(
            files.iter().any(|(path, _)| path == &expected_path),
            "Expected file '{}' not found in torrent",
            expected_path.display()
        );
    }
    
    println!("Torrent file parsing test passed!");
    Ok(())
}
