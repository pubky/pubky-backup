use opendal::{Operator, services::Fs};
use log::{info, debug};
use anyhow::Result;

const DATA_DIR: &str = "../data-dir";
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
        pubky_url.strip_prefix("pubky://")
            .map(|path| path.to_string())
            .ok_or_else(|| anyhow::anyhow!("Invalid pubky URL format: {}", pubky_url))
    }

    /// Write data to storage using pubky URL path
    pub async fn write(&self, pubky_url: &str, data: Vec<u8>) -> Result<()> {
        let file_path = self.url_to_path(pubky_url)?;
        self.operator.write(&file_path, data).await?;
        Ok(())
    }

    /// Delete data from storage using pubky URL path
    pub async fn delete(&self, pubky_url: &str) -> Result<()> {
        let file_path = self.url_to_path(pubky_url)?;
        self.operator.delete(&file_path).await?;
        Ok(())
    }

    pub async fn write_cursor(&self, pubky: &str, cursor_value: String) -> Result<()> {
        let cursor_path = format!("{}/{}", pubky, CURSOR_FILENAME);
        self.operator.write(&cursor_path, cursor_value.clone()).await?;
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
                self.operator.write(&cursor_path, "").await?;
                Ok(String::new())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_cursor_read_write() {
        let storage = Storage::new().expect("Failed to create storage");
        let test_cursor = "0033E867HX6FE";
        let test_pubky = "test_pubky";

        let write_result = storage.write_cursor(test_pubky, test_cursor.to_string()).await;
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