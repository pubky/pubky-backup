//! Persistence layer for backup data and application state.
//!
//! This module provides a hierarchical storage abstraction for the backup application,
//! handling both application-level data and per-key backup data.
//!
//! # Components
//!
//! - [`AppStorage`] - Main storage facade providing unified access to all storage operations
//! - [`KeyStorage`] - Storage for a single pubky's backup data and state
//! - [`KeysStorage`] - Manager for multiple key storage instances
//! - [`migration`] - Legacy data detection and cleanup
//!
//! # Storage Layout
//!
//! ```text
//! ~/.pubky-backup/
//! ├── config.json                # App-level configuration
//! ├── logs/                      # App-level logs
//! │   └── error.log              # Global error log
//! └── keys/                      # Per-key data
//!     └── <pubky>/
//!         ├── state/             # Backup state/metadata
//!         │   ├── cursor         # Sync progress
//!         │   └── error.log      # Key-specific errors
//!         ├── data/              # Actual backed-up data
//!         │   └── pub/
//!         │       ├── profile.json
//!         │       └── ...
//!         └── snapshots/         # Point-in-time snapshots
//!             └── <timestamp>.zip
//! ```
//!
//! # Legacy Data Cleanup
//!
//! When [`AppStorage::new()`] is called, it detects old storage layouts and removes
//! them so the app starts fresh. All data is re-fetched from homeservers.
//! See the [`migration`] module for details.

mod app;
mod common;
pub(crate) mod config;
pub mod error;
mod keys;
mod migration;

pub use app::{get_data_directory, AppStorage};
pub use error::StorageError;
pub use keys::{KeyStorage, KeysStorage};
