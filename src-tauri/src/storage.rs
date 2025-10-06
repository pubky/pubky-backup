use anyhow::{Context, Result};
use futures_lite::StreamExt;
use log::{debug, error, info};
use opendal::{services::Fs, Operator};
use pubky::{PubkyResource, PublicKey};
use std::{path::PathBuf, str::FromStr};

const APP_DATA_DIR_NAME: &str = ".pubky-backup";
const CURSOR_FILENAME: &str = "cursor";
const ERROR_LOG_FILNAME: &str = "error.log";
const LAST_PUBKY_FILENAME: &str = "last_pubky";

/// Get the root data directory - used for both app config and backup data for now
pub fn get_data_directory() -> Result<PathBuf> {
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
    fn new(data_dir: &PathBuf) -> Result<Self> {
        // Ensure the directory exists
        std::fs::create_dir_all(data_dir)
            .with_context(|| format!("Failed to create data directory: {}", data_dir.display()))?;

        let builder = Fs::default().root(&data_dir.to_string_lossy().to_string());
        let operator = Operator::new(builder)?
            .layer(opendal::layers::LoggingLayer::default())
            .finish();
        Ok(Storage { operator })
    }

    async fn write(&self, file_path: &str, data: impl Into<Vec<u8>>) -> Result<()> {
        self.operator
            .write(file_path, data.into())
            .await
            .with_context(|| format!("Failed to write to {}", file_path))?;
        Ok(())
    }

    async fn read(&self, file_path: &str) -> Result<Vec<u8>> {
        let data = self
            .operator
            .read(file_path)
            .await
            .with_context(|| format!("Failed to read from {}", file_path))?;
        Ok(data.to_vec())
    }

    async fn delete(&self, file_path: &str) -> Result<()> {
        self.operator
            .delete(file_path)
            .await
            .with_context(|| format!("Failed to delete {}", file_path))?;
        Ok(())
    }

    async fn list(&self, path: &str) -> Result<Vec<opendal::Entry>> {
        Ok(self.operator.list(path).await?)
    }

    async fn stat(&self, path: &str) -> Result<opendal::Metadata> {
        Ok(self.operator.stat(path).await?)
    }

    /// Create a lister for recursive directory traversal
    async fn lister_recursive(&self, path: &str) -> Result<opendal::Lister> {
        Ok(self.operator.lister_with(path).recursive(true).await?)
    }
}

/// Storage for application data (error logs)
/// Will be expanded to store backup location, last_pubky and other program config
pub struct AppDataStorage(Storage);

impl AppDataStorage {
    fn new(data_dir: &PathBuf) -> Result<Self> {
        Ok(AppDataStorage(Storage::new(data_dir)?))
    }

    /// Write error to error log file in app data storage
    /// TODO: write_append mode?
    pub async fn write_error(&self, url: &str, error_msg: &str) -> Result<()> {
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
            .await
            .with_context(|| "Failed to write error log".to_string())?;

        error!("{}", log_entry);
        Ok(())
    }

    pub async fn write_last_pubky(&self, pubky: String) -> Result<()> {
        self.0
            .write(LAST_PUBKY_FILENAME, pubky.clone())
            .await
            .with_context(|| "Failed to write last used pubky")?;
        Ok(())
    }

    pub async fn read_last_pubky(&self) -> Result<Option<String>> {
        match self.0.read(LAST_PUBKY_FILENAME).await {
            Ok(data) => {
                let pubky = String::from_utf8(data.to_vec())?;
                Ok(Some(pubky))
            }
            Err(_) => Ok(None),
        }
    }
}

/// Storage for user's backed-up data
pub struct BackupDataStorage(Storage);

impl BackupDataStorage {
    fn new(data_dir: &PathBuf) -> Result<Self> {
        Ok(BackupDataStorage(Storage::new(data_dir)?))
    }

    /// Write cursor to track backup progress for a pubky
    pub async fn write_cursor(&self, pubky: &str, cursor_value: String) -> Result<()> {
        self.0
            .write(
                &format!("{}/{}", pubky, CURSOR_FILENAME),
                cursor_value.clone(),
            )
            .await?;
        debug!("Cursor value written: {}", cursor_value);
        Ok(())
    }

    /// Read existing or create new cursor for a pubky
    pub async fn read_cursor(&self, pubky: &str) -> Result<String> {
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
                    .await
                    .with_context(|| format!("Failed to create cursor file for {}", pubky))?;
                Ok(String::new())
            }
        }
    }

    /// Write data to backup storage using PubkyResource path
    pub async fn write(&self, resource: &PubkyResource, data: Vec<u8>) -> Result<String> {
        let file_path = resource.to_string();
        self.0.write(&file_path, data).await?;
        Ok(file_path)
    }

    /// Delete data from backup storage using PubkyResource path
    pub async fn delete(&self, resource: &PubkyResource) -> Result<String> {
        let file_path = resource.to_string();
        self.0.delete(&file_path).await?;
        Ok(file_path)
    }

    /// Read data from backup storage using PubkyResource path
    #[cfg(test)]
    pub async fn read(&self, resource: &PubkyResource) -> Result<Vec<u8>> {
        let file_path = resource.to_string();
        self.0.read(&file_path).await
    }

    /// Calculate the total size of data stored for a specific pubky in backup storage
    pub async fn calculate_pubky_size(&self, pubky: &str) -> u64 {
        // Ensure path ends with / for directory listing
        let path = if pubky.ends_with('/') {
            pubky.to_string()
        } else {
            format!("{}/", pubky)
        };

        match self.calculate_dir_size(&path).await {
            Ok(size) => size,
            Err(e) => {
                error!("Failed to calculate data size for pubky {}: {}", pubky, e);
                0
            }
        }
    }

    /// List directories in the backup storage root to find previously backed-up public keys
    pub async fn list_pubky_directories(&self) -> Result<Vec<String>> {
        let mut keys = Vec::new();
        for entry in self.0.list("").await? {
            let path = entry.path();
            if entry.metadata().is_dir() && PublicKey::from_str(path).is_ok() {
                keys.push(path.trim_end_matches('/').to_string());
            }
        }
        Ok(keys)
    }

    /// Recursively calculate directory size
    async fn calculate_dir_size(&self, path: &str) -> Result<u64> {
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

/// Application-specific storage that manages both app data and backup data
/// This struct bubbles db errors up apart from invalid URLS which are logged into error.log.
/// Currently both app data and backup data are stored in the same root directory.
pub struct AppStorage {
    /// Application's data Eg error.log
    app_data: AppDataStorage,
    /// User's backed-up data
    backup_data: BackupDataStorage,
}

impl AppStorage {
    pub fn new() -> Result<Self> {
        let data_dir = get_data_directory()?;
        Ok(AppStorage {
            app_data: AppDataStorage::new(&data_dir)?,
            backup_data: BackupDataStorage::new(&data_dir)?,
        })
    }

    #[cfg(test)]
    pub fn new_with_single_path(data_dir: &PathBuf) -> Result<Self> {
        Ok(AppStorage {
            app_data: AppDataStorage::new(data_dir)?,
            backup_data: BackupDataStorage::new(data_dir)?,
        })
    }

    /// Write data to backup storage using PubkyResource path
    pub async fn write(&self, resource: &PubkyResource, data: Vec<u8>) -> Result<()> {
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

    /// Delete data from backup storage using PubkyResource path
    pub async fn delete(&self, resource: &PubkyResource) -> Result<()> {
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
    pub async fn read(&self, resource: &PubkyResource) -> Result<Vec<u8>> {
        self.backup_data.read(resource).await
    }

    /// Write cursor to track backup progress
    pub async fn write_cursor(&self, pubky: &str, cursor_value: String) -> Result<()> {
        self.backup_data.write_cursor(pubky, cursor_value).await
    }

    /// Read cursor for backup progress
    pub async fn read_cursor(&self, pubky: &str) -> Result<String> {
        self.backup_data.read_cursor(pubky).await
    }

    /// Write error to error log file in app data storage
    pub async fn write_error(&self, url: &str, error_msg: &str) -> Result<()> {
        self.app_data.write_error(url, error_msg).await
    }

    /// Calculate the total size of data stored for a specific pubky in backup storage
    pub async fn calculate_pubky_size(&self, pubky: &str) -> u64 {
        self.backup_data.calculate_pubky_size(pubky).await
    }

    /// List directories in the backup storage root to find previously backed-up public keys
    pub async fn list_pubky_directories(&self) -> Result<Vec<String>> {
        self.backup_data.list_pubky_directories().await
    }

    /// Get the data directory path (immutable after creation)
    pub fn get_backup_data_dir(&self) -> Result<PathBuf> {
        get_data_directory()
    }

    /// Write last used pubky to app data storage
    pub async fn write_last_pubky(&self, pubky: String) -> Result<()> {
        self.app_data.write_last_pubky(pubky).await
    }

    /// Read last used pubky from app data storage
    pub async fn read_last_pubky(&self) -> Result<Option<String>> {
        self.app_data.read_last_pubky().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_storage_basic_operations() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let storage = BackupDataStorage::new(&temp_dir.path().to_path_buf())
            .expect("Failed to create storage");

        let test_url = "pubky://g1b6wp8bhhxtsksy3td7rj6mgg7s5k8c68663sajkfscshwj8g5y/pub/pubky.app/test/file.txt";
        let test_resource = PubkyResource::from_str(test_url).expect("Valid URL");
        let test_data = b"Hello, world!".to_vec();

        // Test write
        let write_result = storage.write(&test_resource, test_data.clone()).await;
        assert!(write_result.is_ok());

        // Test read
        let read_result = storage.read(&test_resource).await;
        assert!(read_result.is_ok());
        assert_eq!(read_result.unwrap(), test_data);

        // Test delete
        let delete_result = storage.delete(&test_resource).await;
        assert!(delete_result.is_ok());

        // Verify the data no longer exists
        let read_after_delete = storage.read(&test_resource).await;
        assert!(read_after_delete.is_err());
    }

    #[tokio::test]
    async fn test_cursor_read_write() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let storage = AppStorage::new_with_single_path(&temp_dir.path().to_path_buf())
            .expect("Failed to create storage");
        let test_cursor = "0033E867HX6FE";
        let test_pubky = "test_pubky";
        let write_result = storage
            .write_cursor(test_pubky, test_cursor.to_string())
            .await;
        assert!(write_result.is_ok());
        let read_result = storage.read_cursor(test_pubky).await;
        assert!(read_result.is_ok());
        assert_eq!(read_result.unwrap(), test_cursor);

        // last_pubky read/write
        let test_last_pubky = "g1b6wp8bhhxtsksy3td7rj6mgg7s5k8c68663sajkfscshwj8g5y".to_string();
        let write_result = storage.write_last_pubky(test_last_pubky.clone()).await;
        assert!(write_result.is_ok());
        let read_result = storage.read_last_pubky().await;
        assert!(read_result.is_ok());
        assert_eq!(read_result.unwrap(), Some(test_last_pubky));
    }

    #[tokio::test]
    async fn test_write_delete_pubky_data() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let storage = AppStorage::new_with_single_path(&temp_dir.path().to_path_buf())
            .expect("Failed to create storage");
        let test_url = "pubky://68rkfi1d78baobycj6w4b7dga43o8qtnuhubban5at6qywrieb5y/pub/pubky.app/posts/0033E8XNPVSTG";
        let test_resource = PubkyResource::from_str(test_url).expect("Valid URL");
        let test_data = b"Hello, world!".to_vec();
        let expected_path = "68rkfi1d78baobycj6w4b7dga43o8qtnuhubban5at6qywrieb5y/pub/pubky.app/posts/0033E8XNPVSTG";

        // Test write
        let write_result = storage.write(&test_resource, test_data.clone()).await;
        assert!(write_result.is_ok());

        // Verify the data was written to the correct path
        let read_result = storage.backup_data.0.read(expected_path).await;
        assert!(read_result.is_ok());
        assert_eq!(read_result.unwrap(), test_data);

        // Test delete
        let delete_result = storage.delete(&test_resource).await;
        assert!(delete_result.is_ok());

        // Verify the data no longer exists
        let read_after_delete = storage.backup_data.0.read(expected_path).await;
        assert!(read_after_delete.is_err());
    }

    #[test]
    fn test_path_sanitisation() {
        // Test that PubkyResource parsing rejects malicious URLs
        let malicious_url = "pubky://../../etc/passwd";
        let result = PubkyResource::from_str(malicious_url);
        assert!(
            result.is_err(),
            "Path traversal URL should be rejected by PubkyResource"
        );
    }
}
