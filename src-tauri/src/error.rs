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
    Storage(#[from] pubky_backup_core::StorageError),
    #[error("Events error: {0}")]
    Events(#[from] pubky_backup_core::EventsError),
    #[error("Backup error: {0}")]
    Backup(#[from] pubky_backup_core::BackupError),
    #[error("Invalid .pkarr file: {0}")]
    InvalidPkarr(String),
    #[error("Private key does not match the selected public key")]
    PrivateKeyMismatch,
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
