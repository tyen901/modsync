// Configuration Module

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use directories::ProjectDirs;
use anyhow::{Context, Result, bail};

// Configuration Struct
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Config {
    pub remote_torrent_url: String,
    pub local_download_path: PathBuf,
}

// Default values for config if file doesn't exist
impl Default for Config {
    fn default() -> Self {
        Self {
            remote_torrent_url: "".to_string(),
            local_download_path: PathBuf::from("./modsync_downloads"), // Default download path
        }
    }
}

// Function to get the configuration file path
pub fn get_config_path() -> Result<PathBuf> {
    if let Some(proj_dirs) = ProjectDirs::from("com", "YourOrg", "ModSync") { // Replace YourOrg
        let config_dir = proj_dirs.config_dir();
        std::fs::create_dir_all(config_dir)?; // Ensure config directory exists
        Ok(config_dir.join("modsync.toml"))
    } else {
        bail!("Could not determine configuration directory")
    }
}

// Function to load configuration
pub fn load_config() -> Result<Config> {
    let config_path = get_config_path()?;
    if config_path.exists() {
        let config_str = std::fs::read_to_string(&config_path)
            .with_context(|| format!("Failed to read config file: {:?}", config_path))?;
        toml::from_str(&config_str)
            .with_context(|| format!("Failed to parse TOML from config file: {:?}", config_path))
    } else {
        Ok(Config::default()) // Return default config if file doesn't exist
    }
}

// Function to save configuration
pub fn save_config(config: &Config) -> Result<()> {
    let config_path = get_config_path()?;
    let config_str = toml::to_string_pretty(config)
        .context("Failed to serialize config to TOML")?;
    std::fs::write(&config_path, config_str)
        .with_context(|| format!("Failed to write config file: {:?}", config_path))
}

#[cfg(test)]
mod tests {
    use super::*; // Import items from parent module (config)
    use tempfile::tempdir;
    use std::path::Path;

    // Mock ProjectDirs for testing
    struct MockProjectDirs {
        config_dir: PathBuf,
    }

    impl MockProjectDirs {
        fn config_dir(&self) -> &Path {
            &self.config_dir
        }
    }

    // Override get_config_path for testing
    // This is a bit tricky as we need to modify global state implicitly
    // A cleaner way might involve passing the config path explicitly, but let's try this first.
    // We'll use a thread_local or OnceCell if mocking ProjectDirs directly proves too difficult.

    fn test_config_path(temp_dir: &Path) -> PathBuf {
        temp_dir.join("modsync.toml")
    }

    #[test]
    fn test_save_and_load_config() -> Result<()> {
        let dir = tempdir()?;
        let config_path = test_config_path(dir.path());

        // Create a dummy config
        let original_config = Config {
            remote_torrent_url: "http://example.com/test.torrent".to_string(),
            local_download_path: PathBuf::from("/tmp/test_downloads"),
        };

        // Mock the save function to use the temp path
        let save_result: Result<()> = {
            let config_str = toml::to_string_pretty(&original_config)?;
            std::fs::write(&config_path, config_str)?;
            Ok(())
        };
        save_result?;

        // Mock the load function to use the temp path
        let loaded_config: Config = {
             if config_path.exists() {
                let config_str = std::fs::read_to_string(&config_path)?;
                toml::from_str(&config_str)?
            } else {
                Config::default()
            }
        };

        assert_eq!(original_config.remote_torrent_url, loaded_config.remote_torrent_url);
        assert_eq!(original_config.local_download_path, loaded_config.local_download_path);

        dir.close()?;
        Ok(())
    }

    #[test]
    fn test_load_default_config() -> Result<()> {
         let dir = tempdir()?;
        // Don't create the file
         let config_path = test_config_path(dir.path());

        // Mock the load function
        let loaded_config = {
            if config_path.exists() {
                 // ... (same as above, but won't run)
                 unreachable!();
            } else {
                Config::default()
            }
        };

        assert_eq!(loaded_config.remote_torrent_url, "");
        assert_eq!(loaded_config.local_download_path, PathBuf::from("./modsync_downloads"));

        dir.close()?;
        Ok(())
    }

    // Note: Directly testing the original get_config_path, load_config, save_config
    // is hard without mocking the `directories` crate or complex setup.
    // The tests above verify the core serialization/deserialization logic and default handling,
    // assuming the path generation works correctly.
} 