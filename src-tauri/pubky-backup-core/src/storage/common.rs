//! Common storage utilities shared across storage modules.
//!
//! This module provides the low-level [`Storage`] abstraction that wraps
//! OpenDAL operations with consistent error handling.

use super::error::{OperationFailedError, StorageError};
use opendal::{services::Fs, Operator};
use std::path::Path;

/// Simple storage abstraction for reading/writing/deleting files.
///
/// This is a thin wrapper around OpenDAL's [`Operator`] that provides
/// consistent error handling and a simplified API for file operations.
pub(crate) struct Storage {
    operator: Operator,
}

impl Storage {
    /// Create a new Storage instance rooted at the given directory.
    ///
    /// Creates the directory if it doesn't exist.
    pub fn new(data_dir: &Path) -> Result<Self, StorageError> {
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

    /// Write data to a file, creating parent directories as needed.
    pub async fn write(
        &self,
        file_path: &str,
        data: impl Into<Vec<u8>>,
    ) -> Result<(), StorageError> {
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

    /// Read data from a file.
    pub async fn read(&self, file_path: &str) -> Result<Vec<u8>, StorageError> {
        let data = self.operator.read(file_path).await.map_err(|e| {
            StorageError::OperationFailed(Box::new(OperationFailedError {
                operation: "read".to_string(),
                path: file_path.to_string(),
                source: e,
            }))
        })?;
        Ok(data.to_vec())
    }

    /// Delete a file.
    pub async fn delete(&self, file_path: &str) -> Result<(), StorageError> {
        self.operator.delete(file_path).await.map_err(|e| {
            StorageError::OperationFailed(Box::new(OperationFailedError {
                operation: "delete".to_string(),
                path: file_path.to_string(),
                source: e,
            }))
        })?;
        Ok(())
    }

    /// Get metadata for a path.
    pub async fn stat(&self, path: &str) -> Result<opendal::Metadata, StorageError> {
        Ok(self.operator.stat(path).await?)
    }

    /// Get a recursive lister for a path.
    pub async fn lister_recursive(&self, path: &str) -> Result<opendal::Lister, StorageError> {
        Ok(self.operator.lister_with(path).recursive(true).await?)
    }

    /// Rename a file atomically.
    pub async fn rename(&self, from: &str, to: &str) -> Result<(), StorageError> {
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
    pub async fn append(
        &self,
        file_path: &str,
        data: impl Into<Vec<u8>>,
    ) -> Result<(), StorageError> {
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
