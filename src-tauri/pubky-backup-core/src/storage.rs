use crate::error::{OperationFailedError, StorageError};
use futures_lite::StreamExt;
use log::{debug, error, info};
use opendal::{services::Fs, Operator};
use pubky::{PubkyResource, PublicKey};
use std::{fs::File, io::Write, path::{Path, PathBuf}, str::FromStr};
use walkdir::WalkDir;
use zip::write::FileOptions;

const APP_DATA_DIR_NAME: &str = ".pubky-backup";
const CURSOR_FILENAME: &str = "cursor";
const ERROR_LOG_FILNAME: &str = "error.log";
const LAST_PUBKY_FILENAME: &str = "last_pubky";
const SNAPSHOTS_DIR_NAME: &str = "snapshots";

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
    fn new(data_dir: &PathBuf) -> Result<Self, StorageError> {
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

    async fn list(&self, path: &str) -> Result<Vec<opendal::Entry>, StorageError> {
        Ok(self.operator.list(path).await?)
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
}

/// Storage for application data (error logs, last_pubky)
/// Will be expanded to include other program config
pub struct AppDataStorage(Storage);

impl AppDataStorage {
    fn new(data_dir: &PathBuf) -> Result<Self, StorageError> {
        Ok(AppDataStorage(Storage::new(data_dir)?))
    }

    /// Write error to error log file in app data storage
    /// TODO: write_append mode?
    pub async fn write_error(&self, url: &str, error_msg: &str) -> Result<(), StorageError> {
        let log_entry = format!(
            "[{}] Failed to fetch {}: {}\n",
            chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC"),
            url,
            error_msg
        );

        // Append to existing error log or create new one
        let existing_content = match self.0.read(ERROR_LOG_FILNAME).await {
            Ok(data) => String::from_utf8_lossy(&data).to_string(),
            Err(_) => String::new(),
        };

        self.0
            .write(
                ERROR_LOG_FILNAME,
                format!("{}{}", existing_content, log_entry),
            )
            .await?;

        error!("{}", log_entry);
        Ok(())
    }

    pub async fn write_last_pubky(&self, pubky: &PublicKey) -> Result<(), StorageError> {
        let pubky_str = pubky.to_string();
        self.0.write(LAST_PUBKY_FILENAME, pubky_str.clone()).await?;
        debug!("Last pubky value written: {}", pubky_str);
        Ok(())
    }

    pub async fn read_last_pubky(&self) -> Result<Option<PublicKey>, StorageError> {
        match self.0.read(LAST_PUBKY_FILENAME).await {
            Ok(data) => {
                let pubky = PublicKey::from_str(&String::from_utf8(data.to_vec())?)
                    .map_err(|e| StorageError::Internal(e.to_string()))?;
                Ok(Some(pubky))
            }
            Err(_) => Ok(None),
        }
    }
}

/// Storage for User's backed-up data
pub struct BackupDataStorage(Storage);

impl BackupDataStorage {
    fn new(data_dir: &PathBuf) -> Result<Self, StorageError> {
        Ok(BackupDataStorage(Storage::new(data_dir)?))
    }

    /// Write cursor to track backup progress for a pubky
    /// Uses atomic write-then-rename to prevent torn writes on crashes
    pub async fn write_cursor(
        &self,
        pubky: &PublicKey,
        cursor_value: String,
    ) -> Result<(), StorageError> {
        let cursor_path = format!("{}/{}", pubky, CURSOR_FILENAME);
        let temp_path = format!("{}/{}.tmp", pubky, CURSOR_FILENAME);
        self.0.write(&temp_path, cursor_value.clone()).await?;
        self.0.rename(&temp_path, &cursor_path).await?;
        debug!("Cursor value written: {}", cursor_value);
        Ok(())
    }

    /// Read existing or create new cursor for a pubky
    pub async fn read_cursor(&self, pubky: &PublicKey) -> Result<String, StorageError> {
        match self.0.read(&format!("{}/{}", pubky, CURSOR_FILENAME)).await {
            Ok(cursor_data) => {
                let cursor_string = String::from_utf8(cursor_data)?;
                Ok(cursor_string)
            }
            Err(_) => {
                // TODO:In this case check if data exists. If so then something has gone wrong and we will start backup from the top.
                info!("Cursor file not found, creating empty cursor file");
                self.0
                    .write(&format!("{}/{}", pubky, CURSOR_FILENAME), "")
                    .await?;
                Ok(String::new())
            }
        }
    }

    /// Write data to backup storage using PubkyResource path
    pub async fn write(
        &self,
        resource: &PubkyResource,
        data: Vec<u8>,
    ) -> Result<String, StorageError> {
        let file_path = resource.to_string();
        self.0.write(&file_path, data).await?;
        Ok(file_path)
    }

    /// Delete data from backup storage using PubkyResource path
    pub async fn delete(&self, resource: &PubkyResource) -> Result<String, StorageError> {
        let file_path = resource.to_string();
        self.0.delete(&file_path).await?;
        Ok(file_path)
    }

    /// Read data from backup storage using PubkyResource path
    #[cfg(test)]
    pub async fn read(&self, resource: &PubkyResource) -> Result<Vec<u8>, StorageError> {
        let file_path = resource.to_string();
        self.0.read(&file_path).await
    }

    /// List directories in the backup storage root to find previously backed-up public keys
    pub async fn list_pubky_directories(&self) -> Result<Vec<String>, StorageError> {
        let mut keys = Vec::new();
        for entry in self.0.list("").await? {
            let path = entry.path();
            if entry.metadata().is_dir() && PublicKey::from_str(path).is_ok() {
                keys.push(path.trim_end_matches('/').to_string());
            }
        }
        Ok(keys)
    }

    /// Calculate the total size of data stored for a specific pubky in backup storage
    pub async fn calculate_pubky_size(&self, pubky: &PublicKey) -> u64 {
        // Ensure path ends with / for directory listing
        let path = format!("{}/", pubky);

        match self.calculate_dir_size(&path).await {
            Ok(size) => size,
            Err(e) => {
                error!("Failed to calculate data size for pubky {}: {}", pubky, e);
                0
            }
        }
    }

    /// Recursively calculate directory size
    async fn calculate_dir_size(&self, path: &str) -> Result<u64, StorageError> {
        let mut total_size = 0u64;

        // Check if directory exists first
        match self.0.stat(path).await {
            Ok(metadata) => {
                if !metadata.is_dir() {
                    return Ok(0);
                }
            }
            Err(_) => {
                return Ok(0); // Directory doesn't exist, return 0 size
            }
        }

        let mut entries = self.0.lister_recursive(path).await?;
        while let Some(entry) = entries.next().await {
            match entry {
                Ok(entry) => {
                    let metadata = entry.metadata();

                    if metadata.is_file() {
                        // Get actual file size using stat() since lister metadata.content_length() returns 0
                        let size = match self.0.stat(entry.path()).await {
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
}

/// Application-specific storage that manages both app data and backup data.
///
/// This is the main storage interface used by [`BackupController`](crate::BackupController).
/// It handles:
/// - Writing/reading/deleting backed-up Pubky resources
/// - Tracking sync progress via cursors
/// - Logging errors
/// - Storing application metadata (like the last used pubky)
///
/// Errors from write/delete operations are logged to `error.log` and don't cause
/// the backup process to fail. Other errors are propagated to the caller.
///
/// # Storage Layout
///
/// ```text
/// ~/.pubky-backup/
/// ├── error.log              # Error log for write/delete failures
/// ├── last_pubky             # Last used public key
/// └── <pubky>/               # Directory per backed-up pubky
///     ├── cursor             # Sync progress cursor
///     └── pub/               # Backed-up resources
///         ├── profile.json
///         └── ...
/// ```
pub struct AppStorage {
    /// Application's data Eg error.log
    app_data: AppDataStorage,
    /// User's backed-up data
    backup_data: BackupDataStorage,
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
        Ok(AppStorage {
            app_data: AppDataStorage::new(&data_dir)?,
            backup_data: BackupDataStorage::new(&data_dir)?,
            data_dir,
        })
    }

    #[cfg(test)]
    pub fn new_with_single_path(data_dir: &PathBuf) -> Result<Self, StorageError> {
        Ok(AppStorage {
            app_data: AppDataStorage::new(data_dir)?,
            backup_data: BackupDataStorage::new(data_dir)?,
            data_dir: data_dir.clone(),
        })
    }

    /// Write data to backup storage using PubkyResource path.
    ///
    /// Errors are logged to `error.log` but don't cause this method to fail.
    ///
    /// # Arguments
    ///
    /// * `resource` - The Pubky resource to store
    /// * `data` - The raw data to store
    pub async fn write(&self, resource: &PubkyResource, data: Vec<u8>) -> Result<(), StorageError> {
        match self.backup_data.write(resource, data).await {
            Ok(_) => Ok(()),
            Err(e) => {
                self.app_data
                    .write_error(&resource.to_string(), &format!("Failed to write: {}", e))
                    .await?;
                Ok(())
            }
        }
    }

    /// Delete data from backup storage using PubkyResource path.
    ///
    /// Errors are logged to `error.log` but don't cause this method to fail.
    ///
    /// # Arguments
    ///
    /// * `resource` - The Pubky resource to delete
    pub async fn delete(&self, resource: &PubkyResource) -> Result<(), StorageError> {
        match self.backup_data.delete(resource).await {
            Ok(_) => Ok(()),
            Err(e) => {
                self.app_data
                    .write_error(&resource.to_string(), &format!("Failed to delete: {}", e))
                    .await?;
                Ok(())
            }
        }
    }

    /// Read data from backup storage using PubkyResource path
    #[cfg(test)]
    pub async fn read(&self, resource: &PubkyResource) -> Result<Vec<u8>, StorageError> {
        self.backup_data.read(resource).await
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
        self.backup_data.write_cursor(pubky, cursor_value).await
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
        self.backup_data.read_cursor(pubky).await
    }

    /// Write an error to the error log file.
    ///
    /// Used to log non-critical errors that don't stop the backup process.
    ///
    /// # Arguments
    ///
    /// * `url` - The URL or resource that caused the error
    /// * `error_msg` - The error message to log
    pub async fn write_error(&self, url: &str, error_msg: &str) -> Result<(), StorageError> {
        self.app_data.write_error(url, error_msg).await
    }

    /// Calculate the total size of data stored for a specific pubky.
    ///
    /// Recursively traverses the pubky's directory and sums file sizes.
    ///
    /// # Arguments
    ///
    /// * `pubky` - The public key to calculate size for
    ///
    /// # Returns
    ///
    /// Total size in bytes, or 0 if the directory doesn't exist or an error occurs
    pub async fn calculate_pubky_size(&self, pubky: &PublicKey) -> u64 {
        self.backup_data.calculate_pubky_size(pubky).await
    }

    /// List all public keys that have backed-up data.
    ///
    /// Scans the backup storage root for directories that are valid public keys.
    ///
    /// # Returns
    ///
    /// Vector of public key strings
    pub async fn list_pubky_directories(&self) -> Result<Vec<String>, StorageError> {
        self.backup_data.list_pubky_directories().await
    }

    /// Get the data directory path.
    ///
    /// # Returns
    ///
    /// Path to the backup data directory
    pub fn get_backup_data_dir(&self) -> Result<PathBuf, StorageError> {
        get_data_directory()
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
    /// The snapshot is stored in `<data_dir>/snapshots/` with a timestamped filename.
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
    /// - The pubky directory doesn't exist
    /// - The pubky directory is empty (no files to snapshot)
    /// - The snapshots directory cannot be created
    /// - The zip file cannot be created or written
    pub async fn create_snapshot(&self, pubky: &PublicKey) -> Result<PathBuf, StorageError> {
        let data_dir = self.data_dir.clone();
        let pubky_str = pubky.to_string();

        // Run blocking I/O in a separate thread to avoid blocking the async runtime
        let result = tokio::task::spawn_blocking(move || {
            Self::create_snapshot_blocking(&data_dir, &pubky_str)
        })
        .await
        .map_err(|e| StorageError::Internal(format!("Snapshot task failed: {}", e)))?;

        result
    }

    /// Blocking implementation of snapshot creation.
    /// Called from spawn_blocking to avoid blocking the async runtime.
    fn create_snapshot_blocking(
        data_dir: &Path,
        pubky_str: &str,
    ) -> Result<PathBuf, StorageError> {
        let pubky_dir = data_dir.join(pubky_str);
        if !pubky_dir.exists() {
            return Err(StorageError::Internal(format!(
                "No backup data found for pubky: {}",
                pubky_str
            )));
        }

        let snapshots_dir = data_dir.join(SNAPSHOTS_DIR_NAME);
        std::fs::create_dir_all(&snapshots_dir).map_err(|e| {
            StorageError::DirectoryCreation(format!("{}: {}", snapshots_dir.display(), e))
        })?;
        let timestamp = chrono::Utc::now().format("%Y-%m-%d_%H-%M-%S");
        let pubky_prefix = if pubky_str.len() > 10 {
            &pubky_str[..10]
        } else {
            pubky_str
        };
        let snapshot_filename = format!("snapshot_{}_{}.zip", pubky_prefix, timestamp);
        let snapshot_path = snapshots_dir.join(&snapshot_filename);

        let file = File::create(&snapshot_path).map_err(|e| {
            StorageError::Internal(format!("Failed to create snapshot file: {}", e))
        })?;
        let mut zip = zip::ZipWriter::new(file);
        let options: FileOptions<()> = FileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .unix_permissions(0o644);

        info!(
            "Creating snapshot of {} to {}",
            pubky_str,
            snapshot_path.display()
        );

        // Walk the pubky directory and add files to zip
        let mut file_count = 0;
        for entry in WalkDir::new(&pubky_dir)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            let relative_path = path.strip_prefix(&pubky_dir).map_err(|e| {
                StorageError::Internal(format!("Failed to get relative path: {}", e))
            })?;

            if path.is_file() {
                let file_data = std::fs::read(path).map_err(|e| {
                    StorageError::Internal(format!(
                        "Failed to read file {}: {}",
                        path.display(),
                        e
                    ))
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
            return Err(StorageError::Internal(format!(
                "No files to snapshot for pubky: {}",
                pubky_str
            )));
        }

        zip.finish().map_err(|e| {
            StorageError::Internal(format!("Failed to finalize zip file: {}", e))
        })?;

        info!("Snapshot created successfully: {}", snapshot_path.display());
        Ok(snapshot_path)
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
        let storage = AppStorage::new_with_single_path(&temp_dir.path().to_path_buf()).unwrap();
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
        let dirs = storage.list_pubky_directories().await.unwrap();
        assert_eq!(dirs.len(), 0);

        // Write data for two different pubkys
        let resource1 = PubkyResource::new(pubky1.clone(), "/pub/test1.json").unwrap();
        let resource2 = PubkyResource::new(pubky2.clone(), "/pub/test2.json").unwrap();

        storage.write(&resource1, b"data1".to_vec()).await.unwrap();
        storage.write(&resource2, b"data2".to_vec()).await.unwrap();

        // Should now list both pubky directories
        let dirs = storage.list_pubky_directories().await.unwrap();
        assert_eq!(dirs.len(), 2);
        assert!(dirs.contains(&pubky1.to_string()));
        assert!(dirs.contains(&pubky2.to_string()));
    }

    #[tokio::test]
    async fn test_write_error_logs() {
        let (storage, _temp_dir) = create_test_storage();

        // Write some errors
        storage
            .write_error("/pub/test1.json", "Test error 1")
            .await
            .unwrap();
        storage
            .write_error("/pub/test2.json", "Test error 2")
            .await
            .unwrap();

        // Errors should not cause the function to fail
        // Just verify no panic occurred and the function returns Ok
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
        let temp_file_path = temp_dir.path().join(pubky.to_string()).join("cursor.tmp");
        assert!(
            !temp_file_path.exists(),
            "Temporary cursor file should not exist after atomic write"
        );

        // Verify actual cursor file exists
        let cursor_file_path = temp_dir.path().join(pubky.to_string()).join("cursor");
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

        // Verify it's in the snapshots directory
        assert!(
            snapshot_path.parent().unwrap().ends_with("snapshots"),
            "Snapshot should be in snapshots directory"
        );

        // Verify filename format
        let filename = snapshot_path.file_name().unwrap().to_str().unwrap();
        assert!(
            filename.starts_with("snapshot_"),
            "Filename should start with 'snapshot_'"
        );
        assert!(filename.ends_with(".zip"), "Filename should end with '.zip'");

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
                    contents,
                    b"content_one",
                    "test1.json should have correct content"
                );
            } else if name.contains("test2.json") {
                assert_eq!(
                    contents,
                    b"content_two",
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

        // Try to create snapshot for non-existent pubky
        let result = storage.create_snapshot(&pubky).await;

        assert!(result.is_err(), "Should fail when no backup data exists");
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("No backup data found"),
            "Error should mention no backup data"
        );
    }

    #[tokio::test]
    async fn test_create_snapshot_empty_directory() {
        let (storage, temp_dir) = create_test_storage();
        let pubky = PublicKey::from_str(DEV_MODE_PUBKY).unwrap();

        // Create the pubky directory but don't add any files
        let pubky_dir = temp_dir.path().join(pubky.to_string());
        std::fs::create_dir_all(&pubky_dir).unwrap();

        // Try to create snapshot for empty pubky directory
        let result = storage.create_snapshot(&pubky).await;

        assert!(
            result.is_err(),
            "Should fail when pubky directory is empty"
        );
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("No files to snapshot"),
            "Error should mention no files to snapshot"
        );

        // Verify no orphaned zip file was left behind
        let snapshots_dir = temp_dir.path().join("snapshots");
        if snapshots_dir.exists() {
            let entries: Vec<_> = std::fs::read_dir(&snapshots_dir)
                .unwrap()
                .collect();
            assert!(
                entries.is_empty(),
                "No snapshot file should be left behind for empty directory"
            );
        }
    }
}
