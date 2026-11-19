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

    /// The homeserver rejected the key's session
    #[error("The homeserver rejected the session")]
    SessionRejected,

    /// No response came from the homeserver: this device is offline, or the
    /// homeserver is down or unreachable. The details are logged.
    #[error("Could not reach the homeserver. Check your internet connection.")]
    Unreachable,

    /// The homeserver turned the request down for coming too often
    #[error(
        "The homeserver is limiting requests right now. The backup tries again at the next sync."
    )]
    RateLimited,

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
