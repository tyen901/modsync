// src/config/mod.rs

use anyhow::{Context, Result};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize, Debug, Default, Clone)]
pub struct AppConfig {
    pub torrent_url: String,
    pub download_path: PathBuf,
}

pub fn get_config_path() -> Result<PathBuf> {
    let proj_dirs = ProjectDirs::from("com", "ModSync", "ModSync")
        .context("Failed to get project directories")?;
    let config_dir = proj_dirs.config_dir();
    fs::create_dir_all(config_dir)?;
    Ok(config_dir.join("config.toml"))
}

pub fn load_config(config_path: &Path) -> Result<AppConfig> {
    if config_path.exists() {
        let mut file = File::open(config_path)
            .with_context(|| format!("Failed to open config file: {}", config_path.display()))?;
        let mut contents = String::new();
        file.read_to_string(&mut contents)
            .with_context(|| format!("Failed to read config file: {}", config_path.display()))?;
        let config: AppConfig = toml::from_str(&contents)
            .with_context(|| format!("Failed to parse config file: {}", config_path.display()))?;
        Ok(config)
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
        };

        // Test saving
        save_config(&initial_config, &config_path)?;
        assert!(config_path.exists());

        // Test loading
        let loaded_config = load_config(&config_path)?;
        assert_eq!(initial_config.torrent_url, loaded_config.torrent_url);
        assert_eq!(initial_config.download_path, loaded_config.download_path);

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

        dir.close()?;
        Ok(())
    }

    // Note: Testing get_config_path() directly is tricky as ProjectDirs
    // might behave differently in test environments or across OSes.
    // Relying on load/save tests implicitly covers its basic usage.
} 