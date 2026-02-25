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
    /// The homeserver for this pubky (stored for potential future use)
    #[allow(dead_code)]
    homeserver: Option<PublicKey>,
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
        // Check if key already exists or limit reached
        {
            let inner = self.inner.read();
            if inner.keys.contains_key(&pubky) {
                return Err(OrchestratorError::KeyAlreadyExists(pubky.to_string()));
            }
            if inner.keys.len() >= MAX_KEYS {
                return Err(OrchestratorError::KeyLimitReached(MAX_KEYS));
            }
        }

        // Validate the key and discover homeserver
        let homeserver = discovery::validate_pubky(
            &pubky,
            self.config.validation_timeout_secs,
            self.config.developer_mode,
        )
        .await?;

        // Start the controller
        self.start_controller(pubky.clone(), Some(homeserver))
            .await?;

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

    /// Force immediate sync for all keys.
    pub async fn force_sync_all(&self) -> Result<(), OrchestratorError> {
        let keys: Vec<PublicKey> = {
            let inner = self.inner.read();
            inner.keys.keys().cloned().collect()
        };

        for pubky in keys {
            if let Err(e) = self.force_sync(&pubky).await {
                warn!("Failed to force sync for {}: {}", pubky, e);
            }
        }

        Ok(())
    }

    /// Get current state of a specific key.
    ///
    /// Returns `None` if the key is not being backed up.
    pub fn get_key_state(&self, pubky: &PublicKey) -> Option<KeyState> {
        let inner = self.inner.read();
        inner.keys.get(pubky).map(|k| k.state.clone())
    }

    /// Get states of all managed keys.
    pub fn get_all_key_states(&self) -> HashMap<PublicKey, KeyState> {
        let inner = self.inner.read();
        inner
            .keys
            .iter()
            .map(|(k, v)| (k.clone(), v.state.clone()))
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
    async fn start_controller(
        &self,
        pubky: PublicKey,
        homeserver: Option<PublicKey>,
    ) -> Result<(), OrchestratorError> {
        // Create mpsc channel for control commands (single receiver per controller)
        let (control_tx, control_rx) = mpsc::channel(5);

        // Get initial data size
        let initial_size = self.storage.calculate_pubky_size(&pubky).await;

        // Create initial state
        let initial_state = KeyState {
            status: KeyStatus::Starting,
            homeserver: homeserver.clone(),
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
        let initial_delay =
            std::time::Duration::from_secs(rand::random::<u64>() % SYNC_INTERVAL_SECONDS);

        // Create the controller with the shared status channel
        // All controllers send to the same status_tx, identified by pubky in each message
        let controller = BackupController::with_initial_delay(
            pubky.clone(),
            self.storage.clone(),
            pubky_client,
            Some(control_rx),
            Some(self.status_tx.clone()),
            initial_delay,
        );

        // Spawn the controller task
        let task_handle = tokio::spawn(async move {
            controller.run().await;
        });

        // Store the managed key
        {
            let mut inner = self.inner.write();
            inner.keys.insert(
                pubky.clone(),
                ManagedKey {
                    homeserver,
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
                warn!("Failed to list key directories: {}", e);
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
                    warn!("Invalid pubky directory name {}: {}", pubky_str, e);
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
                Ok(homeserver) => {
                    if let Err(e) = self.start_controller(pubky.clone(), Some(homeserver)).await {
                        error!("Failed to resume key {}: {}", pubky, e);
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
                    warn!("Failed to validate key {} during resume: {}", pubky, e);
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
            homeserver: None,
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
                    homeserver: None,
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
                ControllerStatus::Syncing { pubky, .. } => pubky.clone(),
                ControllerStatus::Idle { pubky } => pubky.clone(),
                ControllerStatus::Ended { pubky } => pubky.clone(),
                ControllerStatus::Error { pubky, .. } => pubky.clone(),
            };

            // Get current state to preserve data_size and homeserver
            let (current_data_size, homeserver) = {
                let inner_read = inner.read();
                inner_read
                    .keys
                    .get(&pubky)
                    .map(|k| (k.state.data_size, k.homeserver.clone()))
                    .unwrap_or((0, None))
            };

            let new_state =
                handle_controller_status(&pubky, &storage, &status, homeserver, current_data_size)
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
/// * `homeserver` - The homeserver to preserve in the state
/// * `current_data_size` - The current data size to use as fallback
async fn handle_controller_status(
    pubky: &PublicKey,
    storage: &Arc<AppStorage>,
    status: &ControllerStatus,
    homeserver: Option<PublicKey>,
    current_data_size: u64,
) -> KeyState {
    match status {
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
                homeserver,
                data_size,
                ..Default::default()
            }
        }
        ControllerStatus::Idle { .. } => {
            let data_size = storage.calculate_pubky_size(pubky).await;
            KeyState {
                status: KeyStatus::Idle,
                homeserver,
                data_size,
                last_sync: Some(current_unix_timestamp()),
                next_sync: Some(next_sync_time()),
                ..Default::default()
            }
        }
        ControllerStatus::Ended { .. } => KeyState {
            status: KeyStatus::Stopped,
            homeserver,
            data_size: current_data_size,
            ..Default::default()
        },
        ControllerStatus::Error { message, .. } => KeyState {
            status: KeyStatus::Error,
            homeserver,
            data_size: current_data_size,
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

    fn create_test_config(temp_dir: &TempDir) -> BackupManagerConfig {
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
    async fn test_manager_get_all_key_states() {
        let temp_dir = TempDir::new().unwrap();
        let config = create_test_config(&temp_dir);
        let manager = BackupManager::new(config).await.unwrap();

        let pubky = PublicKey::from_str(DEV_MODE_PUBKY).unwrap();

        // Add key
        manager.add_key(pubky.clone()).await.unwrap();

        // Get all states
        let states = manager.get_all_key_states();
        assert_eq!(states.len(), 1);
        assert!(states.contains_key(&pubky));
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
    async fn test_manager_homeserver_preserved_in_state() {
        let temp_dir = TempDir::new().unwrap();
        let config = create_test_config(&temp_dir);
        let manager = BackupManager::new(config).await.unwrap();

        let pubky = PublicKey::from_str(DEV_MODE_PUBKY).unwrap();

        // Add key
        manager.add_key(pubky.clone()).await.unwrap();

        // Wait a bit for status updates to propagate
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        // Get state and verify homeserver is set (in dev mode, homeserver = pubky)
        let state = manager.get_key_state(&pubky).unwrap();
        assert!(
            state.homeserver.is_some(),
            "Homeserver should be preserved in state"
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

        // Force sync immediately (bypasses random initial delay)
        manager.force_sync(&pubky).await.unwrap();

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
    async fn test_manager_force_sync_all() {
        let temp_dir = TempDir::new().unwrap();
        let config = create_test_config(&temp_dir);
        let manager = BackupManager::new(config).await.unwrap();

        let pubky1 = PublicKey::from_str(DEV_MODE_PUBKY).unwrap();
        let pubky2 =
            PublicKey::from_str("o4dksfbqk85ogzdb5osziw6befigbuxmuxkuxq8434q89uj56uxo").unwrap();

        // Add two keys
        manager.add_key(pubky1.clone()).await.unwrap();
        manager.add_key(pubky2.clone()).await.unwrap();

        // Wait for initial sync to complete
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        // Force sync all should succeed without errors
        let result = manager.force_sync_all().await;
        assert!(result.is_ok(), "force_sync_all should succeed");

        // Both keys should still be running
        assert_eq!(manager.get_keys().len(), 2);
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

        let state = handle_controller_status(&pubky, &storage, &error_status, None, 1000).await;

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
    async fn test_handle_controller_status_preserves_homeserver() {
        // Test that homeserver is preserved across all status types
        let temp_dir = TempDir::new().unwrap();
        let storage = Arc::new(AppStorage::new_with_path(&temp_dir.path().to_path_buf()).unwrap());
        let pubky = PublicKey::from_str(DEV_MODE_PUBKY).unwrap();
        let homeserver = Some(
            PublicKey::from_str("o4dksfbqk85ogzdb5osziw6befigbuxmuxkuxq8434q89uj56uxo").unwrap(),
        );

        // Test Syncing status
        let state = handle_controller_status(
            &pubky,
            &storage,
            &ControllerStatus::Syncing {
                pubky: pubky.clone(),
                events_processed: 5,
            },
            homeserver.clone(),
            0,
        )
        .await;
        assert_eq!(
            state.homeserver, homeserver,
            "Syncing should preserve homeserver"
        );

        // Test Idle status
        let state = handle_controller_status(
            &pubky,
            &storage,
            &ControllerStatus::Idle {
                pubky: pubky.clone(),
            },
            homeserver.clone(),
            0,
        )
        .await;
        assert_eq!(
            state.homeserver, homeserver,
            "Idle should preserve homeserver"
        );

        // Test Ended status
        let state = handle_controller_status(
            &pubky,
            &storage,
            &ControllerStatus::Ended {
                pubky: pubky.clone(),
            },
            homeserver.clone(),
            0,
        )
        .await;
        assert_eq!(
            state.homeserver, homeserver,
            "Ended should preserve homeserver"
        );

        // Test Error status
        let state = handle_controller_status(
            &pubky,
            &storage,
            &ControllerStatus::Error {
                pubky: pubky.clone(),
                message: "test".to_string(),
            },
            homeserver.clone(),
            0,
        )
        .await;
        assert_eq!(
            state.homeserver, homeserver,
            "Error should preserve homeserver"
        );
    }
}
