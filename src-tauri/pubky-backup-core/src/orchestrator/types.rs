//! Types for backup orchestration.
//!
//! This module contains configuration and state types used by the [`BackupManager`](super::BackupManager).

use pubky::{ClientId, PublicKey};
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
    /// Make no network calls (default: false).
    ///
    /// Offline, keys are not validated, syncs find no events and signing in is
    /// unavailable, so only the data already on disk is kept. For tests and
    /// for developing against the manager's behaviour without a network.
    pub offline: bool,
    /// Sync interval in seconds (default: 30)
    pub sync_interval_secs: u64,
    /// HTTP relay that delivers sign-in approvals from the user's signer app
    /// (default: the Pubky SDK's public relay)
    pub http_relay: Option<Url>,
    /// Name this app gives itself when asking to sign in, which the user sees
    /// among the apps they granted access to (default: `pubky-backup`)
    pub client_id: ClientId,
}

/// The default [`BackupManagerConfig::client_id`].
const DEFAULT_CLIENT_ID: &str = "pubky-backup";

impl Default for BackupManagerConfig {
    fn default() -> Self {
        Self {
            data_dir: None,
            validation_timeout_secs: 30,
            offline: false,
            sync_interval_secs: crate::sync::DEFAULT_SYNC_INTERVAL_SECONDS,
            http_relay: None,
            client_id: ClientId::new(DEFAULT_CLIENT_ID)
                .expect("the default client ID is a valid client ID"),
        }
    }
}

/// Observable state for a key (sent to frontends).
///
/// This struct contains all the information needed to display
/// the current status of a key's backup process.
#[derive(Clone, Debug, Serialize, PartialEq)]
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
    /// Whether the key is signed in, which decides what its backup covers
    pub auth: AuthStatus,
    /// Whether the key's data is also copied to a second homeserver
    pub mirror: MirrorStatus,
}

impl Default for KeyState {
    fn default() -> Self {
        Self {
            status: KeyStatus::Starting,
            data_size: 0,
            last_sync: None,
            next_sync: None,
            error: None,
            auth: AuthStatus::SignedOut,
            mirror: MirrorStatus::NotMirrored,
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

/// Whether a key's backed-up data is also copied to a mirror: a second
/// homeserver, which then holds the same data as the key's own.
///
/// What is backed up is what gets mirrored, so private data reaches the mirror
/// only while the key is signed in (see [`AuthStatus`]). Deletions are mirrored
/// too. The key must already have an account on the mirror. Set a mirror up
/// with [`BackupManager::start_mirror`](super::BackupManager::start_mirror).
///
/// Homeservers are given as their z32-encoded public keys.
#[derive(Clone, Debug, Default, Serialize, PartialEq)]
#[serde(tag = "type")]
pub enum MirrorStatus {
    /// The key has no mirror.
    #[default]
    NotMirrored,
    /// A mirror was requested and is waiting for the owner to approve write
    /// access to it in their signer app (e.g. Pubky Ring).
    AwaitingApproval {
        /// The requested mirror homeserver
        homeserver: String,
        /// The `pubkyauth://` link to open or scan with the signer app
        authorization_url: String,
    },
    /// Changes are copied to the mirror after every sync.
    Mirroring {
        /// The mirror homeserver
        homeserver: String,
    },
    /// The mirror no longer accepts the key's session. Nothing is copied until
    /// the owner approves again, after which copying resumes where it stopped.
    SessionExpired {
        /// The mirror homeserver
        homeserver: String,
    },
    /// Copying to the mirror is failing. It is retried on every sync and
    /// resumes where it stopped.
    Failing {
        /// The mirror homeserver
        homeserver: String,
        /// Human-readable reason
        message: String,
    },
    /// The last attempt to set the mirror up failed.
    SetupFailed {
        /// The requested mirror homeserver
        homeserver: String,
        /// Human-readable reason
        message: String,
    },
}

impl MirrorStatus {
    /// The mirror homeserver this status is about, if any.
    pub fn homeserver(&self) -> Option<&str> {
        match self {
            Self::NotMirrored => None,
            Self::AwaitingApproval { homeserver, .. }
            | Self::Mirroring { homeserver }
            | Self::SessionExpired { homeserver }
            | Self::Failing { homeserver, .. }
            | Self::SetupFailed { homeserver, .. } => Some(homeserver),
        }
    }

    /// Whether the key holds a session on its mirror and changes are copied to
    /// it, or at least attempted.
    pub fn is_active(&self) -> bool {
        matches!(self, Self::Mirroring { .. } | Self::Failing { .. })
    }
}

/// Structured error for frontend display.
///
/// Contains error details in a format suitable for
/// showing to users in the UI.
#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct KeyError {
    /// Human-readable error message
    pub message: String,
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
    MirrorStarted,
    MirrorStopped,
    MirrorSessionExpired,
    MirrorFailed,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config_has_the_default_client_id() {
        let config = BackupManagerConfig::default();

        assert_eq!(config.client_id.as_str(), DEFAULT_CLIENT_ID);
    }
}
