use crate::error::{OperationFailedError, StorageError};
use crate::storage_migration::migrate_old_structure;
use futures_lite::StreamExt;
use log::{debug, error, info};
use opendal::{services::Fs, Operator};
use pubky::{PubkyResource, PublicKey};
use std::{
    fs::File,
    io::Write,
    path::{Path, PathBuf},
    str::FromStr,
};
use walkdir::WalkDir;
use zip::write::FileOptions;

const APP_DATA_DIR_NAME: &str = ".pubky-backup";

// App-level directory names
pub(crate) const CONFIG_DIR_NAME: &str = "config";
pub(crate) const LOGS_DIR_NAME: &str = "logs";
pub(crate) const KEYS_DIR_NAME: &str = "keys";

// App-level file names
pub(crate) const LAST_PUBKY_FILENAME: &str = "last_pubky";
pub(crate) const ERROR_LOG_FILENAME: &str = "error.log";

// Per-key directory names
pub(crate) const STATE_DIR_NAME: &str = "state";
pub(crate) const DATA_DIR_NAME: &str = "data";
const SNAPSHOTS_DIR_NAME: &str = "snapshots";

// Per-key file names
pub(crate) const CURSOR_FILENAME: &str = "cursor";

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

/// Simple storage abstraction for reading/writing/deleting files
struct Storage {
    operator: Operator,
}

impl Storage {
    fn new(data_dir: &Path) -> Result<Self, StorageError> {
        // Ensure the directory exists
        std::fs::create_dir_all(data_dir).map_err(|e| {
            StorageError::DirectoryCreation(format!("{}: {}", data_dir.display(), e))
        })?;

        let builder = Fs::default().root(data_dir.to_string_lossy().as_ref());
        let operator = Operator::new(builder)?
            .layer(opendal::layers::LoggingLayer::default())
            .finish();
        Ok(Storage { operator })
    }

    async fn write(&self, file_path: &str, data: impl Into<Vec<u8>>) -> Result<(), StorageError> {
        self.operator
            .write(file_path, data.into())
            .await
            .map_err(|e| {
                StorageError::OperationFailed(Box::new(OperationFailedError {
                    operation: "write".to_string(),
                    path: file_path.to_string(),
                    source: e,
                }))
            })?;
        Ok(())
    }

    async fn read(&self, file_path: &str) -> Result<Vec<u8>, StorageError> {
        let data = self.operator.read(file_path).await.map_err(|e| {
            StorageError::OperationFailed(Box::new(OperationFailedError {
                operation: "read".to_string(),
                path: file_path.to_string(),
                source: e,
            }))
        })?;
        Ok(data.to_vec())
    }

    async fn delete(&self, file_path: &str) -> Result<(), StorageError> {
        self.operator.delete(file_path).await.map_err(|e| {
            StorageError::OperationFailed(Box::new(OperationFailedError {
                operation: "delete".to_string(),
                path: file_path.to_string(),
                source: e,
            }))
        })?;
        Ok(())
    }

    async fn stat(&self, path: &str) -> Result<opendal::Metadata, StorageError> {
        Ok(self.operator.stat(path).await?)
    }

    async fn lister_recursive(&self, path: &str) -> Result<opendal::Lister, StorageError> {
        Ok(self.operator.lister_with(path).recursive(true).await?)
    }

    async fn rename(&self, from: &str, to: &str) -> Result<(), StorageError> {
        self.operator.rename(from, to).await.map_err(|e| {
            StorageError::OperationFailed(Box::new(OperationFailedError {
                operation: format!("rename {} to {}", from, to),
                path: from.to_string(),
                source: e,
            }))
        })?;
        Ok(())
    }

    /// Append data to a file, creating it if it doesn't exist
    async fn append(&self, file_path: &str, data: impl Into<Vec<u8>>) -> Result<(), StorageError> {
        let mut writer = self
            .operator
            .writer_with(file_path)
            .append(true)
            .await
            .map_err(|e| {
                StorageError::OperationFailed(Box::new(OperationFailedError {
                    operation: "append (open writer)".to_string(),
                    path: file_path.to_string(),
                    source: e,
                }))
            })?;

        writer.write(data.into()).await.map_err(|e| {
            StorageError::OperationFailed(Box::new(OperationFailedError {
                operation: "append (write)".to_string(),
                path: file_path.to_string(),
                source: e,
            }))
        })?;

        writer.close().await.map_err(|e| {
            StorageError::OperationFailed(Box::new(OperationFailedError {
                operation: "append (close)".to_string(),
                path: file_path.to_string(),
                source: e,
            }))
        })?;

        Ok(())
    }
}

/// Storage for application-level data (config, global logs)
/// Located at: ~/.pubky-backup/config/ and ~/.pubky-backup/logs/
pub struct AppDataStorage {
    config_storage: Storage,
    logs_storage: Storage,
}

impl AppDataStorage {
    fn new(data_dir: &Path) -> Result<Self, StorageError> {
        let config_dir = data_dir.join(CONFIG_DIR_NAME);
        let logs_dir = data_dir.join(LOGS_DIR_NAME);
        Ok(AppDataStorage {
            config_storage: Storage::new(&config_dir)?,
            logs_storage: Storage::new(&logs_dir)?,
        })
    }

    /// Write error to global error log file
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

    pub async fn write_last_pubky(&self, pubky: &PublicKey) -> Result<(), StorageError> {
        let pubky_str = pubky.to_string();
        self.config_storage
            .write(LAST_PUBKY_FILENAME, pubky_str.clone())
            .await?;
        debug!("Last pubky value written: {}", pubky_str);
        Ok(())
    }

    pub async fn read_last_pubky(&self) -> Result<Option<PublicKey>, StorageError> {
        match self.config_storage.read(LAST_PUBKY_FILENAME).await {
            Ok(data) => {
                let pubky = PublicKey::from_str(&String::from_utf8(data.to_vec())?)
                    .map_err(|e| StorageError::Internal(e.to_string()))?;
                Ok(Some(pubky))
            }
            Err(_) => Ok(None),
        }
    }
}

/// Storage for a single Pubky key's backup data
/// Located at: ~/.pubky-backup/keys/<pubky>/
///
/// Structure:
/// - state/cursor - Sync progress cursor
/// - state/error.log - Key-specific error log
/// - data/pub/... - Backed-up resources
/// - snapshots/<timestamp>.zip - Point-in-time snapshots
pub struct KeyStorage {
    /// The public key this storage is for
    pubky: PublicKey,
    /// Storage for state files (cursor, error log)
    state_storage: Storage,
    /// Storage for backed-up data
    data_storage: Storage,
    /// Root directory for this key (for snapshot creation)
    key_dir: PathBuf,
}

impl KeyStorage {
    fn new(keys_dir: &Path, pubky: &PublicKey) -> Result<Self, StorageError> {
        let key_dir = keys_dir.join(pubky.to_string());
        let state_dir = key_dir.join(STATE_DIR_NAME);
        let data_dir = key_dir.join(DATA_DIR_NAME);

        Ok(KeyStorage {
            pubky: pubky.clone(),
            state_storage: Storage::new(&state_dir)?,
            data_storage: Storage::new(&data_dir)?,
            key_dir,
        })
    }

    /// Write cursor to track backup progress
    /// Uses atomic write-then-rename to prevent torn writes on crashes
    pub async fn write_cursor(&self, cursor_value: String) -> Result<(), StorageError> {
        let temp_path = format!("{}.tmp", CURSOR_FILENAME);
        self.state_storage
            .write(&temp_path, cursor_value.clone())
            .await?;
        self.state_storage
            .rename(&temp_path, CURSOR_FILENAME)
            .await?;
        debug!("Cursor value written: {}", cursor_value);
        Ok(())
    }

    /// Read existing or create new cursor
    pub async fn read_cursor(&self) -> Result<String, StorageError> {
        match self.state_storage.read(CURSOR_FILENAME).await {
            Ok(cursor_data) => {
                let cursor_string = String::from_utf8(cursor_data)?;
                Ok(cursor_string)
            }
            Err(_) => {
                info!("Cursor file not found, creating empty cursor file");
                self.state_storage.write(CURSOR_FILENAME, "").await?;
                Ok(String::new())
            }
        }
    }

    /// Write error to key-specific error log
    pub async fn write_error(&self, url: &str, error_msg: &str) -> Result<(), StorageError> {
        let log_entry = format!(
            "[{}] {}: {}\n",
            chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC"),
            url,
            error_msg
        );

        self.state_storage
            .append(ERROR_LOG_FILENAME, log_entry.clone())
            .await?;

        error!("[{}] {}", self.pubky, log_entry.trim());
        Ok(())
    }

    /// Write data to backup storage using PubkyResource path
    pub async fn write_data(
        &self,
        resource: &PubkyResource,
        data: Vec<u8>,
    ) -> Result<(), StorageError> {
        // Extract path from resource (e.g., "/pub/profile.json")
        let file_path = resource.path.as_str();
        self.data_storage.write(file_path, data).await?;
        Ok(())
    }

    /// Delete data from backup storage using PubkyResource path
    pub async fn delete_data(&self, resource: &PubkyResource) -> Result<(), StorageError> {
        let file_path = resource.path.as_str();
        self.data_storage.delete(file_path).await?;
        Ok(())
    }

    /// Read data from backup storage using PubkyResource path
    #[cfg(test)]
    pub async fn read_data(&self, resource: &PubkyResource) -> Result<Vec<u8>, StorageError> {
        let file_path = resource.path.as_str();
        self.data_storage.read(file_path).await
    }

    /// Calculate the total size of backed-up data for this key
    pub async fn calculate_data_size(&self) -> u64 {
        match self.calculate_dir_size("").await {
            Ok(size) => size,
            Err(e) => {
                error!(
                    "Failed to calculate data size for pubky {}: {}",
                    self.pubky, e
                );
                0
            }
        }
    }

    /// Recursively calculate directory size
    async fn calculate_dir_size(&self, path: &str) -> Result<u64, StorageError> {
        let mut total_size = 0u64;

        // For empty path, we want to list everything
        let list_path = if path.is_empty() { "/" } else { path };

        // Check if directory exists first
        match self.data_storage.stat(list_path).await {
            Ok(metadata) => {
                if !metadata.is_dir() {
                    return Ok(0);
                }
            }
            Err(_) => {
                return Ok(0); // Directory doesn't exist, return 0 size
            }
        }

        let mut entries = self.data_storage.lister_recursive(list_path).await?;
        while let Some(entry) = entries.next().await {
            match entry {
                Ok(entry) => {
                    let metadata = entry.metadata();

                    if metadata.is_file() {
                        // Get actual file size using stat() since lister metadata.content_length() returns 0
                        let size = match self.data_storage.stat(entry.path()).await {
                            Ok(file_metadata) => file_metadata.content_length(),
                            Err(_) => metadata.content_length(), // Fallback to original metadata
                        };
                        total_size += size;
                    }
                }
                Err(e) => {
                    error!("Error listing entry: {}", e);
                    return Err(e.into());
                }
            }
        }
        Ok(total_size)
    }

    /// Create a snapshot (zip archive) of the backed-up data
    pub async fn create_snapshot(&self) -> Result<PathBuf, StorageError> {
        let key_dir = self.key_dir.clone();

        // Run blocking I/O in a separate thread to avoid blocking the async runtime
        let result = tokio::task::spawn_blocking(move || Self::create_snapshot_blocking(&key_dir))
            .await
            .map_err(|e| StorageError::Internal(format!("Snapshot task failed: {}", e)))?;

        result
    }

    /// Blocking implementation of snapshot creation.
    fn create_snapshot_blocking(key_dir: &Path) -> Result<PathBuf, StorageError> {
        let data_dir = key_dir.join(DATA_DIR_NAME);
        if !data_dir.exists() {
            return Err(StorageError::Internal(
                "No backup data found for this key".to_string(),
            ));
        }

        let snapshots_dir = key_dir.join(SNAPSHOTS_DIR_NAME);
        std::fs::create_dir_all(&snapshots_dir).map_err(|e| {
            StorageError::DirectoryCreation(format!("{}: {}", snapshots_dir.display(), e))
        })?;

        let timestamp = chrono::Utc::now().format("%Y-%m-%d_%H-%M-%S%.3f");
        let snapshot_filename = format!("{}.zip", timestamp);
        let snapshot_path = snapshots_dir.join(&snapshot_filename);

        let file = File::create(&snapshot_path).map_err(|e| {
            StorageError::Internal(format!("Failed to create snapshot file: {}", e))
        })?;
        let mut zip = zip::ZipWriter::new(file);
        let options: FileOptions<()> = FileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .unix_permissions(0o644);

        info!("Creating snapshot to {}", snapshot_path.display());

        // Walk the data directory and add files to zip
        let mut file_count = 0;
        for entry in WalkDir::new(&data_dir).into_iter().filter_map(|e| e.ok()) {
            let path = entry.path();
            let relative_path = path.strip_prefix(&data_dir).map_err(|e| {
                StorageError::Internal(format!("Failed to get relative path: {}", e))
            })?;

            if path.is_file() {
                let file_data = std::fs::read(path).map_err(|e| {
                    StorageError::Internal(format!("Failed to read file {}: {}", path.display(), e))
                })?;

                zip.start_file(relative_path.to_string_lossy().to_string(), options)
                    .map_err(|e| {
                        StorageError::Internal(format!("Failed to add file to zip: {}", e))
                    })?;

                zip.write_all(&file_data).map_err(|e| {
                    StorageError::Internal(format!("Failed to write file to zip: {}", e))
                })?;

                file_count += 1;
            }
        }

        // Check if any files were added
        if file_count == 0 {
            // Clean up the empty zip file
            drop(zip);
            let _ = std::fs::remove_file(&snapshot_path);
            return Err(StorageError::Internal("No files to snapshot".to_string()));
        }

        match zip.finish() {
            Ok(_) => {
                info!("Snapshot created successfully: {}", snapshot_path.display());
                Ok(snapshot_path)
            }
            Err(e) => {
                // Clean up partial zip file on error
                let _ = std::fs::remove_file(&snapshot_path);
                Err(StorageError::Internal(format!(
                    "Failed to finalize zip file: {}",
                    e
                )))
            }
        }
    }
}

/// Storage for managing multiple Pubky keys
/// Located at: ~/.pubky-backup/keys/
pub struct KeysStorage {
    keys_dir: PathBuf,
}

impl KeysStorage {
    fn new(data_dir: &Path) -> Result<Self, StorageError> {
        let keys_dir = data_dir.join(KEYS_DIR_NAME);
        // Ensure the keys directory exists
        std::fs::create_dir_all(&keys_dir).map_err(|e| {
            StorageError::DirectoryCreation(format!("{}: {}", keys_dir.display(), e))
        })?;
        Ok(KeysStorage { keys_dir })
    }

    /// Get storage for a specific key
    pub fn get_key_storage(&self, pubky: &PublicKey) -> Result<KeyStorage, StorageError> {
        KeyStorage::new(&self.keys_dir, pubky)
    }

    /// List all public keys that have backed-up data
    pub fn list_keys(&self) -> Result<Vec<String>, StorageError> {
        let mut keys = Vec::new();

        if !self.keys_dir.exists() {
            return Ok(keys);
        }

        for entry in std::fs::read_dir(&self.keys_dir)
            .map_err(|e| StorageError::Internal(format!("Failed to read keys directory: {}", e)))?
        {
            let entry = entry.map_err(|e| {
                StorageError::Internal(format!("Failed to read directory entry: {}", e))
            })?;

            let path = entry.path();
            if path.is_dir() {
                if let Some(name) = path.file_name() {
                    let name_str = name.to_string_lossy().to_string();
                    // Verify it's a valid public key
                    if PublicKey::from_str(&name_str).is_ok() {
                        keys.push(name_str);
                    }
                }
            }
        }

        Ok(keys)
    }
}

/// Application-specific storage that manages both app data and per-key backup data.
///
/// This is the main storage interface used by [`BackupController`](crate::BackupController).
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
/// ├── config/                    # App-level configuration
/// │   └── last_pubky             # Last used pubky
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
        // Migrate from old structure if needed
        migrate_old_structure(&data_dir)?;

        Ok(AppStorage {
            app_data: AppDataStorage::new(&data_dir)?,
            keys_storage: KeysStorage::new(&data_dir)?,
            data_dir,
        })
    }

    #[cfg(test)]
    pub fn new_with_path(data_dir: &PathBuf) -> Result<Self, StorageError> {
        // Migrate from old structure if needed
        migrate_old_structure(data_dir)?;

        Ok(AppStorage {
            app_data: AppDataStorage::new(data_dir)?,
            keys_storage: KeysStorage::new(data_dir)?,
            data_dir: data_dir.clone(),
        })
    }

    /// Get storage for a specific key
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

    /// Read data from backup storage using PubkyResource path
    #[cfg(test)]
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
    /// * `cursor_value` - The cursor value from the last processed event
    pub async fn write_cursor(
        &self,
        pubky: &PublicKey,
        cursor_value: String,
    ) -> Result<(), StorageError> {
        let key_storage = self.key_storage(pubky)?;
        key_storage.write_cursor(cursor_value).await
    }

    /// Read the backup progress cursor for a specific pubky.
    ///
    /// Creates an empty cursor file if one doesn't exist.
    ///
    /// # Arguments
    ///
    /// * `pubky` - The public key to read the cursor for
    ///
    /// # Returns
    ///
    /// The cursor value, or an empty string if no cursor exists yet
    pub async fn read_cursor(&self, pubky: &PublicKey) -> Result<String, StorageError> {
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

    /// Get the data directory path.
    ///
    /// # Returns
    ///
    /// Path to the root data directory
    pub fn get_backup_data_dir(&self) -> Result<PathBuf, StorageError> {
        Ok(self.data_dir.clone())
    }

    /// Write the last used pubky to storage.
    ///
    /// Used to remember which pubky was last backed up by the application.
    ///
    /// # Arguments
    ///
    /// * `pubky` - The public key to store
    pub async fn write_last_pubky(&self, pubky: &PublicKey) -> Result<(), StorageError> {
        self.app_data.write_last_pubky(pubky).await
    }

    /// Read the last used pubky from storage.
    ///
    /// # Returns
    ///
    /// The last used public key, or `None` if none has been stored
    pub async fn read_last_pubky(&self) -> Result<Option<PublicKey>, StorageError> {
        self.app_data.read_last_pubky().await
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

#[cfg(test)]
mod tests {
    use crate::DEV_MODE_PUBKY;

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
        let pubky = PublicKey::from_str(DEV_MODE_PUBKY).unwrap();

        // Write cursor
        storage
            .write_cursor(&pubky, "test_cursor_123".to_string())
            .await
            .unwrap();

        // Read cursor back
        let cursor = storage.read_cursor(&pubky).await.unwrap();
        assert_eq!(cursor, "test_cursor_123");
    }

    #[tokio::test]
    async fn test_read_cursor_creates_empty_if_not_exists() {
        let (storage, _temp_dir) = create_test_storage();
        let pubky = PublicKey::from_str(DEV_MODE_PUBKY).unwrap();

        // Read cursor before writing - should create empty cursor
        let cursor = storage.read_cursor(&pubky).await.unwrap();
        assert_eq!(cursor, "");

        // Should be able to read it again
        let cursor2 = storage.read_cursor(&pubky).await.unwrap();
        assert_eq!(cursor2, "");
    }

    #[tokio::test]
    async fn test_write_and_delete_resource() {
        let (storage, _temp_dir) = create_test_storage();
        let pubky = PublicKey::from_str(DEV_MODE_PUBKY).unwrap();

        let resource = PubkyResource::new(pubky.clone(), "/pub/test.json").unwrap();
        let test_data = b"Hello, World!".to_vec();

        // Write resource
        storage.write(&resource, test_data.clone()).await.unwrap();

        // Read resource back (using internal method)
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
        let pubky = PublicKey::from_str(DEV_MODE_PUBKY).unwrap();

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
        // Size should be approximately the sum of both data sizes (11 bytes)
        // Allowing for filesystem overhead
        assert!(size >= 11);
    }

    #[tokio::test]
    async fn test_write_and_read_last_pubky() {
        let (storage, _temp_dir) = create_test_storage();
        let pubky = PublicKey::from_str(DEV_MODE_PUBKY).unwrap();

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
        let pubky1 = PublicKey::from_str(DEV_MODE_PUBKY).unwrap();
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

        // Should now list both pubky directories
        let dirs = storage.list_pubky_directories().unwrap();
        assert_eq!(dirs.len(), 2);
        assert!(dirs.contains(&pubky1.to_string()));
        assert!(dirs.contains(&pubky2.to_string()));
    }

    #[tokio::test]
    async fn test_write_key_error_logs_appends() {
        let (storage, temp_dir) = create_test_storage();
        let pubky = PublicKey::from_str(DEV_MODE_PUBKY).unwrap();

        // Pre-create an error log with existing content (simulating a previous session)
        let error_log_dir = temp_dir
            .path()
            .join(KEYS_DIR_NAME)
            .join(pubky.to_string())
            .join(STATE_DIR_NAME);
        std::fs::create_dir_all(&error_log_dir).unwrap();
        std::fs::write(
            error_log_dir.join(ERROR_LOG_FILENAME),
            "[2024-01-01 00:00:00 UTC] /old/path: Previous session error\n",
        )
        .unwrap();

        // Write some errors to key-specific log
        storage
            .write_error(&pubky, "/pub/test1.json", "Test error 1")
            .await
            .unwrap();
        storage
            .write_error(&pubky, "/pub/test2.json", "Test error 2")
            .await
            .unwrap();

        // Read the error log file directly and verify all entries are present
        let log_content = std::fs::read_to_string(error_log_dir.join(ERROR_LOG_FILENAME)).unwrap();

        // Verify existing content from previous session is preserved
        assert!(
            log_content.contains("Previous session error"),
            "Existing error should be preserved"
        );
        // Verify new errors are appended
        assert!(
            log_content.contains("Test error 1"),
            "Log should contain first error"
        );
        assert!(
            log_content.contains("Test error 2"),
            "Log should contain second error"
        );

        // Verify the entries are in order (each on its own line)
        let lines: Vec<&str> = log_content.lines().collect();
        assert_eq!(lines.len(), 3, "Should have exactly 3 log entries");
        assert!(
            lines[0].contains("Previous session error"),
            "First line should be the old entry"
        );
        assert!(
            lines[1].contains("Test error 1"),
            "Second line should be the first new entry"
        );
        assert!(
            lines[2].contains("Test error 2"),
            "Third line should be the second new entry"
        );
    }

    #[tokio::test]
    async fn test_write_global_error_logs_appends() {
        let (storage, temp_dir) = create_test_storage();

        // Pre-create a global error log with existing content (simulating a previous session)
        let logs_dir = temp_dir.path().join(LOGS_DIR_NAME);
        std::fs::create_dir_all(&logs_dir).unwrap();
        std::fs::write(
            logs_dir.join(ERROR_LOG_FILENAME),
            "[2024-01-01 00:00:00 UTC] Failed to fetch /old/url: Previous session error\n",
        )
        .unwrap();

        // Write some errors to global log
        storage
            .write_global_error("/some/url", "Global error 1")
            .await
            .unwrap();
        storage
            .write_global_error("/another/url", "Global error 2")
            .await
            .unwrap();

        // Read the error log file directly and verify all entries are present
        let log_content = std::fs::read_to_string(logs_dir.join(ERROR_LOG_FILENAME)).unwrap();

        // Verify existing content from previous session is preserved
        assert!(
            log_content.contains("Previous session error"),
            "Existing error should be preserved"
        );
        // Verify new errors are appended
        assert!(
            log_content.contains("Global error 1"),
            "Log should contain first error"
        );
        assert!(
            log_content.contains("Global error 2"),
            "Log should contain second error"
        );

        // Verify the entries are in order
        let lines: Vec<&str> = log_content.lines().collect();
        assert_eq!(lines.len(), 3, "Should have exactly 3 log entries");
        assert!(
            lines[0].contains("Previous session error"),
            "First line should be the old entry"
        );
        assert!(
            lines[1].contains("Global error 1"),
            "Second line should be the first new entry"
        );
        assert!(
            lines[2].contains("Global error 2"),
            "Third line should be the second new entry"
        );
    }

    #[tokio::test]
    async fn test_atomic_cursor_write_no_temp_file_left_behind() {
        let (storage, temp_dir) = create_test_storage();
        let pubky = PublicKey::from_str(DEV_MODE_PUBKY).unwrap();

        // Write cursor multiple times
        storage
            .write_cursor(&pubky, "cursor_v1".to_string())
            .await
            .unwrap();
        storage
            .write_cursor(&pubky, "cursor_v2".to_string())
            .await
            .unwrap();
        storage
            .write_cursor(&pubky, "cursor_v3".to_string())
            .await
            .unwrap();

        // Verify the cursor has the latest value
        let cursor = storage.read_cursor(&pubky).await.unwrap();
        assert_eq!(cursor, "cursor_v3");

        // Verify no temp file is left behind
        let temp_file_path = temp_dir
            .path()
            .join(KEYS_DIR_NAME)
            .join(pubky.to_string())
            .join(STATE_DIR_NAME)
            .join("cursor.tmp");
        assert!(
            !temp_file_path.exists(),
            "Temporary cursor file should not exist after atomic write"
        );

        // Verify actual cursor file exists
        let cursor_file_path = temp_dir
            .path()
            .join(KEYS_DIR_NAME)
            .join(pubky.to_string())
            .join(STATE_DIR_NAME)
            .join("cursor");
        assert!(cursor_file_path.exists(), "Cursor file should exist");
    }

    #[tokio::test]
    async fn test_atomic_cursor_write_overwrites_previous() {
        let (storage, _temp_dir) = create_test_storage();
        let pubky = PublicKey::from_str(DEV_MODE_PUBKY).unwrap();

        // Write initial cursor
        storage
            .write_cursor(&pubky, "initial_cursor".to_string())
            .await
            .unwrap();

        let cursor = storage.read_cursor(&pubky).await.unwrap();
        assert_eq!(cursor, "initial_cursor");

        // Overwrite with new cursor atomically
        storage
            .write_cursor(&pubky, "updated_cursor".to_string())
            .await
            .unwrap();

        // Verify the cursor was updated atomically
        let cursor = storage.read_cursor(&pubky).await.unwrap();
        assert_eq!(cursor, "updated_cursor");

        // Write one more time to ensure multiple overwrites work
        storage
            .write_cursor(&pubky, "final_cursor".to_string())
            .await
            .unwrap();

        let cursor = storage.read_cursor(&pubky).await.unwrap();
        assert_eq!(cursor, "final_cursor");
    }

    #[tokio::test]
    async fn test_create_snapshot() {
        use std::io::Read;

        let (storage, temp_dir) = create_test_storage();
        let pubky = PublicKey::from_str(DEV_MODE_PUBKY).unwrap();

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

        // Verify snapshot is in the temp directory (not home directory)
        assert!(
            snapshot_path.starts_with(temp_dir.path()),
            "Snapshot should be in temp directory, not home directory. Path: {}",
            snapshot_path.display()
        );

        // Verify it's in the key's snapshots directory
        let expected_snapshots_dir = temp_dir
            .path()
            .join(KEYS_DIR_NAME)
            .join(pubky.to_string())
            .join(SNAPSHOTS_DIR_NAME);
        assert!(
            snapshot_path.starts_with(&expected_snapshots_dir),
            "Snapshot should be in key's snapshots directory"
        );

        // Verify filename format (just timestamp now)
        let filename = snapshot_path.file_name().unwrap().to_str().unwrap();
        assert!(
            filename.ends_with(".zip"),
            "Filename should end with '.zip'"
        );
        // Should be in format YYYY-MM-DD_HH-MM-SS.zip
        assert!(
            filename.len() > 4, // At least longer than ".zip"
            "Filename should have timestamp"
        );

        // Verify the snapshot is a valid zip file and contains correct files
        let file = std::fs::File::open(&snapshot_path).unwrap();
        let mut zip_archive = zip::ZipArchive::new(file).unwrap();

        // Collect file names from archive
        let file_names: Vec<String> = (0..zip_archive.len())
            .map(|i| zip_archive.by_index(i).unwrap().name().to_string())
            .collect();

        // Verify expected files are in the archive
        assert!(
            file_names.iter().any(|n| n.contains("test1.json")),
            "Archive should contain test1.json. Found: {:?}",
            file_names
        );
        assert!(
            file_names.iter().any(|n| n.contains("test2.json")),
            "Archive should contain test2.json. Found: {:?}",
            file_names
        );

        // Verify file contents
        for i in 0..zip_archive.len() {
            let mut file = zip_archive.by_index(i).unwrap();
            let name = file.name().to_string();
            let mut contents = Vec::new();
            file.read_to_end(&mut contents).unwrap();

            if name.contains("test1.json") {
                assert_eq!(
                    contents, b"content_one",
                    "test1.json should have correct content"
                );
            } else if name.contains("test2.json") {
                assert_eq!(
                    contents, b"content_two",
                    "test2.json should have correct content"
                );
            }
        }

        // Clean up
        std::fs::remove_file(&snapshot_path).unwrap();
    }

    #[tokio::test]
    async fn test_create_snapshot_no_data() {
        let (storage, _temp_dir) = create_test_storage();
        let pubky =
            PublicKey::from_str("o4dksfbqk85ogzdb5osziw6befigbuxmuxkuxq8434q89uj56uxo").unwrap();

        // Try to create snapshot for pubky with no data written
        // Note: key_storage() creates directories eagerly, so this results in
        // "No files to snapshot" rather than "No backup data found"
        let result = storage.create_snapshot(&pubky).await;

        assert!(result.is_err(), "Should fail when no backup data exists");
        let error_msg = result.unwrap_err().to_string();
        assert!(
            error_msg.contains("No files to snapshot"),
            "Error should mention no files to snapshot, got: {}",
            error_msg
        );
    }

    #[tokio::test]
    async fn test_create_snapshot_empty_directory() {
        let (storage, temp_dir) = create_test_storage();
        let pubky = PublicKey::from_str(DEV_MODE_PUBKY).unwrap();

        // Create the pubky's data directory but don't add any files
        let data_dir = temp_dir
            .path()
            .join(KEYS_DIR_NAME)
            .join(pubky.to_string())
            .join(DATA_DIR_NAME);
        std::fs::create_dir_all(&data_dir).unwrap();

        // Try to create snapshot for empty pubky directory
        let result = storage.create_snapshot(&pubky).await;

        assert!(result.is_err(), "Should fail when pubky directory is empty");
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("No files to snapshot"),
            "Error should mention no files to snapshot"
        );

        // Verify no orphaned zip file was left behind
        let snapshots_dir = temp_dir
            .path()
            .join(KEYS_DIR_NAME)
            .join(pubky.to_string())
            .join(SNAPSHOTS_DIR_NAME);
        if snapshots_dir.exists() {
            let entries: Vec<_> = std::fs::read_dir(&snapshots_dir).unwrap().collect();
            assert!(
                entries.is_empty(),
                "No snapshot file should be left behind for empty directory"
            );
        }
    }

    #[tokio::test]
    async fn test_storage_directory_structure() {
        let (storage, temp_dir) = create_test_storage();
        let pubky = PublicKey::from_str(DEV_MODE_PUBKY).unwrap();

        // Write some data to trigger directory creation
        let resource = PubkyResource::new(pubky.clone(), "/pub/test.json").unwrap();
        storage.write(&resource, b"test".to_vec()).await.unwrap();

        // Write last pubky to trigger config directory creation
        storage.write_last_pubky(&pubky).await.unwrap();

        // Verify directory structure
        let root = temp_dir.path();

        // App-level directories
        assert!(root.join(CONFIG_DIR_NAME).exists(), "config/ should exist");
        assert!(
            root.join(CONFIG_DIR_NAME)
                .join(LAST_PUBKY_FILENAME)
                .exists(),
            "config/last_pubky should exist"
        );
        assert!(root.join(LOGS_DIR_NAME).exists(), "logs/ should exist");

        // Per-key directories
        let key_dir = root.join(KEYS_DIR_NAME).join(pubky.to_string());
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
}
