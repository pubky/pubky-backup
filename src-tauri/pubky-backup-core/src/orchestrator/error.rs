//! Orchestrator module error types.

use crate::storage::StorageError;

/// Errors that can occur during backup orchestration.
#[derive(thiserror::Error, Debug)]
pub enum OrchestratorError {
    /// Attempted to add a key that is already being backed up
    #[error("Key {0} is already being backed up")]
    KeyAlreadyExists(String),

    /// Attempted to operate on a key that is not being backed up
    #[error("Key {0} is not being backed up")]
    KeyNotFound(String),

    /// Maximum number of keys reached
    #[error("Maximum number of keys ({0}) reached")]
    KeyLimitReached(usize),

    /// Failed to validate a pubky (homeserver discovery, data check)
    #[error("Failed to validate key: {0}")]
    ValidationFailed(String),

    /// Storage operation failed
    #[error("Storage error: {0}")]
    Storage(#[from] StorageError),

    /// Internal orchestration error
    #[error("Internal error: {0}")]
    Internal(String),
}
