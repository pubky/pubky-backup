//! Application configuration storage.
//!
//! Manages `config.json` at the root of the data directory (`~/.pubky-backup/config.json`).

use super::error::StorageError;
use log::{debug, info, warn};
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

        let storage = ConfigStorage { config_path };

        // Migrate from old config/ subdirectory layout if needed
        storage.migrate_config_dir(data_dir)?;

        Ok(storage)
    }

    /// Migrate config.json from old `config/` subdirectory to root.
    ///
    /// The committed storage layout placed config at `~/.pubky-backup/config/config.json`.
    /// We now store it at `~/.pubky-backup/config.json` directly.
    fn migrate_config_dir(&self, data_dir: &Path) -> Result<(), StorageError> {
        let old_config_dir = data_dir.join("config");
        let old_config_file = old_config_dir.join(CONFIG_FILENAME);

        if !old_config_file.exists() {
            return Ok(());
        }

        info!("Migrating config.json from config/ subdirectory to root");

        // If root config.json doesn't exist yet, move the old one
        if !self.config_path.exists() {
            std::fs::rename(&old_config_file, &self.config_path).or_else(|_| {
                // Cross-filesystem fallback
                std::fs::copy(&old_config_file, &self.config_path)
                    .map(|_| ())
                    .map_err(|e| {
                        StorageError::Internal(format!("Failed to copy config.json to root: {}", e))
                    })
            })?;
        }

        // Clean up old config file and directory
        let _ = std::fs::remove_file(&old_config_file);
        let _ = std::fs::remove_dir(&old_config_dir); // only removes if empty

        Ok(())
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

    /// Write the config to disk.
    pub fn write_config(&self, config: &AppConfig) -> Result<(), StorageError> {
        let json = serde_json::to_string_pretty(config)
            .map_err(|e| StorageError::Internal(format!("Failed to serialize config: {}", e)))?;
        std::fs::write(&self.config_path, json)
            .map_err(|e| StorageError::Internal(format!("Failed to write config.json: {}", e)))?;
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

    #[test]
    fn test_migrate_from_config_subdir() {
        let temp_dir = TempDir::new().unwrap();

        // Create old config/config.json
        let old_config_dir = temp_dir.path().join("config");
        std::fs::create_dir_all(&old_config_dir).unwrap();
        let json = r#"{"last_pubky": "test_key", "sync_interval": 600}"#;
        std::fs::write(old_config_dir.join(CONFIG_FILENAME), json).unwrap();

        // Creating ConfigStorage should migrate
        let storage = ConfigStorage::new(temp_dir.path()).unwrap();

        // Config should be readable from root
        let config = storage.read_config();
        assert_eq!(config.last_pubky.as_deref(), Some("test_key"));
        assert_eq!(config.sync_interval, Some(600));

        // Old config dir should be cleaned up
        assert!(!old_config_dir.join(CONFIG_FILENAME).exists());
        assert!(!old_config_dir.exists());
    }

    #[test]
    fn test_migrate_config_subdir_preserves_root_if_exists() {
        let temp_dir = TempDir::new().unwrap();

        // Create root config.json (newer)
        let root_json = r#"{"sync_interval": 900}"#;
        std::fs::write(temp_dir.path().join(CONFIG_FILENAME), root_json).unwrap();

        // Create old config/config.json (older, should be ignored)
        let old_config_dir = temp_dir.path().join("config");
        std::fs::create_dir_all(&old_config_dir).unwrap();
        let old_json = r#"{"sync_interval": 300}"#;
        std::fs::write(old_config_dir.join(CONFIG_FILENAME), old_json).unwrap();

        let storage = ConfigStorage::new(temp_dir.path()).unwrap();

        // Root config should be preserved (not overwritten)
        let config = storage.read_config();
        assert_eq!(config.sync_interval, Some(900));
    }
}
