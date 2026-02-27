use serde::Serialize;

/// Backup App Errors. Only these should be exposed to the front-end.
#[derive(thiserror::Error, Debug, Serialize)]
#[serde(tag = "type")]
pub enum BackupAppError {
    #[error("Internal error: {message}")]
    Internal { message: String },
    #[error("Invalid pubky format: {message}")]
    InvalidPubkyFormat { message: String },
    #[error("Homeserver not found: {message}")]
    HomeserverNotFound { message: String },
    #[error("Storage error: {message}")]
    Storage { message: String },
    #[error("Events error: {message}")]
    Events { message: String },
    #[error("Backup error: {message}")]
    Backup { message: String },
}

impl BackupAppError {
    pub fn internal<E: std::fmt::Display>(err: E) -> Self {
        Self::Internal {
            message: err.to_string(),
        }
    }
}

impl From<pubky_backup_core::StorageError> for BackupAppError {
    fn from(err: pubky_backup_core::StorageError) -> Self {
        Self::Storage {
            message: err.to_string(),
        }
    }
}

impl From<pubky_backup_core::sync::error::EventsError> for BackupAppError {
    fn from(err: pubky_backup_core::sync::error::EventsError) -> Self {
        Self::Events {
            message: err.to_string(),
        }
    }
}

impl From<pubky_backup_core::SyncError> for BackupAppError {
    fn from(err: pubky_backup_core::SyncError) -> Self {
        Self::Backup {
            message: err.to_string(),
        }
    }
}

impl From<pubky_backup_core::OrchestratorError> for BackupAppError {
    fn from(err: pubky_backup_core::OrchestratorError) -> Self {
        use pubky_backup_core::OrchestratorError;
        match err {
            OrchestratorError::HomeserverNotFound(msg) => Self::HomeserverNotFound { message: msg },
            _ => Self::Internal {
                message: err.to_string(),
            },
        }
    }
}
