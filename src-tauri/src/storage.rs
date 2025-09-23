use anyhow::{Context, Result};
use log::{debug, error, info};
use opendal::{services::Fs, Operator};
use std::path::Path;

const DATA_DIR: &str = "../.pubky-backup";
const CURSOR_FILENAME: &str = "cursor";

pub struct Storage {
    operator: Operator,
}

impl Storage {
    pub fn new() -> Result<Self> {
        let builder = Fs::default().root(DATA_DIR);
        let operator = Operator::new(builder)?
            .layer(opendal::layers::LoggingLayer::default())
            .finish();
        Ok(Storage { operator })
    }

    /// Convert pubky URL to file path by stripping the "pubky://" prefix
    fn url_to_path(&self, pubky_url: &str) -> Result<String> {
        pubky_url
            .strip_prefix("pubky://")
            .map(|path| path.to_string())
            .ok_or_else(|| anyhow::anyhow!("Invalid pubky URL format: {}", pubky_url))
    }

    /// Write data to storage using pubky URL path
    pub async fn write(&self, pubky_url: &str, data: Vec<u8>) -> Result<()> {
        let file_path = self.url_to_path(pubky_url)?;
        self.operator
            .write(&file_path, data)
            .await
            .with_context(|| format!("Failed to store data for {}", pubky_url))?;
        Ok(())
    }

    /// Delete data from storage using pubky URL path
    pub async fn delete(&self, pubky_url: &str) -> Result<()> {
        let file_path = self.url_to_path(pubky_url)?;
        self.operator
            .delete(&file_path)
            .await
            .with_context(|| format!("Failed to delete data for {}", pubky_url))?;
        Ok(())
    }

    /// Read data from storage using pubky URL path
    #[allow(dead_code)]
    pub async fn read(&self, pubky_url: &str) -> Result<Vec<u8>> {
        let file_path = self.url_to_path(pubky_url)?;
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

    #[tokio::test]
    async fn test_cursor_read_write() {
        let storage = Storage::new().expect("Failed to create storage");
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
        let storage = Storage::new().unwrap();
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
}
