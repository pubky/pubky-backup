use serde::{Serialize, Serializer};

/// Backup App Errors. Only these should be exposed to the front-end.
#[derive(thiserror::Error, Debug)]
pub enum BackupAppError {
    #[error("Internal error: {0}")]
    Internal(String),
    #[error("Failed to find Homeserver for pubky")]
    HomeserverNotFound,
    #[error("Failed to find data for pubky")]
    DataNotFound,
    #[error("Invalid pubky format: {0}")]
    InvalidPubkyFormat(String),
    #[error("Storage error: {0}")]
    Storage(#[from] StorageError),
    #[error("Events error: {0}")]
    Events(#[from] EventsError),
}

impl Serialize for BackupAppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl BackupAppError {
    pub fn internal<E: std::fmt::Display>(err: E) -> Self {
        Self::Internal(err.to_string())
    }

    pub fn lock_failed() -> Self {
        Self::Internal("Failed to acquire lock".to_string())
    }
}

#[derive(thiserror::Error, Debug)]
pub enum StorageError {
    #[error("Internal error: {0}")]
    Internal(String),
    #[error("OpenDAL error: {0}")]
    OpenDalError(#[from] opendal::Error),
    #[error("Invalid UTF-8: {0}")]
    InvalidUtf8(#[from] std::string::FromUtf8Error),
    #[error("Failed to create directory: {0}")]
    DirectoryCreation(String),
    #[error("{0}")]
    OperationFailed(Box<OperationFailedError>),
}

#[derive(Debug)]
pub struct OperationFailedError {
    pub operation: String,
    pub path: String,
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

#[derive(thiserror::Error, Debug)]
pub enum EventsError {
    #[error("Failed to fetch events: {0}")]
    FetchFailed(String),
    #[error("Invalid response: {0}")]
    InvalidResponse(String),
}
