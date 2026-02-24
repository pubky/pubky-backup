//! Per-key storage implementations.
//!
//! This module provides storage abstractions for individual pubky keys
//! and for managing multiple keys.

use super::error::{OperationFailedError, StorageError};
use futures_lite::StreamExt;
use log::{debug, error, info, warn};
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

// Directory and file names
pub(crate) const STATE_DIR_NAME: &str = "state";
pub(crate) const DATA_DIR_NAME: &str = "data";
const SNAPSHOTS_DIR_NAME: &str = "snapshots";
pub(crate) const CURSOR_FILENAME: &str = "cursor";
pub(crate) const ERROR_LOG_FILENAME: &str = "error.log";
pub(crate) const KEYS_DIR_NAME: &str = "keys";

/// Simple storage abstraction for reading/writing/deleting files.
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

    /// Append data to a file, creating it if it doesn't exist.
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

/// Storage for a single Pubky key's backup data.
///
/// Located at: `~/.pubky-backup/keys/<pubky>/`
///
/// Structure:
/// - `state/cursor` - Sync progress cursor
/// - `state/error.log` - Key-specific error log
/// - `data/pub/...` - Backed-up resources
/// - `snapshots/<timestamp>.zip` - Point-in-time snapshots
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
    pub(crate) fn new(keys_dir: &Path, pubky: &PublicKey) -> Result<Self, StorageError> {
        let key_dir = keys_dir.join(pubky.z32());
        let state_dir = key_dir.join(STATE_DIR_NAME);
        let data_dir = key_dir.join(DATA_DIR_NAME);

        Ok(KeyStorage {
            pubky: pubky.clone(),
            state_storage: Storage::new(&state_dir)?,
            data_storage: Storage::new(&data_dir)?,
            key_dir,
        })
    }

    /// Write cursor to track backup progress.
    ///
    /// Uses atomic write-then-rename to prevent torn writes on crashes.
    pub async fn write_cursor(&self, cursor_value: u64) -> Result<(), StorageError> {
        let temp_path = format!("{}.tmp", CURSOR_FILENAME);
        self.state_storage
            .write(&temp_path, cursor_value.to_string())
            .await?;
        self.state_storage
            .rename(&temp_path, CURSOR_FILENAME)
            .await?;
        debug!("Cursor value written: {}", cursor_value);
        Ok(())
    }

    /// Read existing cursor.
    ///
    /// Handles migration from old string-stored cursors by parsing the string as u64.
    /// The cursor value from the API was always u64, just previously stored as String.
    pub async fn read_cursor(&self) -> Result<Option<u64>, StorageError> {
        match self.state_storage.read(CURSOR_FILENAME).await {
            Ok(cursor_data) => {
                let cursor_string = String::from_utf8(cursor_data)?;
                let cursor_string = cursor_string.trim();
                if cursor_string.is_empty() {
                    Ok(None)
                } else {
                    match cursor_string.parse::<u64>() {
                        Ok(cursor) => Ok(Some(cursor)),
                        Err(_) => {
                            // Cursor value is not a valid u64 - this shouldn't happen with real
                            // homeserver data, but could occur with old mock/test cursors.
                            // Reset to None and delete the invalid cursor file.
                            warn!(
                                "Invalid cursor value '{}' cannot be parsed as u64, resetting cursor",
                                cursor_string
                            );
                            let _ = self.state_storage.delete(CURSOR_FILENAME).await;
                            Ok(None)
                        }
                    }
                }
            }
            Err(_) => {
                info!("Cursor file not found");
                Ok(None)
            }
        }
    }

    /// Write error to key-specific error log.
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

    /// Write data to backup storage using PubkyResource path.
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

    /// Delete data from backup storage using PubkyResource path.
    pub async fn delete_data(&self, resource: &PubkyResource) -> Result<(), StorageError> {
        let file_path = resource.path.as_str();
        self.data_storage.delete(file_path).await?;
        Ok(())
    }

    /// Read data from backup storage using PubkyResource path.
    ///
    /// Returns the raw bytes stored for the given resource.
    pub async fn read_data(&self, resource: &PubkyResource) -> Result<Vec<u8>, StorageError> {
        let file_path = resource.path.as_str();
        self.data_storage.read(file_path).await
    }

    /// Calculate the total size of backed-up data for this key.
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

    /// Recursively calculate directory size.
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
                return Ok(0);
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

    /// Create a snapshot (zip archive) of the backed-up data.
    pub async fn create_snapshot(&self) -> Result<PathBuf, StorageError> {
        let key_dir = self.key_dir.clone();
        tokio::task::spawn_blocking(move || Self::create_snapshot_blocking(&key_dir))
            .await
            .map_err(|e| StorageError::Internal(format!("Snapshot task failed: {}", e)))?
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

/// Storage for managing multiple Pubky keys.
///
/// Located at: `~/.pubky-backup/keys/`
pub struct KeysStorage {
    keys_dir: PathBuf,
}

impl KeysStorage {
    pub(crate) fn new(data_dir: &Path) -> Result<Self, StorageError> {
        let keys_dir = data_dir.join(KEYS_DIR_NAME);
        // Ensure the keys directory exists
        std::fs::create_dir_all(&keys_dir).map_err(|e| {
            StorageError::DirectoryCreation(format!("{}: {}", keys_dir.display(), e))
        })?;
        Ok(KeysStorage { keys_dir })
    }

    /// Create a KeysStorage with a specific keys directory path.
    /// Used for testing.
    #[cfg(test)]
    pub fn new_with_path(keys_dir: &Path) -> Result<Self, StorageError> {
        Ok(KeysStorage {
            keys_dir: keys_dir.to_path_buf(),
        })
    }

    /// Get storage for a specific key.
    pub fn get_key_storage(&self, pubky: &PublicKey) -> Result<KeyStorage, StorageError> {
        KeyStorage::new(&self.keys_dir, pubky)
    }

    /// List all public keys that have backed-up data.
    ///
    /// Returns keys in normalized z32 format (no prefix).
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
                    if let Ok(pubky) = PublicKey::from_str(&name_str) {
                        keys.push(pubky.z32());
                    }
                }
            }
        }

        Ok(keys)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DEV_MODE_PUBKY;
    use tempfile::TempDir;

    fn create_test_key_storage() -> (KeyStorage, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let pubky = PublicKey::from_str(DEV_MODE_PUBKY).unwrap();
        let storage = KeyStorage::new(temp_dir.path(), &pubky).unwrap();
        (storage, temp_dir)
    }

    #[tokio::test]
    async fn test_write_and_read_cursor() {
        let (storage, _temp_dir) = create_test_key_storage();

        // Write cursor
        storage.write_cursor(123).await.unwrap();

        // Read cursor back
        let cursor = storage.read_cursor().await.unwrap();
        assert_eq!(cursor, Some(123));
    }

    #[tokio::test]
    async fn test_read_cursor_returns_none_if_not_exists() {
        let (storage, _temp_dir) = create_test_key_storage();

        // Read cursor before writing - should return None
        let cursor = storage.read_cursor().await.unwrap();
        assert_eq!(cursor, None);
    }

    #[tokio::test]
    async fn test_atomic_cursor_write() {
        let (storage, temp_dir) = create_test_key_storage();
        let pubky = PublicKey::from_str(DEV_MODE_PUBKY).unwrap();

        // Write cursor multiple times
        storage.write_cursor(1).await.unwrap();
        storage.write_cursor(2).await.unwrap();
        storage.write_cursor(3).await.unwrap();

        // Verify the cursor has the latest value
        let cursor = storage.read_cursor().await.unwrap();
        assert_eq!(cursor, Some(3));

        // Verify no temp file is left behind
        let temp_file_path = temp_dir
            .path()
            .join(pubky.z32())
            .join(STATE_DIR_NAME)
            .join("cursor.tmp");
        assert!(
            !temp_file_path.exists(),
            "Temporary cursor file should not exist after atomic write"
        );
    }

    #[tokio::test]
    async fn test_write_and_delete_data() {
        let (storage, _temp_dir) = create_test_key_storage();
        let pubky = PublicKey::from_str(DEV_MODE_PUBKY).unwrap();

        let resource = PubkyResource::new(pubky.clone(), "/pub/test.json").unwrap();
        let test_data = b"Hello, World!".to_vec();

        // Write data
        storage
            .write_data(&resource, test_data.clone())
            .await
            .unwrap();

        // Read data back
        let read_data = storage.read_data(&resource).await.unwrap();
        assert_eq!(read_data, test_data);

        // Delete data
        storage.delete_data(&resource).await.unwrap();

        // Reading deleted data should fail
        assert!(storage.read_data(&resource).await.is_err());
    }

    #[tokio::test]
    async fn test_calculate_data_size() {
        let (storage, _temp_dir) = create_test_key_storage();
        let pubky = PublicKey::from_str(DEV_MODE_PUBKY).unwrap();

        // Initial size should be 0
        let size = storage.calculate_data_size().await;
        assert_eq!(size, 0);

        // Write some data
        let resource1 = PubkyResource::new(pubky.clone(), "/pub/test1.json").unwrap();
        let resource2 = PubkyResource::new(pubky.clone(), "/pub/test2.json").unwrap();

        storage
            .write_data(&resource1, b"data1".to_vec())
            .await
            .unwrap();
        storage
            .write_data(&resource2, b"data22".to_vec())
            .await
            .unwrap();

        // Size should be sum of both files
        let size = storage.calculate_data_size().await;
        assert!(size > 0);
        assert!(size >= 11); // At least 11 bytes
    }

    #[tokio::test]
    async fn test_keys_storage_list_keys() {
        let temp_dir = TempDir::new().unwrap();
        let keys_storage = KeysStorage::new(temp_dir.path()).unwrap();

        // Initially should be empty
        let keys = keys_storage.list_keys().unwrap();
        assert!(keys.is_empty());

        // Create a key directory
        let pubky = PublicKey::from_str(DEV_MODE_PUBKY).unwrap();
        let _key_storage = keys_storage.get_key_storage(&pubky).unwrap();

        // Now should list the key
        let keys = keys_storage.list_keys().unwrap();
        assert_eq!(keys.len(), 1);
        assert!(keys.contains(&pubky.z32()));
    }
}
