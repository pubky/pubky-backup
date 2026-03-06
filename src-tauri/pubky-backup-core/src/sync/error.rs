//! Sync module error types.

use crate::storage::StorageError;

/// Errors that can occur during event synchronization.
#[derive(thiserror::Error, Debug)]
pub enum SyncError {
    /// Storage operation failed during sync
    #[error("Storage error: {0}")]
    Storage(#[from] StorageError),

    /// Event stream error
    #[error("Events error: {0}")]
    Events(#[from] EventsError),

    /// Internal sync error
    #[error("Internal error: {0}")]
    Internal(String),
}

/// Errors that can occur when working with event streams.
#[derive(thiserror::Error, Debug)]
pub enum EventsError {
    /// Failed to fetch or subscribe to event stream
    #[error("Failed to fetch events: {0}")]
    FetchFailed(String),
}
