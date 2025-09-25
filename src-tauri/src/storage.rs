use anyhow::{Context, Result};
use log::{debug, error, info};
use opendal::{services::Fs, Operator};
use pubky::ResourcePath;
use std::path::Path;

const DATA_DIR: &str = "../.pubky-backup";
const CURSOR_FILENAME: &str = "cursor";

pub struct Storage {
    operator: Operator,
}

impl Storage {
    pub fn new() -> Result<Self> {
        Self::with_root(DATA_DIR)
    }

    pub fn with_root(root_dir: &str) -> Result<Self> {
        let builder = Fs::default().root(root_dir);
        let operator = Operator::new(builder)?
            .layer(opendal::layers::LoggingLayer::default())
            .finish();
        Ok(Storage { operator })
    }

    /// Convert pubky URL to safe file path
    fn url_to_path(&self, pubky_url: &str) -> Result<String> {
        let path = pubky_url
            .strip_prefix("pubky://")
            .ok_or_else(|| anyhow::anyhow!("Invalid pubky URL format: {}", pubky_url))?;
        // Parse into ResourcePath for sanitisation
        Ok(ResourcePath::parse(path)?.as_str().to_owned())
    }

    /// Write data to storage using pubky URL path
    pub async fn write(&self, pubky_url: &str, data: Vec<u8>) -> Result<()> {
        let file_path = match self.url_to_path(pubky_url) {
            Ok(path) => path,
            Err(e) => {
                let _ = self
                    .write_error(pubky_url, &format!("Invalid URL path: {}", e))
                    .await;
                return Ok(());
            }
        };
        self.operator
            .write(&file_path, data)
            .await
            .with_context(|| format!("Failed to store data for {}", pubky_url))?;
        Ok(())
    }

    /// Delete data from storage using pubky URL path
    pub async fn delete(&self, pubky_url: &str) -> Result<()> {
        let file_path = match self.url_to_path(pubky_url) {
            Ok(path) => path,
            Err(e) => {
                let _ = self
                    .write_error(pubky_url, &format!("Invalid URL path: {}", e))
                    .await;
                return Ok(());
            }
        };
        self.operator
            .delete(&file_path)
            .await
            .with_context(|| format!("Failed to delete data for {}", pubky_url))?;
        Ok(())
    }

    /// Read data from storage using pubky URL path
    #[allow(dead_code)]
    pub async fn read(&self, pubky_url: &str) -> Result<Vec<u8>> {
        let file_path = match self.url_to_path(pubky_url) {
            Ok(path) => path,
            Err(e) => {
                let _ = self
                    .write_error(pubky_url, &format!("Invalid URL path: {}", e))
                    .await;
                return Ok(Vec::new());
            }
        };
        let data = self
            .operator
            .read(&file_path)
            .await
            .with_context(|| format!("Failed to read data for {}", pubky_url))?;
        Ok(data.to_vec())
    }

    pub async fn write_cursor(&self, pubky: &str, cursor_value: String) -> Result<()> {
        let cursor_path = format!("{}/{}", pubky, CURSOR_FILENAME);
        self.operator
            .write(&cursor_path, cursor_value.clone())
            .await
            .with_context(|| format!("Failed to update cursor for {}", pubky))?;
        debug!("Cursor value written: {}", cursor_value);
        Ok(())
    }

    /// Read existing or create new cursor
    pub async fn read_cursor(&self, pubky: &str) -> Result<String> {
        let cursor_path = format!("{}/{}", pubky, CURSOR_FILENAME);
        match self.operator.read(&cursor_path).await {
            Ok(cursor_data) => {
                let cursor_string = String::from_utf8(cursor_data.to_vec())?;
                Ok(cursor_string)
            }
            Err(_) => {
                // TODO:In this case check if data exists. If so then something has gone wrong and we will start backup from the top.
                info!("Cursor file not found, creating empty cursor file");
                self.operator
                    .write(&cursor_path, "")
                    .await
                    .with_context(|| format!("Failed to create cursor file for {}", pubky))?;
                Ok(String::new())
            }
        }
    }

    /// Write error to error log file
    /// TODO: write_append mode?
    pub async fn write_error(&self, url: &str, error_msg: &str) -> Result<()> {
        let error_log_path = "error.log";
        let timestamp = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC");
        let log_entry = format!("[{}] Failed to fetch {}: {}\n", timestamp, url, error_msg);

        // Append to existing error log or create new one
        let existing_content = match self.operator.read(error_log_path).await {
            Ok(data) => String::from_utf8_lossy(&data.to_vec()).to_string(),
            Err(_) => String::new(),
        };

        self.operator
            .write(error_log_path, format!("{}{}", existing_content, log_entry))
            .await
            .with_context(|| "Failed to write error log".to_string())?;
        Ok(())
    }

    /// Calculate the total size of data stored for a specific pubky
    pub fn calculate_pubky_size(&self, pubky: &str) -> u64 {
        let pubky_path = Path::new(DATA_DIR).join(pubky);
        if !pubky_path.exists() {
            return 0;
        }

        match calculate_dir_size(&pubky_path) {
            Ok(size) => size,
            Err(e) => {
                error!("Failed to calculate data size for pubky {}: {}", pubky, e);
                0
            }
        }
    }
}

/// Recursively calculate the size of a directory
fn calculate_dir_size(dir: &Path) -> Result<u64> {
    let mut total_size = 0u64;

    if dir.is_dir() {
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.is_dir() {
                total_size += calculate_dir_size(&path)?;
            } else if path.is_file() {
                total_size += entry.metadata()?.len();
            }
        }
    }

    Ok(total_size)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_cursor_read_write() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let temp_path = temp_dir.path().to_str().expect("Failed to get temp path");
        let storage = Storage::with_root(temp_path).expect("Failed to create storage");
        let test_cursor = "0033E867HX6FE";
        let test_pubky = "test_pubky";

        let write_result = storage
            .write_cursor(test_pubky, test_cursor.to_string())
            .await;
        assert!(write_result.is_ok());

        let read_result = storage.read_cursor(test_pubky).await;
        assert!(read_result.is_ok());
        assert_eq!(read_result.unwrap(), test_cursor);
    }

    #[tokio::test]
    async fn test_write_delete_pubky_data() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let temp_path = temp_dir.path().to_str().expect("Failed to get temp path");
        let storage = Storage::with_root(temp_path).expect("Failed to create storage");
        let test_url = "pubky://68rkfi1d78baobycj6w4b7dga43o8qtnuhubban5at6qywrieb5y/pub/pubky.app/posts/0033E8XNPVSTG";
        let test_data = b"Hello, world!".to_vec();
        let expected_path = "68rkfi1d78baobycj6w4b7dga43o8qtnuhubban5at6qywrieb5y/pub/pubky.app/posts/0033E8XNPVSTG";
        // Test write
        let write_result = storage.write(test_url, test_data.clone()).await;
        assert!(write_result.is_ok());
        // Verify the data was written to the correct path
        let read_result = storage.operator.read(expected_path).await;
        assert!(read_result.is_ok());
        assert_eq!(read_result.unwrap().to_vec(), test_data);
        // Test delete
        let delete_result = storage.delete(test_url).await;
        assert!(delete_result.is_ok());
        // Verify the data no longer exists
        let read_after_delete = storage.operator.read(expected_path).await;
        assert!(read_after_delete.is_err());
    }

    #[test]
    fn test_url_to_path_sanitisation() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let temp_path = temp_dir.path().to_str().expect("Failed to get temp path");
        let storage = Storage::with_root(temp_path).expect("Failed to create storage");

        // Test path traversal attack - should fail
        let malicious_url = "pubky://../../etc/passwd";
        let result = storage.url_to_path(malicious_url);
        assert!(result.is_err(), "Path traversal URL should be rejected");
    }
}
