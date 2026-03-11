//! Application configuration storage.
//!
//! Manages `config.json` at the root of the data directory (`~/.pubky-backup/config.json`).

use super::error::StorageError;
use log::{debug, warn};
use pubky::PublicKey;
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    str::FromStr,
};

pub(crate) const CONFIG_FILENAME: &str = "config.json";

/// Unified application configuration.
///
/// Located at: `~/.pubky-backup/config.json`
#[derive(Serialize, Deserialize, Default, Debug)]
pub(crate) struct AppConfig {
    /// Last used pubky (z32 string).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_pubky: Option<String>,
    /// Sync interval in seconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sync_interval: Option<u64>,
    /// Custom keys directory location.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keys_location: Option<String>,
}

/// Reads and writes the application config file.
pub(crate) struct ConfigStorage {
    config_path: PathBuf,
}

impl ConfigStorage {
    pub fn new(data_dir: &Path) -> Result<Self, StorageError> {
        let config_path = data_dir.join(CONFIG_FILENAME);
        Ok(ConfigStorage { config_path })
    }

    /// Read the config from disk. Returns default if file doesn't exist or is invalid.
    pub fn read_config(&self) -> AppConfig {
        match std::fs::read_to_string(&self.config_path) {
            Ok(json) => serde_json::from_str(&json).unwrap_or_else(|e| {
                warn!("Failed to parse config.json, using defaults: {}", e);
                AppConfig::default()
            }),
            Err(_) => AppConfig::default(),
        }
    }

    /// Write the config to disk atomically.
    ///
    /// Writes to a temporary file in the same directory, then renames it
    /// over the target. This prevents partial reads if the process crashes
    /// mid-write, and avoids concurrent readers seeing truncated JSON.
    pub fn write_config(&self, config: &AppConfig) -> Result<(), StorageError> {
        let json = serde_json::to_string_pretty(config)
            .map_err(|e| StorageError::Internal(format!("Failed to serialize config: {}", e)))?;

        let tmp_path = self.config_path.with_extension("json.tmp");
        std::fs::write(&tmp_path, &json)
            .map_err(|e| StorageError::Internal(format!("Failed to write config tmp: {}", e)))?;
        std::fs::rename(&tmp_path, &self.config_path).map_err(|e| {
            // Clean up tmp on failure
            let _ = std::fs::remove_file(&tmp_path);
            StorageError::Internal(format!("Failed to rename config tmp: {}", e))
        })?;
        Ok(())
    }

    pub fn write_last_pubky(&self, pubky: &PublicKey) -> Result<(), StorageError> {
        let mut config = self.read_config();
        let pubky_str = pubky.z32();
        config.last_pubky = Some(pubky_str.clone());
        self.write_config(&config)?;
        debug!("Last pubky value written: {}", pubky_str);
        Ok(())
    }

    pub fn read_last_pubky(&self) -> Result<Option<PublicKey>, StorageError> {
        let config = self.read_config();
        match config.last_pubky {
            Some(pubky_str) => {
                let pubky = PublicKey::from_str(&pubky_str)
                    .map_err(|e| StorageError::Internal(e.to_string()))?;
                Ok(Some(pubky))
            }
            None => Ok(None),
        }
    }

    pub fn clear_last_pubky(&self) -> Result<(), StorageError> {
        let mut config = self.read_config();
        config.last_pubky = None;
        self.write_config(&config)?;
        debug!("Last pubky value cleared");
        Ok(())
    }

    pub fn write_sync_interval(&self, interval_secs: u64) -> Result<(), StorageError> {
        let mut config = self.read_config();
        config.sync_interval = Some(interval_secs);
        self.write_config(&config)?;
        debug!("Sync interval written: {}s", interval_secs);
        Ok(())
    }

    pub fn read_sync_interval(&self) -> Option<u64> {
        self.read_config().sync_interval
    }

    pub fn read_keys_location(&self) -> Option<PathBuf> {
        self.read_config()
            .keys_location
            .map(PathBuf::from)
            .filter(|p| !p.as_os_str().is_empty())
    }

    pub fn write_keys_location(&self, keys_dir: &Path) -> Result<(), StorageError> {
        let mut config = self.read_config();
        config.keys_location = Some(keys_dir.to_string_lossy().to_string());
        self.write_config(&config)?;
        debug!("Keys location written: {}", keys_dir.display());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TEST_PUBKY;
    use tempfile::TempDir;

    #[test]
    fn test_config_read_write_roundtrip() {
        let temp_dir = TempDir::new().unwrap();
        let storage = ConfigStorage::new(temp_dir.path()).unwrap();

        let config = AppConfig {
            last_pubky: Some(TEST_PUBKY.to_string()),
            sync_interval: Some(300),
            keys_location: Some("/custom/keys".to_string()),
        };
        storage.write_config(&config).unwrap();

        let read = storage.read_config();
        assert_eq!(read.last_pubky.as_deref(), Some(TEST_PUBKY));
        assert_eq!(read.sync_interval, Some(300));
        assert_eq!(read.keys_location.as_deref(), Some("/custom/keys"));
    }

    #[test]
    fn test_config_default_when_missing() {
        let temp_dir = TempDir::new().unwrap();
        let storage = ConfigStorage::new(temp_dir.path()).unwrap();

        let config = storage.read_config();
        assert!(config.last_pubky.is_none());
        assert!(config.sync_interval.is_none());
        assert!(config.keys_location.is_none());
    }

    #[test]
    fn test_config_ignores_unknown_fields() {
        let temp_dir = TempDir::new().unwrap();
        let json = r#"{"last_pubky": null, "sync_interval": 60, "future_field": "hello"}"#;
        std::fs::write(temp_dir.path().join(CONFIG_FILENAME), json).unwrap();

        let storage = ConfigStorage::new(temp_dir.path()).unwrap();
        let config = storage.read_config();
        assert_eq!(config.sync_interval, Some(60));
    }

    #[test]
    fn test_config_graceful_on_corrupted() {
        let temp_dir = TempDir::new().unwrap();
        std::fs::write(temp_dir.path().join(CONFIG_FILENAME), "not valid json").unwrap();

        let storage = ConfigStorage::new(temp_dir.path()).unwrap();
        let config = storage.read_config();
        assert!(config.sync_interval.is_none());
    }

    #[test]
    fn test_keys_location_roundtrip_non_ascii() {
        let temp_dir = TempDir::new().unwrap();
        let storage = ConfigStorage::new(temp_dir.path()).unwrap();

        let path = PathBuf::from("/données/clés/备份");
        storage.write_keys_location(&path).unwrap();
        let read_back = storage.read_keys_location().unwrap();
        assert_eq!(read_back, path);
    }
}
