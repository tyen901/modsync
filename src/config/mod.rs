// src/config/mod.rs

use anyhow::{Context, Result};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum DownloadMode {
    Auto,           // Try BitTorrent first, fallback to HTTP if needed
    BitTorrentOnly, // Only use BitTorrent
    HttpOnly,       // Only use HTTP downloads
    HttpFallback,   // Use HTTP as fallback after timeout
}

impl Default for DownloadMode {
    fn default() -> Self {
        DownloadMode::Auto
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AppConfig {
    pub torrent_url: String,
    pub download_path: PathBuf,
    pub should_seed: bool,
    pub max_upload_speed: Option<u64>,  // in KB/s, None for unlimited
    pub max_download_speed: Option<u64>, // in KB/s, None for unlimited
    pub http_base_urls: Vec<String>,     // Base URLs for HTTP downloads
    pub download_mode: DownloadMode,     // Download strategy
    pub fallback_timeout_seconds: u64,   // Timeout before HTTP fallback
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            torrent_url: String::new(),
            download_path: PathBuf::new(),
            should_seed: true,  // Default to seeding
            max_upload_speed: None,  // Default to unlimited
            max_download_speed: None,  // Default to unlimited
            http_base_urls: Vec::new(), // Default to empty
            download_mode: DownloadMode::default(), // Auto mode
            fallback_timeout_seconds: 30, // 30 second timeout
        }
    }
}

// Private intermediate structure for backwards compatibility
#[derive(Deserialize)]
struct ConfigLoader {
    torrent_url: Option<String>,
    download_path: Option<PathBuf>,
    should_seed: Option<bool>,
    max_upload_speed: Option<u64>,
    max_download_speed: Option<u64>,
    http_base_urls: Option<Vec<String>>,
    download_mode: Option<DownloadMode>,
    fallback_timeout_seconds: Option<u64>,
}

pub fn get_config_path() -> Result<PathBuf> {
    let proj_dirs = ProjectDirs::from("com", "ModSync", "ModSync")
        .context("Failed to get project directories")?;
    let config_dir = proj_dirs.config_dir();
    fs::create_dir_all(config_dir)?;
    Ok(config_dir.join("config.toml"))
}

// Helper to get the application cache directory
pub fn get_cache_dir() -> Result<PathBuf> {
    let proj_dirs = ProjectDirs::from("com", "ModSync", "ModSync")
        .context("Failed to get project directories")?;
    let cache_dir = proj_dirs.cache_dir();
    fs::create_dir_all(cache_dir)?;
    Ok(cache_dir.to_path_buf())
}

// Helper to get the full path for the cached torrent file
pub fn get_cached_torrent_path() -> Result<PathBuf> {
    Ok(get_cache_dir()?.join("cached.torrent"))
}

pub fn load_config(config_path: &Path) -> Result<AppConfig> {
    if config_path.exists() {
        let mut file = File::open(config_path)
            .with_context(|| format!("Failed to open config file: {}", config_path.display()))?;
        let mut contents = String::new();
        file.read_to_string(&mut contents)
            .with_context(|| format!("Failed to read config file: {}", config_path.display()))?;
        
        // Try to parse with our loader struct that has all fields optional
        match toml::from_str::<ConfigLoader>(&contents) {
            Ok(loader) => {
                // Create AppConfig with defaults and override with values from file
                let default_config = AppConfig::default();
                // Determine if any optional fields were missing so we can upgrade the
                // config file after constructing the full `AppConfig`. Compute this
                // before consuming `loader` so we don't run into move/borrow issues.
                let needs_upgrade = loader.should_seed.is_none() ||
                    loader.max_upload_speed.is_none() ||
                    loader.max_download_speed.is_none() ||
                    loader.http_base_urls.is_none() ||
                    loader.download_mode.is_none() ||
                    loader.fallback_timeout_seconds.is_none();

                let config = AppConfig {
                    torrent_url: loader.torrent_url.unwrap_or(default_config.torrent_url),
                    download_path: loader.download_path.unwrap_or(default_config.download_path),
                    should_seed: loader.should_seed.unwrap_or(default_config.should_seed),
                    max_upload_speed: loader.max_upload_speed.or(default_config.max_upload_speed),
                    max_download_speed: loader.max_download_speed.or(default_config.max_download_speed),
                    http_base_urls: loader.http_base_urls.unwrap_or(default_config.http_base_urls),
                    download_mode: loader.download_mode.unwrap_or(default_config.download_mode),
                    fallback_timeout_seconds: loader.fallback_timeout_seconds.unwrap_or(default_config.fallback_timeout_seconds),
                };

                // Optional: Save config back if it was modified (i.e., defaults were applied)
                // This will update the config file with the new fields
                     if needs_upgrade {
                    println!("Upgrading config file with new profile settings fields");
                    if let Err(e) = save_config(&config, config_path) {
                        eprintln!("Failed to upgrade config file: {}", e);
                        // Continue anyway, not a fatal error
                    }
                }
                
                Ok(config)
            },
            Err(e) => {
                // Fallback to more informative error for troubleshooting
                return Err(anyhow::anyhow!("Failed to parse config file: {} - {}", config_path.display(), e));
            }
        }
    } else {
        // Return default config if file doesn't exist
        Ok(AppConfig::default())
    }
}

pub fn save_config(config: &AppConfig, config_path: &Path) -> Result<()> {
    let contents = toml::to_string_pretty(config)
        .context("Failed to serialize config")?;
    let mut file = File::create(config_path)
        .with_context(|| format!("Failed to create config file: {}", config_path.display()))?;
    file.write_all(contents.as_bytes())
        .with_context(|| format!("Failed to write config file: {}", config_path.display()))?;
    Ok(())
}


#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tempfile::tempdir;

    #[test]
    fn test_save_and_load_config() -> Result<()> {
        let dir = tempdir()?;
        let config_path = dir.path().join("test_config.toml");

        let initial_config = AppConfig {
            torrent_url: "http://example.com/test.torrent".to_string(),
            download_path: PathBuf::from("/tmp/test_download"),
            should_seed: true,
            max_upload_speed: Some(100),
            max_download_speed: Some(500),
            http_base_urls: vec!["http://example.com/files".to_string()],
            download_mode: DownloadMode::Auto,
            fallback_timeout_seconds: 30,
        };

        // Test saving
        save_config(&initial_config, &config_path)?;
        assert!(config_path.exists());

        // Test loading
        let loaded_config = load_config(&config_path)?;
        assert_eq!(initial_config.torrent_url, loaded_config.torrent_url);
        assert_eq!(initial_config.download_path, loaded_config.download_path);
        assert_eq!(initial_config.should_seed, loaded_config.should_seed);
        assert_eq!(initial_config.max_upload_speed, loaded_config.max_upload_speed);
    assert_eq!(initial_config.max_download_speed, loaded_config.max_download_speed);
    assert_eq!(initial_config.http_base_urls, loaded_config.http_base_urls);
    assert_eq!(initial_config.download_mode, loaded_config.download_mode);
    assert_eq!(initial_config.fallback_timeout_seconds, loaded_config.fallback_timeout_seconds);

        dir.close()?;
        Ok(())
    }

    #[test]
    fn test_load_default_config_if_not_exists() -> Result<()> {
        let dir = tempdir()?;
        let config_path = dir.path().join("non_existent_config.toml");

        let loaded_config = load_config(&config_path)?;
        assert_eq!(loaded_config.torrent_url, "");
        assert_eq!(loaded_config.download_path, PathBuf::from(""));
        assert_eq!(loaded_config.should_seed, true);
    assert_eq!(loaded_config.max_upload_speed, None);
    assert_eq!(loaded_config.max_download_speed, None);
    assert_eq!(loaded_config.http_base_urls, Vec::<String>::new());
    assert_eq!(loaded_config.download_mode, DownloadMode::default());
    assert_eq!(loaded_config.fallback_timeout_seconds, 30);

        dir.close()?;
        Ok(())
    }
    
    #[test]
    fn test_load_config_with_missing_fields() -> Result<()> {
        let dir = tempdir()?;
        let config_path = dir.path().join("partial_config.toml");
        
        // Create a config file with only some fields
        let partial_config = r#"
            torrent_url = "http://example.com/test.torrent"
            download_path = "/tmp/test_download"
        "#;
        
        std::fs::write(&config_path, partial_config)?;
        
        // Load the config and check that missing fields use defaults
        let loaded_config = load_config(&config_path)?;
        assert_eq!(loaded_config.torrent_url, "http://example.com/test.torrent");
        assert_eq!(loaded_config.download_path, PathBuf::from("/tmp/test_download"));
    assert_eq!(loaded_config.should_seed, true); // Default value
    assert_eq!(loaded_config.max_upload_speed, None); // Default value
    assert_eq!(loaded_config.max_download_speed, None); // Default value
    assert_eq!(loaded_config.http_base_urls, Vec::<String>::new()); // Default value
    assert_eq!(loaded_config.download_mode, DownloadMode::default()); // Default value
    assert_eq!(loaded_config.fallback_timeout_seconds, 30); // Default value
        
        dir.close()?;
        Ok(())
    }

    // Note: Testing get_config_path() directly is tricky as ProjectDirs
    // might behave differently in test environments or across OSes.
    // Relying on load/save tests implicitly covers its basic usage.
}