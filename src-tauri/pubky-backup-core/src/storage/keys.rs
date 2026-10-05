//! Per-key storage implementations.
//!
//! This module provides storage abstractions for individual pubky keys
//! and for managing multiple keys.

use super::common::Storage;
use super::error::StorageError;
use crate::orchestrator::types::ActivityEntry;
use futures_lite::StreamExt;
use log::{debug, error, info, warn};
use pubky::{PubkyResource, PublicKey};
use std::{
    fs::File,
    io::{Read as _, Seek, SeekFrom, Write},
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
const PRIVATE_CURSOR_FILENAME: &str = "private_cursor";
const SESSION_FILENAME: &str = "session";
const ACTIVITY_LOG_FILENAME: &str = "activity.log";
const INACTIVE_FILENAME: &str = "inactive";
/// Storage for a single Pubky key's backup data.
///
/// Located at: `~/.pubky-backup/keys/<pubky>/`
///
/// Structure:
/// - `state/cursor` - Sync progress cursor for public data
/// - `state/private_cursor` - Sync progress cursor for private data
/// - `state/session` - Session secret of a signed-in key (owner-readable only)
/// - `state/activity.log` - Activity log (JSON lines)
/// - `data/pub/...` - Backed-up public resources
/// - `data/priv/...` - Backed-up private resources (signed-in keys only)
/// - `snapshots/<timestamp>.zip` - Point-in-time snapshots
pub struct KeyStorage {
    /// The public key this storage is for
    pubky: PublicKey,
    /// Storage for state files (cursor, activity log)
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

    /// Write cursor to track backup progress of public data.
    ///
    /// Uses atomic write-then-rename to prevent torn writes on crashes.
    pub async fn write_cursor(&self, cursor_value: u64) -> Result<(), StorageError> {
        self.write_cursor_file(CURSOR_FILENAME, cursor_value).await
    }

    /// Read existing cursor for public data.
    ///
    /// Handles migration from old string-stored cursors by parsing the string as u64.
    /// The cursor value from the API was always u64, just previously stored as String.
    pub async fn read_cursor(&self) -> Result<Option<u64>, StorageError> {
        self.read_cursor_file(CURSOR_FILENAME).await
    }

    /// Write cursor to track backup progress of private data.
    ///
    /// Private data is synced separately from public data, so it has its own cursor.
    pub async fn write_private_cursor(&self, cursor_value: u64) -> Result<(), StorageError> {
        self.write_cursor_file(PRIVATE_CURSOR_FILENAME, cursor_value)
            .await
    }

    /// Read existing cursor for private data.
    pub async fn read_private_cursor(&self) -> Result<Option<u64>, StorageError> {
        self.read_cursor_file(PRIVATE_CURSOR_FILENAME).await
    }

    async fn write_cursor_file(
        &self,
        filename: &str,
        cursor_value: u64,
    ) -> Result<(), StorageError> {
        let temp_path = format!("{}.tmp", filename);
        self.state_storage
            .write(&temp_path, cursor_value.to_string())
            .await?;
        self.state_storage.rename(&temp_path, filename).await?;
        debug!("Cursor value written to {}: {}", filename, cursor_value);
        Ok(())
    }

    async fn read_cursor_file(&self, filename: &str) -> Result<Option<u64>, StorageError> {
        match self.state_storage.read(filename).await {
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
                            let _ = self.state_storage.delete(filename).await;
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

    fn session_path(&self) -> PathBuf {
        self.key_dir.join(STATE_DIR_NAME).join(SESSION_FILENAME)
    }

    /// Store the session secret of a signed-in key.
    ///
    /// The secret gives access to the key's data on its homeserver, so the file
    /// is readable by its owner only (on Unix). Written atomically.
    pub fn write_session_secret(&self, secret: &str) -> Result<(), StorageError> {
        let path = self.session_path();
        let temp_path = path.with_extension("tmp");
        write_owner_only(&temp_path, secret)
            .and_then(|_| std::fs::rename(&temp_path, &path))
            .map_err(|e| {
                let _ = std::fs::remove_file(&temp_path);
                StorageError::Internal(format!("Failed to write session secret: {}", e))
            })
    }

    /// Read the stored session secret, or `None` if the key is not signed in.
    pub fn read_session_secret(&self) -> Result<Option<String>, StorageError> {
        match std::fs::read_to_string(self.session_path()) {
            Ok(secret) => Ok(Some(secret)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(StorageError::Internal(format!(
                "Failed to read session secret: {}",
                e
            ))),
        }
    }

    /// Remove the stored session secret and return it, or `None` if the key is
    /// not signed in.
    pub fn take_session_secret(&self) -> Result<Option<String>, StorageError> {
        let secret = self.read_session_secret()?;
        self.delete_session_secret()?;
        Ok(secret)
    }

    /// Delete the stored session secret. Does nothing if there is none.
    pub fn delete_session_secret(&self) -> Result<(), StorageError> {
        match std::fs::remove_file(self.session_path()) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(StorageError::Internal(format!(
                "Failed to delete session secret: {}",
                e
            ))),
        }
    }

    /// Mark this key as inactive (removed from syncing but data preserved).
    pub async fn mark_inactive(&self) -> Result<(), StorageError> {
        self.state_storage.write(INACTIVE_FILENAME, "").await
    }

    /// Check whether this key is marked as inactive.
    pub fn is_inactive(&self) -> bool {
        self.key_dir
            .join(STATE_DIR_NAME)
            .join(INACTIVE_FILENAME)
            .exists()
    }

    /// Remove the inactive marker, re-activating this key.
    pub async fn clear_inactive(&self) -> Result<(), StorageError> {
        if self.is_inactive() {
            self.state_storage.delete(INACTIVE_FILENAME).await?;
        }
        Ok(())
    }

    /// Append an activity entry to the key's activity log.
    pub async fn write_activity(&self, entry: &ActivityEntry) -> Result<(), StorageError> {
        let mut line =
            serde_json::to_string(entry).map_err(|e| StorageError::Internal(e.to_string()))?;
        line.push('\n');
        self.state_storage
            .append(ACTIVITY_LOG_FILENAME, line)
            .await?;
        Ok(())
    }

    /// Read activity entries, newest first, up to `limit`.
    ///
    /// Reads from the end of the file to avoid loading the entire log into memory.
    pub async fn read_activity(&self, limit: usize) -> Vec<ActivityEntry> {
        let path = self
            .key_dir
            .join(STATE_DIR_NAME)
            .join(ACTIVITY_LOG_FILENAME);
        tokio::task::spawn_blocking(move || Self::read_activity_blocking(&path, limit))
            .await
            .unwrap_or_default()
    }

    /// Read the last `limit` lines from the activity log, newest first.
    fn read_activity_blocking(path: &Path, limit: usize) -> Vec<ActivityEntry> {
        let mut file = match File::open(path) {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Vec::new(),
            Err(e) => {
                log::warn!("Failed to open activity log {}: {}", path.display(), e);
                return Vec::new();
            }
        };

        let file_len = match file.seek(SeekFrom::End(0)) {
            Ok(len) => len,
            Err(e) => {
                log::warn!("Failed to seek activity log {}: {}", path.display(), e);
                return Vec::new();
            }
        };
        if file_len == 0 {
            return Vec::new();
        }

        // Read chunks from the end, growing if we don't find enough entries.
        // Start with 512 bytes per entry; double if insufficient.
        let mut chunk_size = (limit as u64 * 512).min(file_len);
        loop {
            let start = file_len - chunk_size;
            if file.seek(SeekFrom::Start(start)).is_err() {
                return Vec::new();
            }

            let mut buf = vec![0u8; chunk_size as usize];
            if file.read_exact(&mut buf).is_err() {
                return Vec::new();
            }

            let text = String::from_utf8_lossy(&buf);
            // If we started mid-line (start > 0), the first partial line
            // is correctly skipped by filter_map since it won't parse as JSON.
            let entries: Vec<ActivityEntry> = text
                .lines()
                .rev()
                .filter_map(|line| serde_json::from_str(line).ok())
                .take(limit)
                .collect();

            // If we got enough entries or already read the whole file, return.
            if entries.len() >= limit || chunk_size >= file_len {
                return entries;
            }

            // Double the chunk and retry.
            chunk_size = (chunk_size * 2).min(file_len);
        }
    }

    /// Count the number of snapshot zip files for this key.
    pub async fn count_snapshots(&self) -> usize {
        let snapshots_dir = self.key_dir.join(SNAPSHOTS_DIR_NAME);
        tokio::task::spawn_blocking(move || match std::fs::read_dir(&snapshots_dir) {
            Ok(entries) => entries
                .filter_map(|e| e.ok())
                .filter(|e| e.path().extension().is_some_and(|ext| ext == "zip"))
                .count(),
            Err(_) => 0,
        })
        .await
        .unwrap_or(0)
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

/// Write a file that only its owner can read (mode 0600 on Unix).
fn write_owner_only(path: &Path, contents: &str) -> std::io::Result<()> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)?.write_all(contents.as_bytes())
}

/// Storage for managing multiple Pubky keys.
///
/// Located at: `~/.pubky-backup/keys/`
pub struct KeysStorage {
    keys_dir: PathBuf,
}

impl KeysStorage {
    pub fn new_with_keys_dir(keys_dir: &Path) -> Result<Self, StorageError> {
        std::fs::create_dir_all(keys_dir).map_err(|e| {
            StorageError::DirectoryCreation(format!("{}: {}", keys_dir.display(), e))
        })?;
        Ok(KeysStorage {
            keys_dir: keys_dir.to_path_buf(),
        })
    }

    /// Get the keys directory path.
    pub fn keys_dir(&self) -> &Path {
        &self.keys_dir
    }

    /// Get storage for a specific key.
    pub fn get_key_storage(&self, pubky: &PublicKey) -> Result<KeyStorage, StorageError> {
        KeyStorage::new(&self.keys_dir, pubky)
    }

    /// List all active public keys that have backed-up data.
    ///
    /// Returns keys in normalized z32 format (no prefix).
    /// Keys marked as inactive are excluded.
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
                        // Skip keys marked as inactive
                        let inactive_marker = path.join(STATE_DIR_NAME).join(INACTIVE_FILENAME);
                        if !inactive_marker.exists() {
                            keys.push(pubky.z32());
                        }
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
    use crate::TEST_PUBKY;
    use tempfile::TempDir;

    fn create_test_key_storage() -> (KeyStorage, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let pubky = PublicKey::from_str(TEST_PUBKY).unwrap();
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
        let pubky = PublicKey::from_str(TEST_PUBKY).unwrap();

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
        let pubky = PublicKey::from_str(TEST_PUBKY).unwrap();

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
        let pubky = PublicKey::from_str(TEST_PUBKY).unwrap();

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
        let keys_storage = KeysStorage::new_with_keys_dir(temp_dir.path()).unwrap();

        // Initially should be empty
        let keys = keys_storage.list_keys().unwrap();
        assert!(keys.is_empty());

        // Create a key directory
        let pubky = PublicKey::from_str(TEST_PUBKY).unwrap();
        let _key_storage = keys_storage.get_key_storage(&pubky).unwrap();

        // Now should list the key
        let keys = keys_storage.list_keys().unwrap();
        assert_eq!(keys.len(), 1);
        assert!(keys.contains(&pubky.z32()));
    }

    #[tokio::test]
    async fn test_inactive_marker() {
        let (storage, _temp_dir) = create_test_key_storage();

        // Initially active
        assert!(!storage.is_inactive());

        // Mark inactive
        storage.mark_inactive().await.unwrap();
        assert!(storage.is_inactive());

        // Clear inactive
        storage.clear_inactive().await.unwrap();
        assert!(!storage.is_inactive());

        // Clearing when already active is a no-op
        storage.clear_inactive().await.unwrap();
        assert!(!storage.is_inactive());
    }

    #[tokio::test]
    async fn test_list_keys_excludes_inactive() {
        let temp_dir = TempDir::new().unwrap();
        let keys_storage = KeysStorage::new_with_keys_dir(temp_dir.path()).unwrap();

        let pubky = PublicKey::from_str(TEST_PUBKY).unwrap();
        let key_storage = keys_storage.get_key_storage(&pubky).unwrap();

        // Key should be listed
        let keys = keys_storage.list_keys().unwrap();
        assert_eq!(keys.len(), 1);

        // Mark inactive — key should be excluded
        key_storage.mark_inactive().await.unwrap();
        let keys = keys_storage.list_keys().unwrap();
        assert!(keys.is_empty());

        // Clear inactive — key should be listed again
        key_storage.clear_inactive().await.unwrap();
        let keys = keys_storage.list_keys().unwrap();
        assert_eq!(keys.len(), 1);
    }

    #[tokio::test]
    async fn test_write_and_read_activity() {
        let (storage, _temp_dir) = create_test_key_storage();
        use crate::orchestrator::types::{ActivityEntry, ActivityType};

        let entry = ActivityEntry {
            activity_type: ActivityType::InitialBackup,
            message: "Initial backup successful".to_string(),
            timestamp: 1000,
        };
        storage.write_activity(&entry).await.unwrap();

        let entries = storage.read_activity(10).await;
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].activity_type, ActivityType::InitialBackup);
        assert_eq!(entries[0].message, "Initial backup successful");
        assert_eq!(entries[0].timestamp, 1000);
    }

    #[tokio::test]
    async fn test_read_activity_returns_newest_first() {
        let (storage, _temp_dir) = create_test_key_storage();
        use crate::orchestrator::types::{ActivityEntry, ActivityType};

        for i in 1..=5 {
            storage
                .write_activity(&ActivityEntry {
                    activity_type: ActivityType::FilesBackedUp,
                    message: format!("Entry {}", i),
                    timestamp: i * 100,
                })
                .await
                .unwrap();
        }

        let entries = storage.read_activity(10).await;
        assert_eq!(entries.len(), 5);
        // Newest (highest timestamp) first
        assert_eq!(entries[0].timestamp, 500);
        assert_eq!(entries[1].timestamp, 400);
        assert_eq!(entries[4].timestamp, 100);
    }

    #[tokio::test]
    async fn test_read_activity_respects_limit() {
        let (storage, _temp_dir) = create_test_key_storage();
        use crate::orchestrator::types::{ActivityEntry, ActivityType};

        for i in 1..=10 {
            storage
                .write_activity(&ActivityEntry {
                    activity_type: ActivityType::FilesBackedUp,
                    message: format!("Entry {}", i),
                    timestamp: i * 100,
                })
                .await
                .unwrap();
        }

        let entries = storage.read_activity(3).await;
        assert_eq!(entries.len(), 3);
        // Should be the 3 newest
        assert_eq!(entries[0].timestamp, 1000);
        assert_eq!(entries[1].timestamp, 900);
        assert_eq!(entries[2].timestamp, 800);
    }

    #[tokio::test]
    async fn test_read_activity_empty_file() {
        let (storage, _temp_dir) = create_test_key_storage();

        let entries = storage.read_activity(10).await;
        assert!(entries.is_empty());
    }

    #[tokio::test]
    async fn test_read_activity_skips_corrupt_lines() {
        let (storage, _temp_dir) = create_test_key_storage();
        use crate::orchestrator::types::{ActivityEntry, ActivityType};

        // Write a valid entry
        storage
            .write_activity(&ActivityEntry {
                activity_type: ActivityType::InitialBackup,
                message: "Good entry".to_string(),
                timestamp: 1000,
            })
            .await
            .unwrap();

        // Write a corrupt line directly to the file
        let path = storage
            .key_dir
            .join(STATE_DIR_NAME)
            .join(ACTIVITY_LOG_FILENAME);
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap();
        file.write_all(b"not valid json\n").unwrap();

        // Write another valid entry
        storage
            .write_activity(&ActivityEntry {
                activity_type: ActivityType::SyncFailed,
                message: "Also good".to_string(),
                timestamp: 2000,
            })
            .await
            .unwrap();

        let entries = storage.read_activity(10).await;
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].timestamp, 2000);
        assert_eq!(entries[1].timestamp, 1000);
    }

    #[tokio::test]
    async fn test_count_snapshots_none() {
        let (storage, _temp_dir) = create_test_key_storage();

        assert_eq!(storage.count_snapshots().await, 0);
    }

    #[tokio::test]
    async fn test_count_snapshots_with_zips() {
        let (storage, _temp_dir) = create_test_key_storage();
        let pubky = PublicKey::from_str(TEST_PUBKY).unwrap();

        // Write data so snapshot creation works
        let resource = PubkyResource::new(pubky.clone(), "/pub/test.json").unwrap();
        storage
            .write_data(&resource, b"data".to_vec())
            .await
            .unwrap();

        // Create two snapshots
        storage.create_snapshot().await.unwrap();
        // Small delay to avoid timestamp collision
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        storage.create_snapshot().await.unwrap();

        assert_eq!(storage.count_snapshots().await, 2);
    }

    #[tokio::test]
    async fn test_count_snapshots_ignores_non_zip_files() {
        let (storage, _temp_dir) = create_test_key_storage();

        // Create snapshots dir with a non-zip file
        let snapshots_dir = storage.key_dir.join(SNAPSHOTS_DIR_NAME);
        std::fs::create_dir_all(&snapshots_dir).unwrap();
        std::fs::write(snapshots_dir.join("notes.txt"), "not a zip").unwrap();

        assert_eq!(storage.count_snapshots().await, 0);
    }

    #[tokio::test]
    async fn test_activity_all_types_roundtrip() {
        let (storage, _temp_dir) = create_test_key_storage();
        use crate::orchestrator::types::{ActivityEntry, ActivityType};

        let types = [
            ActivityType::FilesBackedUp,
            ActivityType::InitialBackup,
            ActivityType::SnapshotCreated,
            ActivityType::SyncFailed,
            ActivityType::SignedIn,
            ActivityType::SignedOut,
            ActivityType::SessionExpired,
        ];

        for (i, activity_type) in types.iter().enumerate() {
            storage
                .write_activity(&ActivityEntry {
                    activity_type: activity_type.clone(),
                    message: format!("msg {}", i),
                    timestamp: (i as u64) * 100,
                })
                .await
                .unwrap();
        }

        let entries = storage.read_activity(10).await;
        assert_eq!(entries.len(), 7);
        assert_eq!(entries[0].activity_type, ActivityType::SessionExpired);
        assert_eq!(entries[1].activity_type, ActivityType::SignedOut);
        assert_eq!(entries[2].activity_type, ActivityType::SignedIn);
        assert_eq!(entries[3].activity_type, ActivityType::SyncFailed);
        assert_eq!(entries[4].activity_type, ActivityType::SnapshotCreated);
        assert_eq!(entries[5].activity_type, ActivityType::InitialBackup);
        assert_eq!(entries[6].activity_type, ActivityType::FilesBackedUp);
    }

    #[tokio::test]
    async fn test_private_cursor_is_independent_of_public_cursor() {
        let (storage, _temp_dir) = create_test_key_storage();

        assert_eq!(storage.read_private_cursor().await.unwrap(), None);

        storage.write_cursor(10).await.unwrap();
        storage.write_private_cursor(20).await.unwrap();

        assert_eq!(storage.read_cursor().await.unwrap(), Some(10));
        assert_eq!(storage.read_private_cursor().await.unwrap(), Some(20));
    }

    #[test]
    fn test_session_secret_roundtrip() {
        let (storage, _temp_dir) = create_test_key_storage();

        assert_eq!(storage.read_session_secret().unwrap(), None);

        storage.write_session_secret("pubky:secret").unwrap();
        assert_eq!(
            storage.read_session_secret().unwrap(),
            Some("pubky:secret".to_string())
        );

        // Writing again replaces the previous secret
        storage.write_session_secret("pubky:newer").unwrap();
        assert_eq!(
            storage.read_session_secret().unwrap(),
            Some("pubky:newer".to_string())
        );
    }

    #[test]
    fn test_take_session_secret() {
        let (storage, _temp_dir) = create_test_key_storage();
        storage.write_session_secret("pubky:secret").unwrap();

        assert_eq!(
            storage.take_session_secret().unwrap(),
            Some("pubky:secret".to_string())
        );
        assert_eq!(storage.read_session_secret().unwrap(), None);
        // Taking when there is no secret is not an error
        assert_eq!(storage.take_session_secret().unwrap(), None);
    }

    #[test]
    fn test_delete_session_secret() {
        let (storage, _temp_dir) = create_test_key_storage();

        storage.write_session_secret("pubky:secret").unwrap();
        storage.delete_session_secret().unwrap();
        assert_eq!(storage.read_session_secret().unwrap(), None);

        // Deleting when there is no secret is not an error
        storage.delete_session_secret().unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn test_session_secret_is_readable_by_owner_only() {
        use std::os::unix::fs::PermissionsExt;

        let (storage, _temp_dir) = create_test_key_storage();
        storage.write_session_secret("pubky:secret").unwrap();

        let mode = std::fs::metadata(storage.session_path())
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    #[tokio::test]
    async fn test_snapshot_excludes_session_secret() {
        let (storage, _temp_dir) = create_test_key_storage();
        let pubky = PublicKey::from_str(TEST_PUBKY).unwrap();
        let resource = PubkyResource::new(pubky, "/pub/file.txt").unwrap();
        storage
            .write_data(&resource, b"data".to_vec())
            .await
            .unwrap();
        storage.write_session_secret("pubky:secret").unwrap();

        let snapshot_path = storage.create_snapshot().await.unwrap();

        let mut archive = zip::ZipArchive::new(File::open(snapshot_path).unwrap()).unwrap();
        let names: Vec<String> = (0..archive.len())
            .map(|i| archive.by_index(i).unwrap().name().to_string())
            .collect();
        assert!(names.iter().any(|name| name.ends_with("file.txt")));
        assert!(!names.iter().any(|name| name.contains(SESSION_FILENAME)));
    }
}
