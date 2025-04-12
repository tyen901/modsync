# ModSync

ModSync is a BitTorrent-based synchronization utility that keeps a local folder synchronized with the content defined by a remote torrent file URL. It automatically detects changes in the remote torrent and updates the local folder accordingly.

## Features

- **Automatic Synchronization**: Monitors a remote torrent file URL and detects changes using ETag or Last-Modified headers
- **Efficient Updates**: When changes are detected, only downloads the changed/new content
- **Self-Healing**: Automatically verifies and repairs existing files against the torrent metadata
- **Live Status Display**: Shows real-time progress, state, and download/upload speeds
- **Simple Configuration**: Just provide a remote torrent URL and local download path

## How It Works

1. Configure the remote torrent URL and local download folder
2. ModSync monitors the remote URL for changes (via HTTP ETags)
3. When changes are detected, ModSync updates the local folder by:
   - Stopping and removing the existing torrent task
   - Adding the new torrent, pointing to the same local folder
   - Leveraging librqbit to verify existing files and download only what changed

## Requirements

- Any system supported by Rust and librqbit

## Building from Source

```
git clone https://github.com/yourusername/modsync.git
cd modsync
cargo build --release
```

The compiled binary will be available at `target/release/modsync`.

## Usage

1. Launch the application
2. Enter the URL of the remote torrent file
3. Choose a local folder for downloading/synchronizing
4. Save the configuration
5. The application will automatically check for updates and keep files in sync

## Technology Stack

- **Rust**: Core language
- **egui/eframe**: GUI framework
- **librqbit**: BitTorrent library
- **tokio**: Asynchronous runtime
- **reqwest**: HTTP client for remote checks

## Project Status

ModSync is currently in development with core synchronization functionality working. Planned features include:
- Detailed UI sync status reporting
- Folder cleaning to remove extraneous files not in the torrent
- Improved error handling and presentation

## License

[Add license information] 