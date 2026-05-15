//! Multi-key backup orchestration module.
//!
//! This module provides high-level management of multiple pubky backups,
//! handling key lifecycle, validation, and coordination of sync controllers.
//!
//! # Components
//!
//! - [`BackupManager`] - Thread-safe manager for multiple pubky backups
//! - [`types`] - Configuration and state types for the manager
//! - [`discovery`] - Homeserver discovery and pubky validation
//! - [`session`] - Session persistence (last used pubky)
//!
//! # Architecture
//!
//! The orchestrator module sits above the sync module, managing:
//! 1. Adding/removing pubky keys for backup
//! 2. Validating pubkys (homeserver discovery, data existence)
//! 3. Spawning and controlling [`BackupController`](crate::sync::BackupController) instances
//! 4. Broadcasting status updates to subscribers via [`KeyUpdate`]
//! 5. Automatic resumption of backups from stored data
//! 6. Concurrency limiting for sync operations
//!
//! # Channel Architecture
//!
//! The orchestrator communicates with sync controllers via:
//! - [`ControllerCommand`](crate::sync::ControllerCommand): Commands sent to controllers
//! - [`ControllerStatus`](crate::sync::ControllerStatus): Status updates from controllers
//!
//! It then transforms these into [`KeyUpdate`] messages for external consumers.
//!
//! # Example
//!
//! ```no_run
//! use pubky_backup_core::{BackupManager, BackupManagerConfig};
//! use pubky::PublicKey;
//! use std::str::FromStr;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let config = BackupManagerConfig::default();
//!     let manager = BackupManager::new(config).await?;
//!
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

pub mod discovery;
pub mod error;
mod manager;
pub mod session;
mod status;
mod sync_interval;
pub mod types;

pub use error::OrchestratorError;
pub use manager::{BackupManager, MAX_KEYS};
pub use types::{
    ActivityEntry, ActivityType, BackupManagerConfig, KeyError, KeyErrorCode, KeyState, KeyStatus,
    KeyUpdate,
};
