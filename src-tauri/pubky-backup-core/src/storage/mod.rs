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
//! - [`migration`] - Automatic migration from old storage structures
//!
//! # Storage Layout
//!
//! ```text
//! ~/.pubky-backup/
//! ├── config/                    # App-level configuration
//! │   └── last_pubky             # Last used pubky
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
//! # Automatic Migration
//!
//! When [`AppStorage::new()`] is called, it automatically detects and migrates data
//! from the old flat storage structure to the new hierarchical structure. See the
//! [`migration`] module for details.

mod app;
mod common;
pub mod error;
mod keys;
mod migration;

pub use app::{get_data_directory, AppStorage};
pub use error::StorageError;
pub use keys::{KeyStorage, KeysStorage};

// Re-export constants used by migration tests
pub(crate) use app::{
    CONFIG_DIR_NAME, CURSOR_FILENAME, DATA_DIR_NAME, ERROR_LOG_FILENAME, KEYS_DIR_NAME,
    LAST_PUBKY_FILENAME, LOGS_DIR_NAME, STATE_DIR_NAME,
};
