//! Configuration handling for `modsync`.
//!
//! A simple TOML configuration file is stored next to the executable.
//! The configuration stores the URL of the remote repository, the path to
//! the local clone of that repository, the path to the local modpack
//! installation (the folder containing the actual `pbo` files), and an
//! optional path to the Arma 3 executable. The first time the application
//! is launched it will write a default configuration file if none exists.

use anyhow::{Context, Result};
use std::env;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use toml;

/// Representation of the metadata.json file that ships with the modpack.
/// This file is assumed to live at the root of the repository and
/// describes the Arma server the user should connect to.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Metadata {
    /// The hostname or IP address of the Arma server.
    pub address: String,
    /// The port used by the Arma server.  Defaults to 2302 if omitted.
    #[serde(default = "Metadata::default_port")]
    pub port: u16,
    /// Optional password required to connect to the server.
    pub password: Option<String>,
}

impl Metadata {
    /// Provides a default port value when the field is missing in the JSON.
    fn default_port() -> u16 {
        2302
    }
}

/// Top level configuration for the application.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// URL to clone the modpack repository from.  This must be a public
    /// repository.  If using a service like GitHub or GitLab the URL
    /// typically ends with `.git`.
    pub repo_url: String,
    /// Path on disk where the modpack files (the actual `.pbo`s) should be
    /// stored.  If the directory does not exist it will be created.
    pub target_mod_dir: PathBuf,
    /// Optional path to the Arma 3 executable.  If this is `None` the
    /// application will attempt to discover a suitable path automatically.
    pub arma_executable: Option<PathBuf>,
}

impl Default for Config {
    fn default() -> Self {
        // Use a sensible default location relative to the executable.
        // The config file will live next to the executable, so choose
        // mod directory beside it as well.
        let exe_parent = env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.to_path_buf()))
            .unwrap_or_else(|| std::path::PathBuf::from("."));
        let target_mod = exe_parent.join("mods");
        Self {
            repo_url: String::from("https://example.com/your/modpack.git"),
            target_mod_dir: target_mod,
            arma_executable: None,
        }
    }
}

impl Config {
    /// Returns the path where the repository cache is stored. This is always
    /// located next to the executable and is not user-configurable.
    pub fn repo_cache_path(&self) -> PathBuf {
        let exe_parent = env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.to_path_buf()))
            .unwrap_or_else(|| std::path::PathBuf::from("."));
        exe_parent.join("repo")
    }

    /// Location of the configuration file.  The file is stored next to the
    /// executable and uses TOML syntax. The filename used is `config.txt`.
    fn config_path() -> Result<PathBuf> {
        // Always place the configuration file in the same directory as the
        // executable.  If we cannot determine the executable path return an
        // error rather than falling back to platform-specific locations.
        let exe = env::current_exe().with_context(|| "Unable to determine current executable path")?;
        let dir = exe.parent().ok_or_else(|| anyhow::anyhow!("Executable has no parent directory"))?;
        Ok(dir.join("config.txt"))
    }

    /// Loads the configuration from disk or returns a default configuration
    /// when the file does not exist.  The configuration is automatically
    /// saved to disk if it was newly created.
    pub fn load() -> Result<Self> {
        let config_path = Self::config_path()?;
        if config_path.exists() {
            let contents = fs::read_to_string(&config_path).with_context(|| {
                format!(
                    "Failed to read configuration file at {}",
                    config_path.display()
                )
            })?;
            let config: Config = toml::from_str(&contents).with_context(|| {
                format!(
                    "Configuration file {} is not valid TOML",
                    config_path.display()
                )
            })?;
            Ok(config)
        } else {
            // Create a default configuration and write it out.
            let config = Config::default();
            config.save()?;
            Ok(config)
        }
    }

    /// Saves the configuration back to disk.  Any errors encountered will
    /// propagate up to the caller.
    pub fn save(&self) -> Result<()> {
        let config_path = Self::config_path()?;
        // Only create the configuration file if it does not already exist.
        // This prevents the application from overwriting a user-managed
        // configuration; subsequent calls to `save` will be no-ops.
        if config_path.exists() {
            return Ok(());
        }

        let toml = toml::to_string_pretty(self).context("Failed to serialise configuration to TOML")?;
        fs::write(&config_path, toml).with_context(|| {
            format!(
                "Failed to write configuration file to {}",
                config_path.display()
            )
        })?;
        Ok(())
    }

    /// Attempts to read a metadata.json file from the cloned repository.  This
    /// file is expected to live at the root of the repository and is
    /// optional.  When found it contains details about the remote server
    /// specified by the modpack maintainer.
    pub fn read_metadata(&self) -> Result<Option<Metadata>> {
        let meta_path = self.repo_cache_path().join("metadata.json");
        if meta_path.exists() {
            let contents = fs::read_to_string(&meta_path).with_context(|| {
                format!("Failed to read metadata file at {}", meta_path.display())
            })?;
            let meta: Metadata = serde_json::from_str(&contents).with_context(|| {
                format!("Failed to parse metadata file at {}", meta_path.display())
            })?;
            Ok(Some(meta))
        } else {
            Ok(None)
        }
    }
}
