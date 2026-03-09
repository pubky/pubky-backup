//! Pubky Backup Core Library
//!
//! This library provides the core functionality for backing up Pubky data to local storage.
//! It handles synchronization of data from Pubky homeservers, storage management, and
//! multi-key backup orchestration.
//!
//! # Architecture
//!
//! The library is organized into three main modules:
//!
//! - [`sync`] - Event synchronization from homeservers for a single pubkey (streaming, processing, fetching)
//! - [`orchestrator`] - Multi-key backup management (validation, lifecycle, coordination)
//! - [`storage`] - Persistence layer (app data, per-key data, snapshots)
//!
//! # Quick Start
//!
//! For most use cases, use the [`BackupManager`] which handles everything:
//!
//! ```no_run
//! use pubky_backup_core::{BackupManager, BackupManagerConfig};
//! use pubky::PublicKey;
//! use std::str::FromStr;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // Create manager with default configuration
//!     let manager = BackupManager::new(BackupManagerConfig::default()).await?;
//!
//!     // Add a key to backup
//!     let pubky = PublicKey::from_str("your_pubky_here")?;
//!     manager.add_key(pubky.clone()).await?;
//!
//!     // Subscribe to status updates
//!     let mut rx = manager.subscribe();
//!     while let Ok(update) = rx.recv().await {
//!         println!("Key {:?} status: {:?}", update.pubky, update.state.status);
//!     }
//!
//!     Ok(())
//! }
//! ```
//!
//! # Low-Level API
//!
//! For more control, use [`BackupController`] directly:
//!
//! ```no_run
//! use pubky_backup_core::{AppStorage, BackupController, ControllerCommand, ControllerStatus};
//! use pubky::{Pubky, PublicKey};
//! use std::sync::Arc;
//! use std::str::FromStr;
//! use tokio::sync::{broadcast, mpsc};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let storage = Arc::new(AppStorage::new()?);
//!     let pubky_client = Arc::new(Pubky::new()?);
//!     let pubky = PublicKey::from_str("your_pubky_here")?;
//!
//!     let (_control_tx, control_rx) = mpsc::channel::<ControllerCommand>(5);
//!     let (status_tx, _status_rx) = broadcast::channel::<ControllerStatus>(5);
//!
//!     let controller = BackupController::new(
//!         pubky,
//!         storage,
//!         pubky_client,
//!         Some(control_rx),
//!         Some(status_tx),
//!     );
//!
//!     tokio::spawn(controller.run());
//!
//!     Ok(())
//! }
//! ```
//!
//! # Developer Mode (Offline/No-Network Mode)
//!
//! Enable developer mode by setting the `PUBKY_DEVELOPER_MODE` environment variable.
//! This skips network validation and returns empty data instead of making real network
//! calls, useful for offline development and testing with arbitrary pubky keys.

pub mod orchestrator;
pub mod storage;
pub mod sync;
mod utils;

// Re-export main types from orchestrator module
pub use orchestrator::{
    BackupManager, BackupManagerConfig, KeyError, KeyErrorCode, KeyState, KeyStatus, KeyUpdate,
    OrchestratorError,
};

// Re-export main types from storage module
pub use storage::{get_data_directory, AppStorage, StorageError};

// Re-export main types from sync module
pub use sync::{
    BackupController, ControllerCommand, ControllerStatus, SyncError,
    DEFAULT_SYNC_INTERVAL_SECONDS, MIN_SYNC_INTERVAL_SECONDS,
};

// Re-export utility functions
pub use utils::retry_with_backoff;

// Re-export SDK types used in our public API
pub use pubky::{Event, EventType};

use std::env;

/// Test-only pubky constant (for unit tests that need a valid pubky string)
#[cfg(test)]
pub const TEST_PUBKY: &str = "g1b6wp8bhhxtsksy3td7rj6mgg7s5k8c68663sajkfscshwj8g5y";

/// Check if developer mode is enabled via environment variable.
///
/// Developer mode skips network validation and returns empty data instead of
/// making real network calls, allowing offline development with arbitrary keys.
/// Enable by setting `PUBKY_DEVELOPER_MODE` environment variable.
///
/// # Example
///
/// ```bash
/// PUBKY_DEVELOPER_MODE=1 cargo run
/// ```
pub fn is_developer_mode() -> bool {
    env::var("PUBKY_DEVELOPER_MODE").is_ok()
}
