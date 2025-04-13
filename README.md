# ModSync

ModSync is a simple GUI application built with Rust and egui that uses the BitTorrent protocol to keep a local folder synchronized with the content defined by a remote `.torrent` file URL.

It automatically detects changes in the remote torrent, downloads only the differences, verifies file integrity, and optionally cleans up extraneous files.

## Features

-   **Automatic Synchronization**: Monitors a remote `.torrent` file URL.
-   **Efficient Change Detection**: Uses direct download and hash comparison of the remote `.torrent` file to detect updates.
-   **Differential Downloads**: When updates are detected, only downloads the changed or new pieces using `librqbit`.
-   **File Verification**: Automatically verifies local files against the torrent manifest using `librqbit`.
-   **Missing Files Handling**: Detects files listed in the torrent but missing locally and prompts the user to re-download them.
-   **Folder Cleaning (Optional)**: Allows users to manually verify the download folder and prompts to delete any files present locally that are *not* listed in the current torrent manifest.
-   **Live Status Display**: Shows real-time progress, state (Checking, Downloading, Seeding, Completed, etc.), and download/upload speeds.
-   **Detailed View**: Includes tabs for "Details" (torrent metadata, transfer stats) and "Files" (hierarchical file tree).
-   **Simple Configuration**: Requires only a remote torrent URL and a local download path.

## How It Works

1.  **Configuration**: User provides the remote `.torrent` URL and a local download path.
2.  **Monitoring**: ModSync periodically downloads the remote `.torrent` file and compares its hash to the locally cached version.
3.  **Update Detection**: If the hash differs, it indicates an update.
4.  **User Confirmation**: The user is prompted to apply the update.
5.  **Torrent Update**: If confirmed, ModSync stops the old torrent task (if any) and adds the *new* torrent using the same download path.
6.  **Differential Download**: `librqbit` automatically checks existing files in the download path against the new torrent's metadata and downloads only the necessary pieces.
7.  **Manual Verification**: User can click "Verify Local Files" to:
    *   Check for files listed in the torrent but missing locally.
    *   Check for files present locally but *not* listed in the torrent.
8.  **Prompting**: If missing or extra files are found, the user is prompted to either fix the missing files (by restarting the torrent) or delete the extra files.

## Requirements

-   Any system supported by Rust, egui, and librqbit (Windows, macOS, Linux).
-   Network access to the remote torrent URL.
-   Permissions to read/write to the chosen download location.

## Building from Source

```bash
# Ensure you have Rust and Cargo installed: https://www.rust-lang.org/tools/install
git clone ...
cd modsync
cargo build --release
```

The compiled binary will be available at `target/release/modsync` (or `target\release\modsync.exe` on Windows).

## Usage

1.  Launch the application (`modsync` or `modsync.exe`).
2.  In the **Configuration** section:
    *   Enter the URL of the remote `.torrent` file.
    *   Enter the desired local folder path for downloading/synchronizing.
    *   Click **Save Configuration**.
3.  Optionally, click **Update from Remote** to check for changes immediately.
4.  The **Torrent Status** section will display the current state:
    *   Progress bar, status (Idle, Checking, Downloading, Seeding, etc.), speeds.
    *   Tabs for **Details** (metadata, transfer stats) and **Files** (file tree).
5.  Click **Verify Local Files** to check for missing or extra files and potentially clean the directory.
6.  Click **Open Folder** to open the download directory in your file explorer.

## Technology Stack

-   **Rust**: Core application logic.
-   **egui/eframe**: Immediate mode GUI framework.
-   **librqbit**: Embedded BitTorrent engine library.
-   **tokio**: Asynchronous runtime for background tasks.
-   **reqwest**: HTTP client for fetching the remote torrent file.
-   **serde/toml**: Configuration loading/saving.
-   **walkdir**: Directory scanning for verification.
-   **sha2**: Hash calculation for change detection.

## Project Status

ModSync is functional with core synchronization, update detection, file verification, missing/extra file handling, and a working UI. Future improvements may include:

-   More detailed sync status reporting (e.g., specific stages of checking/updating).
-   Configuration options for automatic updates/cleaning.
-   Improved error handling presentation.
-   More comprehensive automated tests.
-   Graceful shutdown mechanism.
