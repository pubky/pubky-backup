use opendal::{Operator, services::Fs};
use log::{info, error, debug};
use anyhow::Result;

pub struct Storage {
    operator: Operator,
}

impl Storage {
    pub fn new() -> Result<Self> {
        let builder = Fs::default().root("./");
        let operator = Operator::new(builder)?
            .layer(opendal::layers::LoggingLayer::default())
            .finish();
        Ok(Storage { operator })
    }

    pub async fn write_cursor(&self, cursor_value: String) -> Result<()> {
        self.operator.write("cursor.txt", cursor_value.clone()).await?;
        debug!("Cursor value written successfully: {}", cursor_value);
        Ok(())
    }

    pub async fn read_cursor(&self) -> Result<String> {
        let cursor_data = self.operator.read("cursor.txt").await?;
        let cursor_string = String::from_utf8(cursor_data.to_vec())?;
        info!("Cursor value read successfully: {}", cursor_string);
        Ok(cursor_string)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_cursor_read_write() {
        let storage = Storage::new().expect("Failed to create storage");
        let test_cursor = "0033E867HX6FE";

        let write_result = storage.write_cursor(test_cursor.to_string()).await;
        assert!(write_result.is_ok());

        let read_result = storage.read_cursor().await;
        assert!(read_result.is_ok());
        assert_eq!(read_result.unwrap(), test_cursor);
    }
}