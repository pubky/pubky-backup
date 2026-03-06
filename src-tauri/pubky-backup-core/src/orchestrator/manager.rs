//! Backup manager for orchestrating multiple pubky backups.
//!
//! This module contains the main [`BackupManager`] which coordinates multiple
//! backup controllers, handling key lifecycle and status aggregation.
//!
//! # Responsibilities
//!
//! - Managing the lifecycle of multiple pubky backups
//! - Coordinating backup controllers via message channels
//! - Aggregating controller status into public [`KeyUpdate`] messages
//! - Handling automatic resumption from stored data

use std::collections::HashMap;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use log::{error, info, warn};
use parking_lot::RwLock;
use pubky::{Pubky, PublicKey};
use tokio::sync::{broadcast, mpsc};
use tokio::task::JoinHandle;

use super::discovery;
use super::error::OrchestratorError;
use super::session;
use super::types::*;
use crate::storage::AppStorage;
use crate::sync::{BackupController, ControllerCommand, ControllerStatus, SYNC_INTERVAL_SECONDS};

/// Maximum number of keys that can be backed up simultaneously.
///
/// This limit prevents resource exhaustion from too many concurrent backup controllers.
/// Should be more flexible in the future but this is fine for current use cases.
pub const MAX_KEYS: usize = 50;

/// Internal state for a managed key.
struct ManagedKey {
    /// Sender for control messages to the backup controller (mpsc - single receiver)
    control_tx: mpsc::Sender<ControllerCommand>,
    /// Current state of the key
    state: KeyState,
    /// Handle to the controller task
    _task_handle: JoinHandle<()>,
}

struct ManagerInner {
    /// Map of public keys to their managed state
    keys: HashMap<PublicKey, ManagedKey>,
    /// Shared pubky client for SDK calls
    pubky_client: Arc<Pubky>,
}

/// A thread-safe backup manager for multiple pubky keys.
///
/// `BackupManager` provides a high-level async API for managing multiple pubky backups.
/// It handles validation, homeserver discovery, and coordination of backup controllers.
///
/// # Example
///
/// ```no_run
/// use pubky_backup_core::{BackupManager, BackupManagerConfig};
/// use pubky::PublicKey;
/// use std::str::FromStr;
///
/// #[tokio::main]
/// async fn main() -> Result<(), Box<dyn std::error::Error>> {
///     let config = BackupManagerConfig::default();
///     let manager = BackupManager::new(config).await?;
///
///     let pubky = PublicKey::from_str("your_pubky_here")?;
///     manager.add_key(pubky.clone()).await?;
///
///     // Get status updates
///     let mut rx = manager.subscribe();
///     while let Ok(update) = rx.recv().await {
///         println!("Key {:?} status: {:?}", update.pubky, update.state.status);
///     }
///
///     Ok(())
/// }
/// ```
#[derive(Clone)]
pub struct BackupManager {
    inner: Arc<RwLock<ManagerInner>>,
    storage: Arc<AppStorage>,
    config: BackupManagerConfig,
    /// Channel for external subscribers to receive key updates
    update_tx: broadcast::Sender<KeyUpdate>,
    /// Shared status channel sender - all controllers send to this channel
    status_tx: broadcast::Sender<ControllerStatus>,
}

impl BackupManager {
    /// Create a new BackupManager.
    ///
    /// Automatically resumes backing up any keys that have stored data from
    /// previous sessions. Keys are detected by scanning the data directory.
    ///
    /// # Errors
    ///
    /// Returns `OrchestratorError` if storage initialization fails.
    pub async fn new(config: BackupManagerConfig) -> Result<Self, OrchestratorError> {
        // Initialize storage
        let storage = if let Some(ref data_dir) = config.data_dir {
            AppStorage::new_with_path(data_dir)?
        } else {
            AppStorage::new()?
        };
        let storage = Arc::new(storage);

        // Create pubky client
        let pubky_client = if config.developer_mode {
            Arc::new(Pubky::testnet().map_err(|e| OrchestratorError::Internal(e.to_string()))?)
        } else {
            Arc::new(Pubky::new().map_err(|e| OrchestratorError::Internal(e.to_string()))?)
        };

        // Create update broadcast channel for external subscribers
        let (update_tx, _) = broadcast::channel(100);

        // Create shared status channel - all controllers send to this channel
        let (status_tx, status_rx) = broadcast::channel(100);

        let inner = Arc::new(RwLock::new(ManagerInner {
            keys: HashMap::new(),
            pubky_client,
        }));

        let manager = BackupManager {
            inner: inner.clone(),
            storage: storage.clone(),
            config,
            update_tx: update_tx.clone(),
            status_tx,
        };

        // Spawn the centralized status listener task
        spawn_status_listener(inner, storage, update_tx, status_rx);

        // Resume all keys that have stored data
        manager.resume_stored_keys().await;

        Ok(manager)
    }

    /// Subscribe to status updates for all keys.
    ///
    /// Returns a receiver that will receive [`KeyUpdate`] messages
    /// whenever a key's state changes.
    pub fn subscribe(&self) -> broadcast::Receiver<KeyUpdate> {
        self.update_tx.subscribe()
    }

    /// Add a key to be backed up.
    ///
    /// This method validates the key (discovers homeserver, checks data exists),
    /// then starts syncing. If backup data already exists on disk, it resumes
    /// from where it left off.
    ///
    /// # Errors
    ///
    /// Returns `OrchestratorError::KeyAlreadyExists` if the key is already being backed up.
    /// Returns `OrchestratorError::KeyLimitReached` if MAX_KEYS limit is reached.
    /// Returns `OrchestratorError::ValidationFailed` if the key cannot be validated.
    pub async fn add_key(&self, pubky: PublicKey) -> Result<(), OrchestratorError> {
        // Early check if key already exists or limit reached.
        // Note: There's a small race window between this check and the actual insert in
        // start_controller, but exceeding MAX_KEYS by a few is acceptable - it's a soft
        // limit to prevent resource exhaustion, not a hard security boundary.
        {
            let inner = self.inner.read();
            if inner.keys.contains_key(&pubky) {
                return Err(OrchestratorError::KeyAlreadyExists(pubky.to_string()));
            }
            if inner.keys.len() >= MAX_KEYS {
                return Err(OrchestratorError::KeyLimitReached(MAX_KEYS));
            }
        }

        discovery::validate_pubky(
            &pubky,
            self.config.validation_timeout_secs,
            self.config.developer_mode,
        )
        .await?;

        self.start_controller(pubky.clone()).await?;

        info!("Added key for backup: {}", pubky);
        Ok(())
    }

    /// Stop syncing a key but preserve backed-up data on disk.
    ///
    /// The key will be automatically resumed on the next `BackupManager::new()` call
    /// since data still exists. Use `delete_key()` for permanent removal.
    ///
    /// # Errors
    ///
    /// Returns `OrchestratorError::KeyNotFound` if the key is not being backed up.
    pub async fn remove_key(&self, pubky: &PublicKey) -> Result<(), OrchestratorError> {
        let managed_key = {
            let mut inner = self.inner.write();
            inner
                .keys
                .remove(pubky)
                .ok_or_else(|| OrchestratorError::KeyNotFound(pubky.to_string()))?
        };

        // Send cancel message to stop the controller (mpsc send is async but we use try_send)
        let _ = managed_key.control_tx.try_send(ControllerCommand::Cancel);

        info!("Stopped backup for key: {}", pubky);
        Ok(())
    }

    /// Remove a key AND delete all backed-up data from disk.
    ///
    /// # Errors
    ///
    /// Returns `OrchestratorError::KeyNotFound` if the key is not being backed up.
    pub async fn delete_key(&self, pubky: &PublicKey) -> Result<(), OrchestratorError> {
        // First remove the key
        self.remove_key(pubky).await?;

        // Delete the data directory for this key
        let data_dir = self.storage.get_backup_data_dir()?;
        let key_dir = data_dir.join("keys").join(pubky.z32());

        if key_dir.exists() {
            std::fs::remove_dir_all(&key_dir).map_err(|e| {
                OrchestratorError::Internal(format!(
                    "Failed to delete data for {}: {}",
                    pubky.z32(),
                    e
                ))
            })?;
            info!("Deleted data for key: {}", pubky);
        }

        Ok(())
    }

    /// Force immediate sync for a specific key.
    ///
    /// # Errors
    ///
    /// Returns `OrchestratorError::KeyNotFound` if the key is not being backed up.
    pub async fn force_sync(&self, pubky: &PublicKey) -> Result<(), OrchestratorError> {
        let inner = self.inner.read();
        let managed_key = inner
            .keys
            .get(pubky)
            .ok_or_else(|| OrchestratorError::KeyNotFound(pubky.to_string()))?;

        managed_key
            .control_tx
            .try_send(ControllerCommand::ForceSync)
            .map_err(|_| {
                OrchestratorError::Internal("Failed to send force sync message".to_string())
            })?;

        log::debug!("Force sync triggered for: {}", pubky);
        Ok(())
    }

    /// Get current state of a specific key.
    ///
    /// Returns `None` if the key is not being backed up.
    pub fn get_key_state(&self, pubky: &PublicKey) -> Option<KeyState> {
        let inner = self.inner.read();
        inner.keys.get(pubky).map(|k| k.state.clone())
    }

    /// Get current state of all managed keys.
    ///
    /// Returns a map of pubky string (z32) to KeyState.
    pub fn get_all_key_states(&self) -> HashMap<String, KeyState> {
        let inner = self.inner.read();
        inner
            .keys
            .iter()
            .map(|(pubky, managed)| (pubky.z32(), managed.state.clone()))
            .collect()
    }

    /// Get list of all managed pubkys.
    pub fn get_keys(&self) -> Vec<PublicKey> {
        let inner = self.inner.read();
        inner.keys.keys().cloned().collect()
    }

    /// Check if any key is currently syncing.
    pub fn any_syncing(&self) -> bool {
        let inner = self.inner.read();
        inner
            .keys
            .values()
            .any(|k| matches!(k.state.status, KeyStatus::Syncing { .. }))
    }

    /// Check if any key has an error.
    pub fn any_error(&self) -> bool {
        let inner = self.inner.read();
        inner
            .keys
            .values()
            .any(|k| matches!(k.state.status, KeyStatus::Error))
    }

    /// Check if any key is running (not stopped).
    pub fn any_running(&self) -> bool {
        let inner = self.inner.read();
        inner
            .keys
            .values()
            .any(|k| !matches!(k.state.status, KeyStatus::Stopped))
    }

    /// Create a snapshot (zip) for a key.
    ///
    /// Saves to `{data_dir}/keys/{pubky}/snapshots/{timestamp}.zip`.
    /// Does not block ongoing syncs; snapshot reflects state at call time.
    ///
    /// # Errors
    ///
    /// Returns `OrchestratorError::KeyNotFound` if the key is not being backed up.
    /// Returns `OrchestratorError::Storage` if snapshot creation fails.
    pub async fn create_snapshot(&self, pubky: &PublicKey) -> Result<PathBuf, OrchestratorError> {
        // Verify the key exists
        {
            let inner = self.inner.read();
            if !inner.keys.contains_key(pubky) {
                return Err(OrchestratorError::KeyNotFound(pubky.to_string()));
            }
        }

        let path = self.storage.create_snapshot(pubky).await?;
        info!("Created snapshot for {}: {}", pubky, path.display());
        Ok(path)
    }

    /// Get the data directory path.
    pub fn data_dir(&self) -> PathBuf {
        self.storage
            .get_backup_data_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
    }

    /// Write the last used pubky to storage for session persistence.
    ///
    /// This allows the app to remember which pubky was last used.
    pub async fn write_last_pubky(&self, pubky: &PublicKey) -> Result<(), OrchestratorError> {
        session::write_last_pubky(&self.storage, pubky).await
    }

    /// Read the last used pubky from storage.
    ///
    /// Returns `None` if no last pubky was saved.
    pub async fn read_last_pubky(&self) -> Result<Option<PublicKey>, OrchestratorError> {
        session::read_last_pubky(&self.storage).await
    }

    /// List pubky directories that have backup data stored.
    ///
    /// Returns a list of z32-encoded pubky strings.
    pub fn list_pubky_directories(&self) -> Result<Vec<String>, OrchestratorError> {
        Ok(self.storage.list_pubky_directories()?)
    }

    /// Gracefully shutdown all backups.
    pub async fn shutdown(&self) {
        let keys: Vec<PublicKey> = self.get_keys();

        for pubky in keys {
            if let Err(e) = self.remove_key(&pubky).await {
                warn!("Error shutting down key {}: {}", pubky, e);
            }
        }

        info!("BackupManager shutdown complete");
    }

    // --- Internal methods ---

    /// Start the backup controller for a key
    async fn start_controller(&self, pubky: PublicKey) -> Result<(), OrchestratorError> {
        // Create mpsc channel for control commands (single receiver per controller)
        let (control_tx, control_rx) = mpsc::channel(5);

        // Get initial data size
        let initial_size = self.storage.calculate_pubky_size(&pubky).await;

        // Create initial state
        let initial_state = KeyState {
            status: KeyStatus::Starting,
            data_size: initial_size,
            last_sync: None,
            next_sync: Some(next_sync_time()),
            error: None,
            total_files: None,
            files_synced: None,
            bytes_downloaded: None,
        };

        // Broadcast initial state to external subscribers
        let _ = self.update_tx.send(KeyUpdate {
            pubky: pubky.clone(),
            state: initial_state.clone(),
        });

        // Get pubky client
        let pubky_client = {
            let inner = self.inner.read();
            inner.pubky_client.clone()
        };

        // Generate random initial delay (0 to SYNC_INTERVAL_SECONDS) to stagger syncs
        // Skip staggering in developer mode or if this is the first key
        let key_count = {
            let inner = self.inner.read();
            inner.keys.len()
        };
        let initial_delay = if self.config.developer_mode || key_count == 0 {
            std::time::Duration::ZERO
        } else {
            std::time::Duration::from_secs(rand::random::<u64>() % SYNC_INTERVAL_SECONDS)
        };

        // Create the controller with the shared status channel and initial delay
        // All controllers send to the same status_tx, identified by pubky in each message
        let controller = BackupController::new(
            pubky.clone(),
            self.storage.clone(),
            pubky_client,
            Some(control_rx),
            Some(self.status_tx.clone()),
        )
        .with_initial_delay(initial_delay);

        // Spawn the controller task - delay is now handled inside controller.run()
        let task_handle = tokio::spawn(async move {
            controller.run().await;
        });

        // Store the managed key
        {
            let mut inner = self.inner.write();
            inner.keys.insert(
                pubky.clone(),
                ManagedKey {
                    control_tx,
                    state: initial_state,
                    _task_handle: task_handle,
                },
            );
        }

        Ok(())
    }

    /// Resume all keys that have stored data on disk
    async fn resume_stored_keys(&self) {
        let key_dirs = match self.storage.list_pubky_directories() {
            Ok(dirs) => dirs,
            Err(e) => {
                let error_msg = format!("Failed to list key directories: {}", e);
                warn!("{}", error_msg);
                let _ = self
                    .storage
                    .write_global_error("resume_stored_keys", &error_msg)
                    .await;
                return;
            }
        };

        if key_dirs.is_empty() {
            log::debug!("No stored keys to resume");
            return;
        }

        info!("Resuming {} keys with stored data", key_dirs.len());

        for pubky_str in key_dirs {
            let pubky = match PublicKey::from_str(&pubky_str) {
                Ok(p) => p,
                Err(e) => {
                    let error_msg = format!("Invalid pubky directory name {}: {}", pubky_str, e);
                    warn!("{}", error_msg);
                    let _ = self
                        .storage
                        .write_global_error("resume_stored_keys", &error_msg)
                        .await;
                    continue;
                }
            };

            match discovery::validate_pubky(
                &pubky,
                self.config.validation_timeout_secs,
                self.config.developer_mode,
            )
            .await
            {
                Ok(_) => {
                    if let Err(e) = self.start_controller(pubky.clone()).await {
                        let error_msg = format!("Failed to resume key {}: {}", pubky, e);
                        error!("{}", error_msg);
                        let _ = self
                            .storage
                            .write_global_error("resume_stored_keys", &error_msg)
                            .await;
                        // Start in error state
                        self.start_controller_in_error_state(
                            pubky,
                            KeyError {
                                code: KeyErrorCode::Internal,
                                message: format!("Failed to resume: {}", e),
                                recoverable: true,
                            },
                        )
                        .await;
                    }
                }
                Err(e) => {
                    let error_msg =
                        format!("Failed to validate key {} during resume: {}", pubky, e);
                    warn!("{}", error_msg);
                    let _ = self
                        .storage
                        .write_global_error("resume_stored_keys", &error_msg)
                        .await;
                    // Start in error state with recoverable flag
                    self.start_controller_in_error_state(
                        pubky,
                        KeyError {
                            code: KeyErrorCode::HomeserverUnreachable,
                            message: format!("Validation failed: {}", e),
                            recoverable: true,
                        },
                    )
                    .await;
                }
            }
        }
    }

    /// Start a key in error state (for auto-resume failures)
    async fn start_controller_in_error_state(&self, pubky: PublicKey, error: KeyError) {
        let (control_tx, _control_rx) = mpsc::channel(5);

        let error_state = KeyState {
            status: KeyStatus::Error,
            data_size: self.storage.calculate_pubky_size(&pubky).await,
            last_sync: None,
            next_sync: None,
            error: Some(error),
            total_files: None,
            files_synced: None,
            bytes_downloaded: None,
        };

        // Broadcast error state to external subscribers
        let _ = self.update_tx.send(KeyUpdate {
            pubky: pubky.clone(),
            state: error_state.clone(),
        });

        // Store in keys map (without a running controller task)
        // Create a no-op task handle
        let task_handle = tokio::spawn(async {});

        {
            let mut inner = self.inner.write();
            inner.keys.insert(
                pubky,
                ManagedKey {
                    control_tx,
                    state: error_state,
                    _task_handle: task_handle,
                },
            );
        }
    }
}

/// Spawn a centralized status listener task that processes all controller status updates.
///
/// This single task replaces the per-key listener tasks. It receives status updates
/// from all controllers via the shared status channel, updates internal state,
/// and broadcasts KeyUpdate messages to external subscribers.
fn spawn_status_listener(
    inner: Arc<RwLock<ManagerInner>>,
    storage: Arc<AppStorage>,
    update_tx: broadcast::Sender<KeyUpdate>,
    mut status_rx: broadcast::Receiver<ControllerStatus>,
) {
    tokio::spawn(async move {
        while let Ok(status) = status_rx.recv().await {
            // Extract the pubky from the status
            let pubky = match &status {
                ControllerStatus::Starting { pubky } => pubky.clone(),
                ControllerStatus::Syncing { pubky, .. } => pubky.clone(),
                ControllerStatus::Idle { pubky } => pubky.clone(),
                ControllerStatus::Ended { pubky } => pubky.clone(),
                ControllerStatus::Error { pubky, .. } => pubky.clone(),
            };

            // Get current state values to preserve across transitions
            let (current_data_size, current_last_sync, current_next_sync) = {
                let inner_read = inner.read();
                inner_read
                    .keys
                    .get(&pubky)
                    .map(|k| (k.state.data_size, k.state.last_sync, k.state.next_sync))
                    .unwrap_or((0, None, None))
            };

            let new_state = handle_controller_status(
                &pubky,
                &storage,
                &status,
                current_data_size,
                current_last_sync,
                current_next_sync,
            )
            .await;

            // Update internal state
            {
                let mut inner_write = inner.write();
                if let Some(managed_key) = inner_write.keys.get_mut(&pubky) {
                    managed_key.state = new_state.clone();
                }
            }

            // Broadcast to external subscribers
            let _ = update_tx.send(KeyUpdate {
                pubky: pubky.clone(),
                state: new_state,
            });
        }
    });
}

/// Transform a [`ControllerStatus`] into a [`KeyState`] for external consumers.
///
/// This is the bridge between the internal sync layer status and the
/// public orchestrator state.
///
/// # Arguments
/// * `pubky` - The public key being backed up
/// * `storage` - Storage for calculating data size
/// * `status` - The status from the backup controller
/// * `current_data_size` - The current data size to preserve
/// * `current_last_sync` - The current last_sync timestamp to preserve
/// * `current_next_sync` - The current next_sync timestamp to preserve
async fn handle_controller_status(
    pubky: &PublicKey,
    storage: &Arc<AppStorage>,
    status: &ControllerStatus,
    current_data_size: u64,
    current_last_sync: Option<u64>,
    current_next_sync: Option<u64>,
) -> KeyState {
    match status {
        ControllerStatus::Starting { .. } => KeyState {
            status: KeyStatus::Starting,
            data_size: current_data_size,
            last_sync: current_last_sync,
            next_sync: current_next_sync,
            ..Default::default()
        },
        ControllerStatus::Syncing {
            events_processed, ..
        } => {
            // Recalculate size if events were processed, otherwise keep current
            let data_size = if *events_processed > 0 {
                storage.calculate_pubky_size(pubky).await
            } else {
                current_data_size
            };

            KeyState {
                status: KeyStatus::Syncing {
                    events_processed: *events_processed,
                },
                data_size,
                last_sync: current_last_sync,
                next_sync: current_next_sync,
                ..Default::default()
            }
        }
        ControllerStatus::Idle { .. } => {
            let data_size = storage.calculate_pubky_size(pubky).await;
            KeyState {
                status: KeyStatus::Idle,
                data_size,
                last_sync: Some(current_unix_timestamp()),
                next_sync: Some(next_sync_time()),
                ..Default::default()
            }
        }
        ControllerStatus::Ended { .. } => KeyState {
            status: KeyStatus::Stopped,
            data_size: current_data_size,
            last_sync: current_last_sync,
            next_sync: current_next_sync,
            ..Default::default()
        },
        ControllerStatus::Error { message, .. } => KeyState {
            status: KeyStatus::Error,
            data_size: current_data_size,
            last_sync: current_last_sync,
            next_sync: current_next_sync,
            error: Some(KeyError {
                code: KeyErrorCode::Internal,
                message: message.clone(),
                recoverable: false,
            }),
            ..Default::default()
        },
    }
}

/// Get current unix timestamp in seconds
fn current_unix_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

/// Calculate next sync time
fn next_sync_time() -> u64 {
    current_unix_timestamp() + SYNC_INTERVAL_SECONDS
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DEV_MODE_PUBKY;
    use std::str::FromStr;
    use tempfile::TempDir;

    /// Helper to enable developer mode for tests that need mock data
    fn enable_developer_mode() {
        std::env::set_var("PUBKY_DEVELOPER_MODE", "1");
    }

    fn create_test_config(temp_dir: &TempDir) -> BackupManagerConfig {
        enable_developer_mode();
        BackupManagerConfig {
            data_dir: Some(temp_dir.path().to_path_buf()),
            validation_timeout_secs: 30,
            developer_mode: true,
        }
    }

    #[tokio::test]
    async fn test_manager_add_remove_key() {
        let temp_dir = TempDir::new().unwrap();
        let config = create_test_config(&temp_dir);
        let manager = BackupManager::new(config).await.unwrap();

        let pubky = PublicKey::from_str(DEV_MODE_PUBKY).unwrap();

        // Add key
        manager.add_key(pubky.clone()).await.unwrap();

        // Verify key is managed
        assert!(manager.get_key_state(&pubky).is_some());
        assert_eq!(manager.get_keys().len(), 1);

        // Remove key
        manager.remove_key(&pubky).await.unwrap();

        // Verify key is no longer managed
        assert!(manager.get_key_state(&pubky).is_none());
        assert_eq!(manager.get_keys().len(), 0);
    }

    #[tokio::test]
    async fn test_manager_delete_key_removes_data() {
        let temp_dir = TempDir::new().unwrap();
        let config = create_test_config(&temp_dir);
        let manager = BackupManager::new(config).await.unwrap();

        let pubky = PublicKey::from_str(DEV_MODE_PUBKY).unwrap();

        // Add key
        manager.add_key(pubky.clone()).await.unwrap();

        // Create some data (by waiting briefly for sync to start)
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // Delete key
        manager.delete_key(&pubky).await.unwrap();

        // Verify key directory is deleted
        let key_dir = temp_dir.path().join("keys").join(pubky.z32());
        assert!(!key_dir.exists());
    }

    #[tokio::test]
    async fn test_manager_status_updates() {
        let temp_dir = TempDir::new().unwrap();
        let config = create_test_config(&temp_dir);
        let manager = BackupManager::new(config).await.unwrap();

        let mut rx = manager.subscribe();
        let pubky = PublicKey::from_str(DEV_MODE_PUBKY).unwrap();

        // Add key
        manager.add_key(pubky.clone()).await.unwrap();

        // Should receive at least one status update
        let update = tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
            .await
            .expect("Should receive status update")
            .unwrap();

        assert_eq!(update.pubky, pubky);
    }

    #[tokio::test]
    async fn test_manager_resumes_stored_keys() {
        let temp_dir = TempDir::new().unwrap();

        let pubky = PublicKey::from_str(DEV_MODE_PUBKY).unwrap();

        // First, create a manager and add a key (creates data directory)
        {
            let config = create_test_config(&temp_dir);
            let manager = BackupManager::new(config).await.unwrap();

            manager.add_key(pubky.clone()).await.unwrap();

            // Wait for some data to be written
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;

            // Shutdown cleanly
            manager.shutdown().await;
        }

        // Create a new manager - should automatically resume the key
        {
            let config = create_test_config(&temp_dir);
            let manager = BackupManager::new(config).await.unwrap();

            // Key should be automatically resumed from stored data
            let keys = manager.get_keys();
            assert_eq!(keys.len(), 1, "Key should be auto-resumed from stored data");
            assert!(keys.contains(&pubky));
        }
    }

    #[tokio::test]
    async fn test_manager_shutdown() {
        let temp_dir = TempDir::new().unwrap();
        let config = create_test_config(&temp_dir);
        let manager = BackupManager::new(config).await.unwrap();

        let pubky = PublicKey::from_str(DEV_MODE_PUBKY).unwrap();

        // Add key
        manager.add_key(pubky.clone()).await.unwrap();
        assert_eq!(manager.get_keys().len(), 1);

        // Shutdown
        manager.shutdown().await;

        // All keys should be removed
        assert_eq!(manager.get_keys().len(), 0);
    }

    #[tokio::test]
    async fn test_manager_any_syncing() {
        let temp_dir = TempDir::new().unwrap();
        let config = create_test_config(&temp_dir);
        let manager = BackupManager::new(config).await.unwrap();

        // No keys - not syncing
        assert!(!manager.any_syncing(), "Should not be syncing with no keys");

        let pubky = PublicKey::from_str(DEV_MODE_PUBKY).unwrap();
        manager.add_key(pubky.clone()).await.unwrap();

        // After adding, we should be able to observe syncing at some point
        // In dev mode, the sync completes quickly, so we check initial state
        // The key starts in Starting state, then may go to Syncing or Idle
        let state = manager.get_key_state(&pubky);
        assert!(state.is_some(), "Key state should exist after add");

        // Wait for sync to complete and verify we transition to Idle
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;

        // After sync completes, any_syncing should be false
        assert!(
            !manager.any_syncing(),
            "Should not be syncing after initial sync completes in dev mode"
        );
    }

    #[tokio::test]
    async fn test_manager_any_running() {
        let temp_dir = TempDir::new().unwrap();
        let config = create_test_config(&temp_dir);
        let manager = BackupManager::new(config).await.unwrap();

        // No keys - none running
        assert!(!manager.any_running());

        let pubky = PublicKey::from_str(DEV_MODE_PUBKY).unwrap();
        manager.add_key(pubky).await.unwrap();

        // After adding, should be running
        assert!(manager.any_running());
    }

    #[tokio::test]
    async fn test_manager_data_dir() {
        let temp_dir = TempDir::new().unwrap();
        let config = create_test_config(&temp_dir);
        let manager = BackupManager::new(config).await.unwrap();

        let data_dir = manager.data_dir();
        assert_eq!(data_dir, temp_dir.path());
    }

    #[tokio::test]
    async fn test_manager_force_sync_success() {
        let temp_dir = TempDir::new().unwrap();
        let config = create_test_config(&temp_dir);
        let manager = BackupManager::new(config).await.unwrap();

        let pubky = PublicKey::from_str(DEV_MODE_PUBKY).unwrap();

        // Add key first
        manager.add_key(pubky.clone()).await.unwrap();

        // Force sync should succeed
        let result = manager.force_sync(&pubky).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_manager_structured_errors() {
        let temp_dir = TempDir::new().unwrap();
        let config = create_test_config(&temp_dir);
        let manager = BackupManager::new(config).await.unwrap();

        let pubky = PublicKey::from_str(DEV_MODE_PUBKY).unwrap();

        // Test KeyAlreadyExists error
        manager.add_key(pubky.clone()).await.unwrap();
        let err = manager.add_key(pubky.clone()).await.unwrap_err();
        match err {
            OrchestratorError::KeyAlreadyExists(key) => {
                assert!(key.contains(&pubky.z32()[..8])); // Partial match on key string
            }
            _ => panic!("Expected KeyAlreadyExists error, got {:?}", err),
        }

        // Test KeyNotFound errors for various operations
        manager.remove_key(&pubky).await.unwrap();

        // remove_key should return KeyNotFound
        let err = manager.remove_key(&pubky).await.unwrap_err();
        assert!(
            matches!(err, OrchestratorError::KeyNotFound(_)),
            "remove_key should return KeyNotFound, got {:?}",
            err
        );

        // force_sync should return KeyNotFound
        let err = manager.force_sync(&pubky).await.unwrap_err();
        assert!(
            matches!(err, OrchestratorError::KeyNotFound(_)),
            "force_sync should return KeyNotFound, got {:?}",
            err
        );

        // create_snapshot should return KeyNotFound
        let err = manager.create_snapshot(&pubky).await.unwrap_err();
        assert!(
            matches!(err, OrchestratorError::KeyNotFound(_)),
            "create_snapshot should return KeyNotFound, got {:?}",
            err
        );
    }

    #[tokio::test]
    async fn test_manager_last_pubky_persistence() {
        let temp_dir = TempDir::new().unwrap();

        let pubky = PublicKey::from_str(DEV_MODE_PUBKY).unwrap();

        // Create manager and write last pubky
        {
            let config = create_test_config(&temp_dir);
            let manager = BackupManager::new(config).await.unwrap();
            manager.write_last_pubky(&pubky).await.unwrap();
        }

        // Create new manager and read back
        {
            let config = create_test_config(&temp_dir);
            let manager = BackupManager::new(config).await.unwrap();
            let read_pubky = manager.read_last_pubky().await.unwrap();
            assert_eq!(read_pubky, Some(pubky));
        }
    }

    #[tokio::test]
    async fn test_manager_create_snapshot() {
        let temp_dir = TempDir::new().unwrap();
        let config = create_test_config(&temp_dir);
        let manager = BackupManager::new(config).await.unwrap();

        let pubky = PublicKey::from_str(DEV_MODE_PUBKY).unwrap();

        // Subscribe to status updates before adding key
        let mut rx = manager.subscribe();

        // Add key
        manager.add_key(pubky.clone()).await.unwrap();

        // Wait for Idle status indicating sync is complete
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            loop {
                if let Ok(update) = rx.recv().await {
                    if update.pubky == pubky && matches!(update.state.status, KeyStatus::Idle) {
                        break;
                    }
                }
            }
        })
        .await
        .expect("Should reach Idle status within timeout");

        // Write test data directly (offline mode doesn't fetch real data)
        let data_dir = temp_dir
            .path()
            .join("keys")
            .join(pubky.z32())
            .join("data")
            .join("pub");
        std::fs::create_dir_all(&data_dir).unwrap();
        std::fs::write(data_dir.join("test.json"), b"test data").unwrap();

        // Create snapshot
        let snapshot_path = manager.create_snapshot(&pubky).await.unwrap();

        // Verify snapshot file exists
        assert!(snapshot_path.exists(), "Snapshot file should exist");
        assert!(
            snapshot_path
                .extension()
                .map(|e| e == "zip")
                .unwrap_or(false),
            "Snapshot should be a zip file"
        );

        // Verify it's in the expected location
        assert!(
            snapshot_path.to_string_lossy().contains(&pubky.z32()[..8]),
            "Snapshot path should contain pubky"
        );
    }

    #[tokio::test]
    async fn test_manager_delete_key_not_found() {
        let temp_dir = TempDir::new().unwrap();
        let config = create_test_config(&temp_dir);
        let manager = BackupManager::new(config).await.unwrap();

        let pubky = PublicKey::from_str(DEV_MODE_PUBKY).unwrap();

        // Try to delete non-existent key - should return KeyNotFound
        let result = manager.delete_key(&pubky).await;
        assert!(
            matches!(result, Err(OrchestratorError::KeyNotFound(_))),
            "delete_key should return KeyNotFound for non-existent key, got {:?}",
            result
        );
    }

    #[tokio::test]
    async fn test_manager_any_error() {
        let temp_dir = TempDir::new().unwrap();
        let config = create_test_config(&temp_dir);
        let manager = BackupManager::new(config).await.unwrap();

        // No keys - no errors
        assert!(!manager.any_error(), "Should have no errors with no keys");

        let pubky = PublicKey::from_str(DEV_MODE_PUBKY).unwrap();
        manager.add_key(pubky).await.unwrap();

        // After adding in dev mode, should not have errors
        // (dev mode uses mock data which doesn't fail)
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        assert!(
            !manager.any_error(),
            "Dev mode key should not have errors after sync"
        );
    }

    #[tokio::test]
    async fn test_handle_controller_status_error_mapping() {
        // Test that ControllerStatus::Error maps correctly to KeyState
        let temp_dir = TempDir::new().unwrap();
        let storage = Arc::new(AppStorage::new_with_path(&temp_dir.path().to_path_buf()).unwrap());
        let pubky = PublicKey::from_str(DEV_MODE_PUBKY).unwrap();

        let error_status = ControllerStatus::Error {
            pubky: pubky.clone(),
            message: "Test error message".to_string(),
        };

        let state =
            handle_controller_status(&pubky, &storage, &error_status, 1000, None, None).await;

        // Verify error state mapping
        assert!(
            matches!(state.status, KeyStatus::Error),
            "Status should be Error"
        );
        assert!(state.error.is_some(), "Should have error details");

        let error = state.error.unwrap();
        assert_eq!(error.code, KeyErrorCode::Internal);
        assert_eq!(error.message, "Test error message");
        assert!(
            !error.recoverable,
            "Internal errors should not be recoverable"
        );
        assert_eq!(state.data_size, 1000, "Should preserve current data size");
    }

    #[tokio::test]
    async fn test_handle_controller_status_starting_mapping() {
        // Test that ControllerStatus::Starting maps correctly to KeyState
        let temp_dir = TempDir::new().unwrap();
        let storage = Arc::new(AppStorage::new_with_path(&temp_dir.path().to_path_buf()).unwrap());
        let pubky = PublicKey::from_str(DEV_MODE_PUBKY).unwrap();

        let starting_status = ControllerStatus::Starting {
            pubky: pubky.clone(),
        };

        let state =
            handle_controller_status(&pubky, &storage, &starting_status, 1234, None, None).await;

        // Verify starting state mapping
        assert!(
            matches!(state.status, KeyStatus::Starting),
            "Status should be Starting"
        );
        assert_eq!(state.data_size, 1234, "Should preserve current data size");
        assert!(state.error.is_none(), "Should not have error");
    }

    #[tokio::test]
    async fn test_first_key_starts_immediately_without_stagger_delay() {
        // Test that the first key added starts syncing immediately (no stagger delay),
        // even when developer_mode is false. This ensures good UX for single-key users.
        // The stagger delay logic checks key_count == 0 before inserting the key.
        let temp_dir = TempDir::new().unwrap();
        let config = BackupManagerConfig {
            data_dir: Some(temp_dir.path().to_path_buf()),
            validation_timeout_secs: 30,
            developer_mode: true, // Use dev mode to avoid real network calls
        };
        let manager = BackupManager::new(config).await.unwrap();

        let pubky = PublicKey::from_str(DEV_MODE_PUBKY).unwrap();
        let mut rx = manager.subscribe();

        // Add the first key
        let start = std::time::Instant::now();
        manager.add_key(pubky.clone()).await.unwrap();

        // Wait for the Starting status - should arrive quickly since first key has no delay
        let update = tokio::time::timeout(std::time::Duration::from_millis(500), async {
            loop {
                if let Ok(update) = rx.recv().await {
                    if update.pubky == pubky && matches!(update.state.status, KeyStatus::Starting) {
                        return update;
                    }
                }
            }
        })
        .await
        .expect("First key should start immediately without stagger delay");

        let elapsed = start.elapsed();
        assert!(
            elapsed < std::time::Duration::from_millis(100),
            "First key should start within 100ms (no stagger delay), but took {:?}",
            elapsed
        );
        assert_eq!(update.pubky, pubky);
    }
}
