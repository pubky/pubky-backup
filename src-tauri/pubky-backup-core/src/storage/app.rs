//! Application-level storage.
//!
//! This module provides the main [`AppStorage`] facade and app-level storage components.

use super::common::Storage;
use super::config::ConfigStorage;
use super::error::StorageError;
use super::keys::{KeyStorage, KeysStorage};
use super::migration::migrate_old_structure;
use log::{error, info, warn};
use pubky::{PubkyResource, PublicKey};
use std::path::{Path, PathBuf};

const APP_DATA_DIR_NAME: &str = ".pubky-backup";

// App-level directory names
pub(crate) const LOGS_DIR_NAME: &str = "logs";
pub(crate) const KEYS_DIR_NAME: &str = "keys";

// App-level file names
pub(crate) const ERROR_LOG_FILENAME: &str = "error.log";

// Legacy config file names (used for migration only)
pub(crate) const LEGACY_LAST_PUBKY_FILENAME: &str = "last_pubky";

// Per-key directory/file names (re-exported from keys module)
pub(crate) use super::keys::{CURSOR_FILENAME, DATA_DIR_NAME, STATE_DIR_NAME};

/// Get the root data directory for application storage.
///
/// Returns `~/.pubky-backup` on Unix-like systems or the equivalent on other platforms.
/// Falls back to `./.pubky-backup` if the home directory cannot be determined.
///
/// # Returns
///
/// Path to the data directory
///
/// # Errors
///
/// Returns `StorageError` if the directory path cannot be determined
pub fn get_data_directory() -> Result<PathBuf, StorageError> {
    match dirs::home_dir() {
        Some(home) => Ok(home.join(APP_DATA_DIR_NAME)),
        None => {
            error!("Failed to find home directory, using current directory");
            Ok(PathBuf::from(APP_DATA_DIR_NAME))
        }
    }
}

/// Storage for application-level data (config, global logs).
struct AppDataStorage {
    config: ConfigStorage,
    logs_storage: Storage,
}

impl AppDataStorage {
    fn new(data_dir: &Path) -> Result<Self, StorageError> {
        let logs_dir = data_dir.join(LOGS_DIR_NAME);

        Ok(AppDataStorage {
            config: ConfigStorage::new(data_dir)?,
            logs_storage: Storage::new(&logs_dir)?,
        })
    }

    /// Write error to global error log file.
    pub async fn write_error(&self, url: &str, error_msg: &str) -> Result<(), StorageError> {
        let log_entry = format!(
            "[{}] Failed to fetch {}: {}\n",
            chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC"),
            url,
            error_msg
        );
        error!("{}", log_entry.trim());
        self.logs_storage
            .append(ERROR_LOG_FILENAME, log_entry)
            .await?;
        Ok(())
    }
}

/// Application-specific storage that manages both app data and per-key backup data.
///
/// This is the main storage interface used by [`BackupController`](crate::sync::BackupController).
/// It handles:
/// - Writing/reading/deleting backed-up Pubky resources
/// - Tracking sync progress via cursors
/// - Logging errors (both global and per-key)
/// - Storing application metadata (like the last used pubky)
/// - Creating point-in-time snapshots
///
/// # Automatic Migration
///
/// When [`AppStorage::new()`] is called, it automatically detects and migrates data
/// from the old flat storage structure to the new hierarchical structure. This migration:
/// - Is safe to run multiple times (idempotent)
/// - Preserves all data integrity
/// - Logs migration progress for debugging
/// - Removes old structure only after successful migration
///
/// # Storage Layout
///
/// ```text
/// ~/.pubky-backup/
/// ├── config.json                # App-level configuration
/// ├── logs/                      # App-level logs
/// │   └── error.log              # Global error log
/// └── keys/                      # Per-key data
///     └── <pubky>/
///         ├── state/             # Backup state/metadata
///         │   ├── cursor         # Sync progress
///         │   └── error.log      # Key-specific errors
///         ├── data/              # Actual backed-up data
///         │   └── pub/
///         │       ├── profile.json
///         │       └── ...
///         └── snapshots/         # Key-specific snapshots
///             └── <timestamp>.zip
/// ```
pub struct AppStorage {
    /// Application's data (config, global logs)
    app_data: AppDataStorage,
    /// Per-key storage manager
    keys_storage: KeysStorage,
    /// Root data directory path
    data_dir: PathBuf,
}

impl AppStorage {
    /// Creates a new `AppStorage` instance using the default data directory.
    ///
    /// The data directory is determined by [`get_data_directory`].
    ///
    /// # Errors
    ///
    /// Returns `StorageError` if:
    /// - The data directory cannot be determined
    /// - The directory cannot be created
    /// - Storage initialization fails
    pub fn new() -> Result<Self, StorageError> {
        let data_dir = get_data_directory()?;
        Self::init(data_dir)
    }

    /// Create AppStorage with a custom data directory path.
    ///
    /// This is primarily useful for testing with temporary directories.
    pub fn new_with_path(data_dir: &Path) -> Result<Self, StorageError> {
        Self::init(data_dir.to_path_buf())
    }

    fn init(data_dir: PathBuf) -> Result<Self, StorageError> {
        // Migrate from old structure if needed
        migrate_old_structure(&data_dir)?;

        let app_data = AppDataStorage::new(&data_dir)?;

        // Resolve keys directory: use stored location if valid, otherwise default
        let keys_dir = match app_data.config.read_keys_location() {
            Some(path) if path.exists() => path,
            other => {
                if let Some(path) = other {
                    warn!(
                        "Stored keys location {} does not exist, falling back to default",
                        path.display()
                    );
                }
                let default = data_dir.join(KEYS_DIR_NAME);
                app_data.config.write_keys_location(&default)?;
                default
            }
        };

        Ok(AppStorage {
            app_data,
            keys_storage: KeysStorage::new_with_keys_dir(&keys_dir)?,
            data_dir,
        })
    }

    /// Get storage for a specific key.
    pub fn key_storage(&self, pubky: &PublicKey) -> Result<KeyStorage, StorageError> {
        self.keys_storage.get_key_storage(pubky)
    }

    /// Write data to backup storage using PubkyResource path.
    ///
    /// Errors are logged to the key's error log but don't cause this method to fail.
    ///
    /// # Arguments
    ///
    /// * `resource` - The Pubky resource to store
    /// * `data` - The raw data to store
    pub async fn write(&self, resource: &PubkyResource, data: Vec<u8>) -> Result<(), StorageError> {
        let key_storage = self.key_storage(&resource.owner)?;
        match key_storage.write_data(resource, data).await {
            Ok(_) => Ok(()),
            Err(e) => {
                key_storage
                    .write_error(&resource.to_string(), &format!("Failed to write: {}", e))
                    .await?;
                Ok(())
            }
        }
    }

    /// Delete data from backup storage using PubkyResource path.
    ///
    /// Errors are logged to the key's error log but don't cause this method to fail.
    ///
    /// # Arguments
    ///
    /// * `resource` - The Pubky resource to delete
    pub async fn delete(&self, resource: &PubkyResource) -> Result<(), StorageError> {
        let key_storage = self.key_storage(&resource.owner)?;
        match key_storage.delete_data(resource).await {
            Ok(_) => Ok(()),
            Err(e) => {
                key_storage
                    .write_error(&resource.to_string(), &format!("Failed to delete: {}", e))
                    .await?;
                Ok(())
            }
        }
    }

    /// Read data from backup storage using PubkyResource path.
    ///
    /// Returns the raw bytes stored for the given resource.
    pub async fn read(&self, resource: &PubkyResource) -> Result<Vec<u8>, StorageError> {
        let key_storage = self.key_storage(&resource.owner)?;
        key_storage.read_data(resource).await
    }

    /// Write cursor to track backup progress for a specific pubky.
    ///
    /// The cursor tracks the last processed event from the homeserver's event stream.
    ///
    /// # Arguments
    ///
    /// * `pubky` - The public key to track progress for
    /// * `cursor_value` - The cursor value (event ID) from the last processed event
    pub async fn write_cursor(
        &self,
        pubky: &PublicKey,
        cursor_value: u64,
    ) -> Result<(), StorageError> {
        let key_storage = self.key_storage(pubky)?;
        key_storage.write_cursor(cursor_value).await
    }

    /// Read the backup progress cursor for a specific pubky.
    ///
    /// # Arguments
    ///
    /// * `pubky` - The public key to read the cursor for
    ///
    /// # Returns
    ///
    /// The cursor value, or `None` if no cursor exists yet
    pub async fn read_cursor(&self, pubky: &PublicKey) -> Result<Option<u64>, StorageError> {
        let key_storage = self.key_storage(pubky)?;
        key_storage.read_cursor().await
    }

    /// Write an error to the key-specific error log file.
    ///
    /// Used to log non-critical errors that don't stop the backup process.
    ///
    /// # Arguments
    ///
    /// * `pubky` - The public key this error relates to
    /// * `url` - The URL or resource that caused the error
    /// * `error_msg` - The error message to log
    pub async fn write_error(
        &self,
        pubky: &PublicKey,
        url: &str,
        error_msg: &str,
    ) -> Result<(), StorageError> {
        let key_storage = self.key_storage(pubky)?;
        key_storage.write_error(url, error_msg).await
    }

    /// Write an error to the global error log file.
    ///
    /// Used for errors not specific to any key.
    pub async fn write_global_error(&self, url: &str, error_msg: &str) -> Result<(), StorageError> {
        self.app_data.write_error(url, error_msg).await
    }

    /// Calculate the total size of data stored for a specific pubky.
    ///
    /// Recursively traverses the pubky's data directory and sums file sizes.
    ///
    /// # Arguments
    ///
    /// * `pubky` - The public key to calculate size for
    ///
    /// # Returns
    ///
    /// Total size in bytes, or 0 if the directory doesn't exist or an error occurs
    pub async fn calculate_pubky_size(&self, pubky: &PublicKey) -> u64 {
        match self.key_storage(pubky) {
            Ok(key_storage) => key_storage.calculate_data_size().await,
            Err(_) => 0,
        }
    }

    /// List all public keys that have backed-up data.
    ///
    /// Scans the keys directory for directories that are valid public keys.
    ///
    /// # Returns
    ///
    /// Vector of public key strings
    pub fn list_pubky_directories(&self) -> Result<Vec<String>, StorageError> {
        self.keys_storage.list_keys()
    }

    /// Get the root data directory path (e.g. `~/.pubky-backup`).
    pub fn get_backup_data_dir(&self) -> Result<PathBuf, StorageError> {
        Ok(self.data_dir.clone())
    }

    /// Get the current keys directory path.
    pub fn keys_dir(&self) -> &Path {
        self.keys_storage.keys_dir()
    }

    /// Write the last used pubky to storage.
    ///
    /// Used to remember which pubky was last backed up by the application.
    ///
    /// # Arguments
    ///
    /// * `pubky` - The public key to store
    pub async fn write_last_pubky(&self, pubky: &PublicKey) -> Result<(), StorageError> {
        self.app_data.config.write_last_pubky(pubky)
    }

    /// Read the last used pubky from storage.
    ///
    /// # Returns
    ///
    /// The last used public key, or `None` if none has been stored
    pub async fn read_last_pubky(&self) -> Result<Option<PublicKey>, StorageError> {
        self.app_data.config.read_last_pubky()
    }

    /// Clear the last used pubky from storage.
    pub async fn clear_last_pubky(&self) -> Result<(), StorageError> {
        self.app_data.config.clear_last_pubky()
    }

    /// Read the current keys location from config.
    pub fn read_keys_location(&self) -> Option<PathBuf> {
        self.app_data.config.read_keys_location()
    }

    /// Write the keys location to config.
    pub fn write_keys_location(&self, keys_dir: &Path) -> Result<(), StorageError> {
        self.app_data.config.write_keys_location(keys_dir)
    }

    /// Move the keys directory to a new location.
    ///
    /// Tries `fs::rename` first (instant on same filesystem).
    /// Falls back to recursive copy + delete for cross-filesystem moves.
    /// Updates the `keys_location` config file on success.
    ///
    /// **Must be called with no active controllers** — the caller is
    /// responsible for shutting down the BackupManager first.
    ///
    /// **This `AppStorage` instance must not be used after this call.**
    /// The internal `keys_storage` still points at the old (now moved)
    /// path. The caller must create a fresh `AppStorage` (typically via
    /// a new `BackupManager`) to operate from the new location.
    pub fn move_keys(&self, new_parent: &Path) -> Result<PathBuf, StorageError> {
        let old_keys_dir = self.keys_dir();
        let new_keys_dir = new_parent.join(KEYS_DIR_NAME);

        if old_keys_dir == new_keys_dir {
            return Ok(new_keys_dir);
        }

        if new_keys_dir.exists() {
            return Err(StorageError::Internal(format!(
                "Destination already contains a 'keys' directory: {}",
                new_keys_dir.display()
            )));
        }

        // Ensure parent exists
        std::fs::create_dir_all(new_parent).map_err(|e| {
            StorageError::DirectoryCreation(format!("{}: {}", new_parent.display(), e))
        })?;

        info!(
            "Moving keys from {} to {}",
            old_keys_dir.display(),
            new_keys_dir.display()
        );

        // Try atomic rename first (same filesystem)
        match std::fs::rename(old_keys_dir, &new_keys_dir) {
            Ok(()) => {
                info!("Keys moved via rename (same filesystem)");
            }
            Err(e) => {
                // Cross-device: copy, update config, then delete old
                let is_cross_device = e.kind() == std::io::ErrorKind::CrossesDevices;
                if !is_cross_device {
                    return Err(StorageError::Internal(format!(
                        "Failed to move keys directory: {}",
                        e
                    )));
                }

                info!("Cross-filesystem move detected, copying...");
                if let Err(e) = copy_dir_recursive(old_keys_dir, &new_keys_dir) {
                    let _ = std::fs::remove_dir_all(&new_keys_dir);
                    return Err(e);
                }

                // Update config BEFORE deleting old dir — if delete fails, config
                // already points to the complete new copy.
                self.app_data.config.write_keys_location(&new_keys_dir)?;
                info!("Keys location updated to {}", new_keys_dir.display());

                std::fs::remove_dir_all(old_keys_dir).map_err(|e| {
                    StorageError::Internal(format!(
                        "Failed to remove old keys directory after copy: {}",
                        e
                    ))
                })?;
                info!("Keys copied and old directory removed");

                return Ok(new_keys_dir);
            }
        }

        // Update config to point to new location (same-filesystem rename path)
        self.app_data.config.write_keys_location(&new_keys_dir)?;
        info!("Keys location updated to {}", new_keys_dir.display());

        Ok(new_keys_dir)
    }

    /// Write the sync interval to storage.
    pub async fn write_sync_interval(&self, interval_secs: u64) -> Result<(), StorageError> {
        self.app_data.config.write_sync_interval(interval_secs)
    }

    /// Read the sync interval from storage.
    ///
    /// Returns `None` if no interval has been stored.
    pub fn read_sync_interval(&self) -> Option<u64> {
        self.app_data.config.read_sync_interval()
    }

    /// Create a snapshot (zip archive) of the backed-up data for a specific pubky.
    ///
    /// Creates a compressed zip file containing all data for the given pubky.
    /// The snapshot is stored in `keys/<pubky>/snapshots/` with a timestamped filename.
    ///
    /// # Arguments
    ///
    /// * `pubky` - The public key to create a snapshot for
    ///
    /// # Returns
    ///
    /// Path to the created snapshot file
    ///
    /// # Errors
    ///
    /// Returns `StorageError` if:
    /// - The pubky's data directory doesn't exist
    /// - The data directory is empty (no files to snapshot)
    /// - The snapshots directory cannot be created
    /// - The zip file cannot be created or written
    pub async fn create_snapshot(&self, pubky: &PublicKey) -> Result<PathBuf, StorageError> {
        let key_storage = self.key_storage(pubky)?;
        key_storage.create_snapshot().await
    }
}

/// Recursively copy a directory tree.
fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<(), StorageError> {
    std::fs::create_dir_all(dst)
        .map_err(|e| StorageError::DirectoryCreation(format!("{}: {}", dst.display(), e)))?;

    for entry in std::fs::read_dir(src).map_err(|e| {
        StorageError::Internal(format!("Failed to read directory {}: {}", src.display(), e))
    })? {
        let entry =
            entry.map_err(|e| StorageError::Internal(format!("Failed to read entry: {}", e)))?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());

        if src_path.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            std::fs::copy(&src_path, &dst_path).map_err(|e| {
                StorageError::Internal(format!(
                    "Failed to copy {} to {}: {}",
                    src_path.display(),
                    dst_path.display(),
                    e
                ))
            })?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::TEST_PUBKY;

    use super::*;
    use std::str::FromStr;
    use tempfile::TempDir;

    fn create_test_storage() -> (AppStorage, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let storage = AppStorage::new_with_path(&temp_dir.path().to_path_buf()).unwrap();
        (storage, temp_dir)
    }

    #[tokio::test]
    async fn test_write_and_read_cursor() {
        let (storage, _temp_dir) = create_test_storage();
        let pubky = PublicKey::from_str(TEST_PUBKY).unwrap();

        // Write cursor
        storage.write_cursor(&pubky, 123).await.unwrap();

        // Read cursor back
        let cursor = storage.read_cursor(&pubky).await.unwrap();
        assert_eq!(cursor, Some(123));
    }

    #[tokio::test]
    async fn test_read_cursor_returns_none_if_not_exists() {
        let (storage, _temp_dir) = create_test_storage();
        let pubky = PublicKey::from_str(TEST_PUBKY).unwrap();

        // Read cursor before writing - should return None
        let cursor = storage.read_cursor(&pubky).await.unwrap();
        assert_eq!(cursor, None);

        // Should be able to read it again
        let cursor2 = storage.read_cursor(&pubky).await.unwrap();
        assert_eq!(cursor2, None);
    }

    #[tokio::test]
    async fn test_write_and_delete_resource() {
        let (storage, _temp_dir) = create_test_storage();
        let pubky = PublicKey::from_str(TEST_PUBKY).unwrap();

        let resource = PubkyResource::new(pubky.clone(), "/pub/test.json").unwrap();
        let test_data = b"Hello, World!".to_vec();

        // Write resource
        storage.write(&resource, test_data.clone()).await.unwrap();

        // Read resource back
        let read_data = storage.read(&resource).await.unwrap();
        assert_eq!(read_data, test_data);

        // Delete resource
        storage.delete(&resource).await.unwrap();

        // Reading deleted resource should fail
        assert!(storage.read(&resource).await.is_err());
    }

    #[tokio::test]
    async fn test_calculate_pubky_size() {
        let (storage, _temp_dir) = create_test_storage();
        let pubky = PublicKey::from_str(TEST_PUBKY).unwrap();

        // Initial size should be 0
        let size = storage.calculate_pubky_size(&pubky).await;
        assert_eq!(size, 0);

        // Write some data
        let resource1 = PubkyResource::new(pubky.clone(), "/pub/test1.json").unwrap();
        let resource2 = PubkyResource::new(pubky.clone(), "/pub/test2.json").unwrap();

        storage.write(&resource1, b"data1".to_vec()).await.unwrap();
        storage.write(&resource2, b"data22".to_vec()).await.unwrap();

        // Size should be sum of both files
        let size = storage.calculate_pubky_size(&pubky).await;
        assert!(size > 0);
        assert!(size >= 11);
    }

    #[tokio::test]
    async fn test_write_and_read_last_pubky() {
        let (storage, _temp_dir) = create_test_storage();
        let pubky = PublicKey::from_str(TEST_PUBKY).unwrap();

        // Initially should be None
        let last = storage.read_last_pubky().await.unwrap();
        assert!(last.is_none());

        // Write a pubky
        storage.write_last_pubky(&pubky).await.unwrap();

        // Read it back
        let last = storage.read_last_pubky().await.unwrap();
        assert_eq!(last, Some(pubky));
    }

    #[tokio::test]
    async fn test_list_pubky_directories() {
        let (storage, _temp_dir) = create_test_storage();
        let pubky1 = PublicKey::from_str(TEST_PUBKY).unwrap();
        let pubky2 =
            PublicKey::from_str("o4dksfbqk85ogzdb5osziw6befigbuxmuxkuxq8434q89uj56uxo").unwrap();

        // Initially should be empty
        let dirs = storage.list_pubky_directories().unwrap();
        assert_eq!(dirs.len(), 0);

        // Write data for two different pubkys
        let resource1 = PubkyResource::new(pubky1.clone(), "/pub/test1.json").unwrap();
        let resource2 = PubkyResource::new(pubky2.clone(), "/pub/test2.json").unwrap();

        storage.write(&resource1, b"data1".to_vec()).await.unwrap();
        storage.write(&resource2, b"data2".to_vec()).await.unwrap();

        // Should now list both pubky directories (in z32 format)
        let dirs = storage.list_pubky_directories().unwrap();
        assert_eq!(dirs.len(), 2);
        assert!(dirs.contains(&pubky1.z32()));
        assert!(dirs.contains(&pubky2.z32()));
    }

    #[tokio::test]
    async fn test_create_snapshot() {
        use std::io::Read;

        let (storage, temp_dir) = create_test_storage();
        let pubky = PublicKey::from_str(TEST_PUBKY).unwrap();

        // Create some test data first
        let resource1 = PubkyResource::new(pubky.clone(), "/pub/test1.json").unwrap();
        let resource2 = PubkyResource::new(pubky.clone(), "/pub/nested/test2.json").unwrap();

        storage
            .write(&resource1, b"content_one".to_vec())
            .await
            .unwrap();
        storage
            .write(&resource2, b"content_two".to_vec())
            .await
            .unwrap();

        // Create a snapshot
        let snapshot_path = storage.create_snapshot(&pubky).await.unwrap();

        // Verify snapshot file exists
        assert!(snapshot_path.exists(), "Snapshot file should exist");

        // Verify snapshot is in the temp directory
        assert!(
            snapshot_path.starts_with(temp_dir.path()),
            "Snapshot should be in temp directory"
        );

        // Verify the snapshot is a valid zip file
        let file = std::fs::File::open(&snapshot_path).unwrap();
        let mut zip_archive = zip::ZipArchive::new(file).unwrap();

        // Collect file names from archive
        let file_names: Vec<String> = (0..zip_archive.len())
            .map(|i| zip_archive.by_index(i).unwrap().name().to_string())
            .collect();

        // Verify expected files are in the archive
        assert!(file_names.iter().any(|n| n.contains("test1.json")));
        assert!(file_names.iter().any(|n| n.contains("test2.json")));

        // Verify file contents
        for i in 0..zip_archive.len() {
            let mut file = zip_archive.by_index(i).unwrap();
            let name = file.name().to_string();
            let mut contents = Vec::new();
            file.read_to_end(&mut contents).unwrap();

            if name.contains("test1.json") {
                assert_eq!(contents, b"content_one");
            } else if name.contains("test2.json") {
                assert_eq!(contents, b"content_two");
            }
        }
    }

    #[tokio::test]
    async fn test_create_snapshot_no_data() {
        let (storage, _temp_dir) = create_test_storage();
        let pubky =
            PublicKey::from_str("o4dksfbqk85ogzdb5osziw6befigbuxmuxkuxq8434q89uj56uxo").unwrap();

        // Try to create snapshot for pubky with no data written
        let result = storage.create_snapshot(&pubky).await;

        assert!(result.is_err(), "Should fail when no backup data exists");
        let error_msg = result.unwrap_err().to_string();
        assert!(error_msg.contains("No files to snapshot"));
    }

    #[tokio::test]
    async fn test_write_and_read_sync_interval() {
        let (storage, _temp_dir) = create_test_storage();

        // Initially should be None
        let interval = storage.read_sync_interval();
        assert!(interval.is_none());

        // Write an interval
        storage.write_sync_interval(600).await.unwrap();

        // Read it back
        let interval = storage.read_sync_interval();
        assert_eq!(interval, Some(600));

        // Overwrite with a new value
        storage.write_sync_interval(1800).await.unwrap();
        let interval = storage.read_sync_interval();
        assert_eq!(interval, Some(1800));
    }

    #[tokio::test]
    async fn test_read_sync_interval_with_corrupted_file() {
        use super::super::config::CONFIG_FILENAME;

        let (_storage, temp_dir) = create_test_storage();

        // Write invalid JSON directly to the config.json file
        std::fs::write(temp_dir.path().join(CONFIG_FILENAME), "not valid json").unwrap();

        // Re-create storage to read the corrupted file
        let storage = AppStorage::new_with_path(temp_dir.path()).unwrap();

        // Should return None (graceful fallback), not error
        let interval = storage.read_sync_interval();
        assert!(
            interval.is_none(),
            "Should return None for corrupted config"
        );
    }

    #[tokio::test]
    async fn test_storage_directory_structure() {
        use super::super::config::CONFIG_FILENAME;

        let (storage, temp_dir) = create_test_storage();
        let pubky = PublicKey::from_str(TEST_PUBKY).unwrap();

        // Write some data to trigger directory creation
        let resource = PubkyResource::new(pubky.clone(), "/pub/test.json").unwrap();
        storage.write(&resource, b"test".to_vec()).await.unwrap();

        // Write last pubky to trigger config file creation
        storage.write_last_pubky(&pubky).await.unwrap();

        // Verify directory structure
        let root = temp_dir.path();

        // App-level files
        assert!(
            root.join(CONFIG_FILENAME).exists(),
            "config.json should exist at root"
        );
        assert!(root.join(LOGS_DIR_NAME).exists(), "logs/ should exist");

        // Per-key directories
        let key_dir = root.join(KEYS_DIR_NAME).join(pubky.z32());
        assert!(key_dir.exists(), "keys/<pubky>/ should exist");
        assert!(
            key_dir.join(STATE_DIR_NAME).exists(),
            "keys/<pubky>/state/ should exist"
        );
        assert!(
            key_dir.join(DATA_DIR_NAME).exists(),
            "keys/<pubky>/data/ should exist"
        );
        assert!(
            key_dir
                .join(DATA_DIR_NAME)
                .join("pub")
                .join("test.json")
                .exists(),
            "keys/<pubky>/data/pub/test.json should exist"
        );
    }

    #[tokio::test]
    async fn test_keys_location_default_written_on_init() {
        use super::super::config::{AppConfig, CONFIG_FILENAME};

        let temp_dir = TempDir::new().unwrap();
        let _storage = AppStorage::new_with_path(temp_dir.path()).unwrap();

        // config.json should be created at root with keys_location set to default path
        let config_file = temp_dir.path().join(CONFIG_FILENAME);
        assert!(config_file.exists(), "config.json should exist");

        let config: AppConfig =
            serde_json::from_str(&std::fs::read_to_string(&config_file).unwrap()).unwrap();
        let expected = temp_dir.path().join(KEYS_DIR_NAME);
        assert_eq!(
            config.keys_location.as_deref(),
            Some(expected.to_string_lossy().as_ref())
        );
    }

    #[tokio::test]
    async fn test_keys_location_persisted_survives_restart() {
        let temp_dir = TempDir::new().unwrap();

        // Create storage — writes default keys_location
        let storage = AppStorage::new_with_path(temp_dir.path()).unwrap();
        let original_keys_dir = storage.keys_dir();

        // Create new storage from same path — should read stored location
        let storage2 = AppStorage::new_with_path(temp_dir.path()).unwrap();
        assert_eq!(storage2.keys_dir(), original_keys_dir);
    }

    #[tokio::test]
    async fn test_move_keys_same_filesystem() {
        let temp_dir = TempDir::new().unwrap();
        let storage = AppStorage::new_with_path(temp_dir.path()).unwrap();
        let pubky = PublicKey::from_str(TEST_PUBKY).unwrap();

        // Write some data
        let resource = PubkyResource::new(pubky.clone(), "/pub/test.json").unwrap();
        storage
            .write(&resource, b"test data".to_vec())
            .await
            .unwrap();

        // Move to new location within same temp dir
        let new_parent = temp_dir.path().join("new_location");
        let new_keys_dir = storage.move_keys(&new_parent).unwrap();

        // Old location should be gone
        let old_keys_dir = temp_dir.path().join(KEYS_DIR_NAME);
        assert!(!old_keys_dir.exists(), "Old keys dir should be removed");

        // New location should have the data
        assert!(new_keys_dir.exists(), "New keys dir should exist");
        let data_file = new_keys_dir
            .join(pubky.z32())
            .join(DATA_DIR_NAME)
            .join("pub")
            .join("test.json");
        assert!(data_file.exists(), "Data should exist in new location");

        // Config should be updated
        let stored_location = storage.read_keys_location().unwrap();
        assert_eq!(stored_location, new_keys_dir);
    }

    #[tokio::test]
    async fn test_move_keys_same_location_is_noop() {
        let temp_dir = TempDir::new().unwrap();
        let storage = AppStorage::new_with_path(temp_dir.path()).unwrap();

        // Move to same parent — should be a no-op
        let result = storage.move_keys(temp_dir.path()).unwrap();
        assert_eq!(result, temp_dir.path().join(KEYS_DIR_NAME));
    }

    #[tokio::test]
    async fn test_move_keys_rejects_existing_destination() {
        let temp_dir = TempDir::new().unwrap();
        let storage = AppStorage::new_with_path(temp_dir.path()).unwrap();

        // Create a destination that already has a keys/ directory
        let new_parent = temp_dir.path().join("dest");
        std::fs::create_dir_all(new_parent.join(KEYS_DIR_NAME)).unwrap();

        let result = storage.move_keys(&new_parent);
        assert!(
            result.is_err(),
            "Should reject when destination keys/ exists"
        );
    }

    #[tokio::test]
    async fn test_move_keys_new_storage_reads_from_new_location() {
        let temp_dir = TempDir::new().unwrap();
        let storage = AppStorage::new_with_path(temp_dir.path()).unwrap();
        let pubky = PublicKey::from_str(TEST_PUBKY).unwrap();

        // Write data
        let resource = PubkyResource::new(pubky.clone(), "/pub/test.json").unwrap();
        storage.write(&resource, b"hello".to_vec()).await.unwrap();

        // Move
        let new_parent = temp_dir.path().join("moved");
        let new_keys_dir = storage.move_keys(&new_parent).unwrap();

        // Create fresh storage from same config dir — should use new location
        let storage2 = AppStorage::new_with_path(temp_dir.path()).unwrap();
        assert_eq!(storage2.keys_dir(), new_keys_dir);

        // Should be able to list keys from new location
        let keys = storage2.list_pubky_directories().unwrap();
        assert_eq!(keys.len(), 1);
        assert!(keys.contains(&pubky.z32()));
    }

    #[tokio::test]
    async fn test_keys_location_fallback_when_stored_path_missing() {
        use super::super::config::{AppConfig, CONFIG_FILENAME};

        let temp_dir = TempDir::new().unwrap();

        // Create storage so config is written
        let storage = AppStorage::new_with_path(temp_dir.path()).unwrap();
        drop(storage);

        // Overwrite keys_location in config.json to point to a non-existent path
        let config_file = temp_dir.path().join(CONFIG_FILENAME);
        let mut config: AppConfig =
            serde_json::from_str(&std::fs::read_to_string(&config_file).unwrap()).unwrap();
        config.keys_location = Some("/nonexistent/keys".to_string());
        std::fs::write(&config_file, serde_json::to_string_pretty(&config).unwrap()).unwrap();

        // Creating new storage should fall back to default and update config
        let storage2 = AppStorage::new_with_path(temp_dir.path()).unwrap();
        let expected_default = temp_dir.path().join(KEYS_DIR_NAME);
        assert_eq!(
            storage2.keys_dir(),
            expected_default,
            "Should fall back to default keys dir"
        );

        // Config should have been updated to the default
        let updated: AppConfig =
            serde_json::from_str(&std::fs::read_to_string(&config_file).unwrap()).unwrap();
        assert_eq!(
            updated.keys_location.as_deref(),
            Some(expected_default.to_string_lossy().as_ref())
        );
    }

    #[tokio::test]
    async fn test_clear_last_pubky_preserves_other_fields() {
        let (storage, _temp_dir) = create_test_storage();
        let pubky = PublicKey::from_str(TEST_PUBKY).unwrap();

        // Set all config values
        storage.write_last_pubky(&pubky).await.unwrap();
        storage.write_sync_interval(300).await.unwrap();

        // Clear only last_pubky
        storage.clear_last_pubky().await.unwrap();

        // last_pubky should be gone, sync_interval should remain
        let last = storage.read_last_pubky().await.unwrap();
        assert!(last.is_none());
        let interval = storage.read_sync_interval();
        assert_eq!(interval, Some(300));
    }

    /// Verify that storage reads work without a BackupManager.
    ///
    /// The startup screen calls `get_last_pubky` and `get_keys` to decide
    /// whether to auto-navigate to the dashboard.  These must return
    /// instantly from disk — they must NOT depend on BackupManager being
    /// initialised (which does slow network calls in `resume_stored_keys`).
    #[tokio::test]
    async fn test_storage_reads_work_without_manager() {
        let temp_dir = TempDir::new().unwrap();
        let pubky = PublicKey::from_str(TEST_PUBKY).unwrap();

        // Simulate a previous session: write last_pubky and create a key directory
        {
            let storage = AppStorage::new_with_path(temp_dir.path()).unwrap();
            storage.write_last_pubky(&pubky).await.unwrap();
            let resource = PubkyResource::new(pubky.clone(), "/pub/test.json").unwrap();
            storage.write(&resource, b"data".to_vec()).await.unwrap();
        }
        // storage is dropped — no manager, no controllers

        // A fresh AppStorage (cheap, no network) should read everything back
        let storage2 = AppStorage::new_with_path(temp_dir.path()).unwrap();

        let last = storage2.read_last_pubky().await.unwrap();
        assert_eq!(
            last,
            Some(pubky.clone()),
            "last_pubky should be readable without a manager"
        );

        let dirs = storage2.list_pubky_directories().unwrap();
        assert_eq!(
            dirs.len(),
            1,
            "key directory should be listed without a manager"
        );
        assert_eq!(dirs[0], pubky.z32());
    }
}
