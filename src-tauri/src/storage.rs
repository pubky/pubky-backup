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

    pub async fn write_cursor(&self, pubky: &str, cursor_value: String) -> Result<()> {
        let cursor_path = format!("{}/{}", pubky, CURSOR_FILENAME);
        self.operator.write(&cursor_path, cursor_value.clone()).await?;
        debug!("Cursor value written: {}", cursor_value);
        Ok(())
    }

    /// Fetch existsing or create new cursor
    pub async fn read_cursor(&self, pubky: &str) -> Result<String> {
        let cursor_path = format!("{}/{}", pubky, CURSOR_FILENAME);
        match self.operator.read(&cursor_path).await {
            Ok(cursor_data) => {
                let cursor_string = String::from_utf8(cursor_data.to_vec())?;
                debug!("Cursor value read: {}", cursor_string);
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
}