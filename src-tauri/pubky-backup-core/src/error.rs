#[derive(thiserror::Error, Debug)]
pub enum SyncError {
    #[error("Storage error: {0}")]
    Storage(#[from] StorageError),
    #[error("Events error: {0}")]
    Events(#[from] EventsError),
    #[error("Internal error: {0}")]
    Internal(String),
    #[error("Authentication error: {0}")]
    Authentication(String),
    #[error("File watcher error: {0}")]
    FileWatcher(String),
    #[error("Upload error: {0}")]
    Upload(String),
    #[error("Delete error: {0}")]
    Delete(String),
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
