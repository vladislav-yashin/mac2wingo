use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::core::options::WriteOptions;
use crate::i18n::Language;

/// Persisted application settings.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    /// `None` means "follow the operating system language".
    pub language: Option<Language>,
    pub last_image_dir: Option<PathBuf>,
    pub last_target_device: Option<String>,
    pub options: WriteOptions,
}

impl AppConfig {
    /// Location of the JSON configuration file, if the platform provides one.
    #[must_use]
    pub fn config_path() -> Option<PathBuf> {
        directories::ProjectDirs::from("dev", "mac2wingo", "Mac2WinGo")
            .map(|dirs| dirs.config_dir().join("config.json"))
    }

    /// Load settings from disk, falling back to defaults on any error.
    #[must_use]
    pub fn load() -> Self {
        let Some(path) = Self::config_path() else {
            return Self::default();
        };
        Self::load_from(&path).unwrap_or_default()
    }

    pub fn load_from(path: &Path) -> Option<Self> {
        let data = std::fs::read_to_string(path).ok()?;
        match serde_json::from_str(&data) {
            Ok(config) => Some(config),
            Err(error) => {
                tracing::warn!(%error, path = %path.display(), "failed to parse config");
                None
            }
        }
    }

    /// Persist settings to their default location.
    pub fn save(&self) -> std::io::Result<()> {
        let Some(path) = Self::config_path() else {
            return Ok(());
        };
        self.save_to(&path)
    }

    pub fn save_to(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let data = serde_json::to_string_pretty(self)
            .map_err(|error| std::io::Error::other(error.to_string()))?;
        std::fs::write(path, data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_json() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");

        let config = AppConfig {
            language: Some(Language::Ru),
            last_target_device: Some("disk5".into()),
            ..AppConfig::default()
        };
        config.save_to(&path).unwrap();

        let loaded = AppConfig::load_from(&path).unwrap();
        assert_eq!(config, loaded);
    }

    #[test]
    fn missing_file_yields_none() {
        let dir = tempfile::tempdir().unwrap();
        assert!(AppConfig::load_from(&dir.path().join("nope.json")).is_none());
    }

    #[test]
    fn default_follows_system_language() {
        assert_eq!(AppConfig::default().language, None);
    }
}
