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
//!
//! # Architecture
//!
//! The sync module is responsible for:
//! 1. Connecting to a pubky's homeserver event stream
//! 2. Processing PUT/DELETE events as they arrive
//! 3. Fetching resource data for PUT events
//! 4. Persisting data to storage via the storage module
//! 5. Tracking sync progress via cursors
//!
//! # Channel Architecture
//!
//! The controller uses two broadcast channels for communication with the orchestrator:
//! - `ControllerCommand`: Commands sent TO the controller (Cancel, ForceSync)
//! - `ControllerStatus`: Status updates sent FROM the controller (Syncing, Idle, Ended, Error)
//!
//! The [`BackupController`] runs as an async task and can be controlled via
//! message channels for cancellation and force-sync triggers.

mod controller;
pub mod error;
pub mod events;
pub mod fetcher;

pub use controller::{
    BackupController, ControllerCommand, ControllerStatus, SYNC_INTERVAL_SECONDS,
};
pub use error::SyncError;
