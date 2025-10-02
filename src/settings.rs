use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// Application settings stored as TOML next to the executable.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct AppSettings {
    pub torrent_url: String,
    pub download_path: PathBuf,
    pub max_upload_speed: Option<u32>,
    pub max_download_speed: Option<u32>,
    pub should_seed: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            torrent_url: String::new(),
            download_path: PathBuf::from("downloads"),
            max_upload_speed: None,
            max_download_speed: None,
            should_seed: false,
        }
    }
}

impl AppSettings {
    /// Determine the settings file path next to the running executable.
    pub fn settings_file_path() -> Result<PathBuf> {
        let exe = std::env::current_exe().context("Failed to determine current exe path")?;
        let dir = exe
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
        Ok(dir.join("modsync-settings.toml"))
    }

    /// Load settings if present, otherwise return defaults.
    pub fn load() -> Result<Self> {
        let path = Self::settings_file_path()?;
        if !path.exists() {
            return Ok(Self::default());
        }
        let s = fs::read_to_string(&path).with_context(|| format!("Failed to read settings file: {}", path.display()))?;
        let settings: Self = toml::from_str(&s).context("Failed to parse settings TOML")?;
        Ok(settings)
    }

    /// Save settings to the file next to the exe.
    pub fn save(&self) -> Result<()> {
        let path = Self::settings_file_path()?;
        let toml = toml::to_string_pretty(self).context("Failed to serialize settings to TOML")?;
        fs::write(&path, toml).with_context(|| format!("Failed to write settings file: {}", path.display()))?;
        Ok(())
    }

    /// Reset settings to defaults by overwriting the file with default values.
    pub fn reset() -> Result<()> {
        let default = Self::default();
        default.save()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_save_and_load_settings() -> Result<()> {
        let tmp = tempdir()?;
        let path = tmp.path().join("modsync-settings.toml");

        // Temporarily override current_exe by creating a fake exe path (we can't change current_exe),
        // so we test save/load by writing directly to the path using the same toml format.
        let mut s = AppSettings::default();
        s.torrent_url = "https://example.com/torrent".into();
        s.download_path = PathBuf::from("/tmp/downloads");

        let toml = toml::to_string_pretty(&s)?;
        fs::write(&path, toml)?;

        let content = fs::read_to_string(&path)?;
        let loaded: AppSettings = toml::from_str(&content)?;
        assert_eq!(s, loaded);
        Ok(())
    }
}
