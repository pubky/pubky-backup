//! Event synchronization module.
//!
//! This module handles the core backup synchronization logic - streaming events
//! from a Pubky homeserver and persisting them to local storage.
//!
//! # Components
//!
//! - [`BackupController`] - The main sync loop that processes events for a single pubky
//! - [`events`] - Event stream creation and mock streams for testing
//! - [`fetcher`] - Resource data fetching with retry logic
//! - [`scope`] - The public and private scopes a key's data is synced in
//! - [`session`] - Restoring the session a signed-in key's private data is synced with
//!
//! # Architecture
//!
//! The sync module is responsible for:
//! 1. Connecting to a pubky's homeserver event stream
//! 2. Processing PUT/DELETE events as they arrive
//! 3. Fetching resource data for PUT events
//! 4. Persisting data to storage via the storage module
//! 5. Tracking sync progress via cursors, one per scope
//!
//! Each sync cycle syncs a key's public data, then, if a session secret is stored
//! for the key, its private data. A session the homeserver rejects is reported
//! with `ControllerStatus::SessionRejected` and not used again, while public data
//! keeps syncing.
//!
//! # Channel Architecture
//!
//! The controller uses two channels for communication with the orchestrator:
//! - `ControllerCommand`: Commands sent TO the controller (Cancel, ForceSync)
//! - `ControllerUpdate`: Status updates sent FROM the controller, each carrying
//!   the `ControllerStatus` (Syncing, Idle, Ended, Error, SessionRejected) of
//!   the pubky it names
//!
//! The [`BackupController`] runs as an async task and can be controlled via
//! message channels for cancellation and force-sync triggers.

mod controller;
pub mod error;
mod events;
mod fetcher;
mod scope;
pub(crate) mod session;

pub use controller::{
    BackupController, ControllerCommand, ControllerStatus, ControllerUpdate,
    DEFAULT_SYNC_INTERVAL_SECONDS, MIN_SYNC_INTERVAL_SECONDS,
};
pub use error::SyncError;
pub(crate) use scope::PRIVATE_PATH;
pub use session::SessionSecret;
