//! Storage module error types.

/// Errors that can occur during storage operations.
#[derive(thiserror::Error, Debug)]
pub enum StorageError {
    /// Internal storage error
    #[error("Internal error: {0}")]
    Internal(String),

    /// OpenDAL operation failed
    #[error("OpenDAL error: {0}")]
    OpenDalError(#[from] opendal::Error),

    /// Invalid UTF-8 in stored data
    #[error("Invalid UTF-8: {0}")]
    InvalidUtf8(#[from] std::string::FromUtf8Error),

    /// Failed to create a directory
    #[error("Failed to create directory: {0}")]
    DirectoryCreation(String),

    /// A storage operation failed
    #[error("{0}")]
    OperationFailed(Box<OperationFailedError>),
}

/// Detailed error for failed storage operations.
#[derive(Debug)]
pub struct OperationFailedError {
    /// The operation that failed (e.g., "read", "write", "delete")
    pub operation: String,
    /// The path the operation was attempted on
    pub path: String,
    /// The underlying error
    pub source: opendal::Error,
}

impl std::fmt::Display for OperationFailedError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Failed to {} {}: {}",
            self.operation, self.path, self.source
        )
    }
}
