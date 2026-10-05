//! Types for backup orchestration.
//!
//! This module contains configuration and state types used by the [`BackupManager`](super::BackupManager).

use pubky::PublicKey;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use url::Url;

/// Configuration for the [`BackupManager`](super::BackupManager).
#[derive(Clone, Debug)]
pub struct BackupManagerConfig {
    /// Data directory path (default: ~/.pubky-backup)
    pub data_dir: Option<PathBuf>,
    /// Timeout for key validation/homeserver discovery in seconds (default: 30)
    pub validation_timeout_secs: u64,
    /// Enable developer mode (default: false)
    pub developer_mode: bool,
    /// Sync interval in seconds (default: 30)
    pub sync_interval_secs: u64,
    /// HTTP relay that delivers sign-in approvals from the user's signer app
    /// (default: the Pubky SDK's public relay)
    pub http_relay: Option<Url>,
}

impl Default for BackupManagerConfig {
    fn default() -> Self {
        Self {
            data_dir: None,
            validation_timeout_secs: 30,
            developer_mode: false,
            sync_interval_secs: crate::sync::DEFAULT_SYNC_INTERVAL_SECONDS,
            http_relay: None,
        }
    }
}

/// Observable state for a key (sent to frontends).
///
/// This struct contains all the information needed to display
/// the current status of a key's backup process.
#[derive(Clone, Debug, Serialize)]
pub struct KeyState {
    /// Current status of the key backup
    pub status: KeyStatus,
    /// Total size of backed up data in bytes
    pub data_size: u64,
    /// Unix timestamp of last successful sync
    pub last_sync: Option<u64>,
    /// Unix timestamp of next scheduled sync
    pub next_sync: Option<u64>,
    /// Error information if status is Error
    pub error: Option<KeyError>,
    /// Total number of files being backed up (if known)
    pub total_files: Option<u64>,
    /// Number of files synced so far
    pub files_synced: Option<u64>,
    /// Total bytes downloaded in current sync
    pub bytes_downloaded: Option<u64>,
    /// Whether the key is signed in, which decides what its backup covers
    pub auth: AuthStatus,
}

impl Default for KeyState {
    fn default() -> Self {
        Self {
            status: KeyStatus::Starting,
            data_size: 0,
            last_sync: None,
            next_sync: None,
            error: None,
            total_files: None,
            files_synced: None,
            bytes_downloaded: None,
            auth: AuthStatus::SignedOut,
        }
    }
}

/// Whether a key is signed in, which decides what its backup covers.
///
/// Anyone can back up a key's public data (`/pub`). Private data (`/priv`) is
/// only readable by the key's owner, so it is backed up only while signed in.
/// Sign in with [`BackupManager::start_sign_in`](super::BackupManager::start_sign_in).
#[derive(Clone, Debug, Default, Serialize, PartialEq)]
#[serde(tag = "type")]
pub enum AuthStatus {
    /// Not signed in. Only public data is backed up.
    #[default]
    SignedOut,
    /// A sign-in was started and is waiting for the owner to approve it in
    /// their signer app (e.g. Pubky Ring). Only public data is backed up meanwhile.
    AwaitingApproval {
        /// The `pubkyauth://` link to open or scan with the signer app
        authorization_url: String,
    },
    /// Signed in. Private data is backed up as well as public data.
    SignedIn,
    /// The homeserver no longer accepts the session. Only public data is
    /// backed up until the owner signs in again.
    SessionExpired,
    /// The last sign-in attempt failed. Only public data is backed up.
    SignInFailed {
        /// Human-readable reason
        message: String,
    },
}

/// Structured error for frontend display.
///
/// Contains error details in a format suitable for
/// showing to users in the UI.
#[derive(Clone, Debug, Serialize)]
pub struct KeyError {
    /// Error classification
    pub code: KeyErrorCode,
    /// Human-readable error message
    pub message: String,
    /// Whether this error is likely to resolve on retry
    pub recoverable: bool,
}

/// Error codes for key-specific errors.
///
/// These codes help the frontend display appropriate
/// error messages and recovery options.
#[derive(Clone, Debug, Serialize, PartialEq)]
pub enum KeyErrorCode {
    /// Homeserver is not responding
    HomeserverUnreachable,
    /// Could not discover homeserver for the pubky
    HomeserverNotFound,
    /// No data found for this pubky
    NoDataFound,
    /// Local storage is full
    StorageFull,
    /// Network connectivity issue
    NetworkError,
    /// Operation timed out
    Timeout,
    /// Internal error
    Internal,
}

/// Status of a key's backup process.
#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(tag = "type")]
pub enum KeyStatus {
    /// Key is starting up (validating, discovering homeserver)
    Starting,
    /// Key is actively syncing data
    Syncing {
        /// Number of events processed in current sync batch
        events_processed: usize,
    },
    /// Key is idle, waiting for next sync interval
    Idle,
    /// Key backup has been stopped
    Stopped,
    /// Key backup encountered an error
    Error,
}

/// Emitted when a key's state changes.
///
/// This is the public API for status updates, consumed by UI/external subscribers.
/// Subscribe to these updates via [`BackupManager::subscribe`](super::BackupManager::subscribe).
///
/// Note: This is distinct from [`ControllerStatus`](crate::sync::ControllerStatus) which is
/// the internal status type used for communication between the sync and orchestrator layers.
#[derive(Clone, Debug, Serialize)]
pub struct KeyUpdate {
    /// The public key this update is for (serialized as string)
    #[serde(serialize_with = "serialize_pubky")]
    pub pubky: PublicKey,
    /// The new state of the key
    pub state: KeyState,
}

/// Type of activity event logged for a key.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ActivityType {
    FilesBackedUp,
    InitialBackup,
    SnapshotCreated,
    SyncFailed,
    SignedIn,
    SignedOut,
    SessionExpired,
}

/// A single activity log entry for a key.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ActivityEntry {
    #[serde(rename = "type")]
    pub activity_type: ActivityType,
    pub message: String,
    pub timestamp: u64,
}

fn serialize_pubky<S>(pubky: &PublicKey, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    serializer.serialize_str(&pubky.z32())
}
