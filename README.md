# ModSync

ModSync is a lightweight synchronization tool written in Rust that uses the BitTorrent protocol to keep a local folder synchronized with the content defined by a remote `.torrent` file URL.

It automatically detects changes in the remote torrent, downloads only the differences, verifies file integrity, and optionally cleans up extraneous files.

## Features

-   **Automatic Synchronization**: Monitors a remote `.torrent` file URL.
-   **Efficient Change Detection**: Uses direct download and hash comparison of the remote `.torrent` file to detect updates.
-   **Differential Downloads**: When updates are detected, only downloads the changed or new pieces using `librqbit`.
-   **File Verification**: Automatically verifies local files against the torrent manifest using `librqbit`.
-   **Missing Files Handling**: Detects files listed in the torrent but missing locally and can re-download them.
-   **Folder Cleaning (Optional)**: Allows verification of the download folder and removal of files not listed in the current torrent manifest.
-   **Live Status Reporting**: Provides progress and state updates (Checking, Downloading, Seeding, Completed, etc.) via internal events and logs.
-   **Simple Configuration**: Requires only a remote torrent URL and a local download path.

## How It Works

1.  **Configuration**: Provide the remote `.torrent` URL and a local download path.
2.  **Monitoring**: ModSync periodically downloads the remote `.torrent` file and compares its hash to the locally cached version.
3.  **Update Detection**: If the hash differs, it indicates an update.
4.  **Apply Update**: The tool can be instructed to apply the update; ModSync will stop the old torrent task (if any) and add the new torrent using the same download path.
5.  **Differential Download**: `librqbit` checks existing files in the download path against the new torrent's metadata and downloads only the necessary pieces.
6.  **Manual Verification**: A verification routine checks for missing or extra files and offers options to fix or remove them.

## Requirements

-   Any system supported by Rust and librqbit (Windows, macOS, Linux).
-   Network access to the remote torrent URL.
-   Permissions to read/write to the chosen download location.

## Building from Source

```powershell
# Ensure you have Rust and Cargo installed: https://www.rust-lang.org/tools/install
git clone <repo-url>
cd modsync
cargo build --release
```

The compiled binary will be available at `target/release/modsync` (or `target\release\modsync.exe` on Windows).

## Usage

1.  Run the binary (`modsync` or `modsync.exe`).
2.  Configure the tool by editing the configuration file or using the CLI-hooks (see configuration in `src/config`).
3.  Optionally trigger an immediate remote check using the provided command/option.
4.  Monitor logs and status events produced by the tool for progress and state information.

## Technology Stack

-   **Rust**: Core application logic.
-   **librqbit**: Embedded BitTorrent engine library.
-   **tokio**: Asynchronous runtime for background tasks.
-   **reqwest**: HTTP client for fetching the remote torrent file.
-   **serde/toml**: Configuration loading/saving.
-   **walkdir**: Directory scanning for verification.
-   **sha2**: Hash calculation for change detection.

## Project Status

ModSync provides core synchronization, update detection, file verification, missing/extra file handling, and a headless runtime suitable for running as a background process or service. Future improvements may include:

-   More detailed sync status reporting (e.g., specific stages of checking/updating).
-   Configuration options for automatic updates/cleaning.
-   Improved error handling and logging.
-   More comprehensive automated tests.
-   Graceful shutdown mechanism.
