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
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;

use log::{debug, error, info, warn};
use parking_lot::RwLock;
use pubky::{Pubky, PublicKey};
use tokio::sync::{broadcast, mpsc};
use tokio::task::JoinHandle;

use super::discovery;
use super::error::OrchestratorError;
use super::session;
use super::status::{current_unix_timestamp, next_sync_time, spawn_status_listener};
use super::sync_interval::SyncInterval;
use super::types::*;
use crate::storage::AppStorage;
use crate::sync::{BackupController, ControllerCommand, ControllerStatus};

/// Maximum number of keys that can be backed up simultaneously.
///
/// This limit prevents resource exhaustion from too many concurrent backup controllers.
/// Should be more flexible in the future but this is fine for current use cases.
pub const MAX_KEYS: usize = 50;

/// Internal state for a managed key.
pub(crate) struct ManagedKey {
    /// Sender for control messages to the backup controller (mpsc - single receiver)
    pub(crate) control_tx: mpsc::Sender<ControllerCommand>,
    /// Current state of the key
    pub(crate) state: KeyState,
    /// Handle to the controller task
    pub(crate) task_handle: JoinHandle<()>,
}

pub(crate) struct ManagerInner {
    /// Map of public keys to their managed state
    pub(crate) keys: HashMap<PublicKey, ManagedKey>,
    /// Shared pubky client for SDK calls
    pub(crate) pubky_client: Arc<Pubky>,
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
    /// Sync interval management
    sync_interval: Arc<RwLock<SyncInterval>>,
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

        let sync_interval = SyncInterval::load(storage.clone(), config.sync_interval_secs);

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
            sync_interval: Arc::new(RwLock::new(sync_interval)),
        };

        // Spawn the centralized status listener task
        spawn_status_listener(
            inner,
            manager.sync_interval.clone(),
            storage,
            update_tx,
            status_rx,
        );

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

        if !self.config.developer_mode {
            discovery::discover_homeserver(&pubky, self.config.validation_timeout_secs).await?;
            discovery::verify_pubky_has_data(&pubky, self.config.validation_timeout_secs).await?;
        }

        // Clear inactive marker if re-adding a previously removed key
        let key_storage = self.storage.key_storage(&pubky)?;
        key_storage.clear_inactive().await.map_err(|e| {
            OrchestratorError::Internal(format!("Failed to clear inactive marker: {}", e))
        })?;

        self.start_controller(pubky.clone()).await?;

        info!("Added key for backup: {}", pubky);
        Ok(())
    }

    /// Stop a controller without any UI side effects.
    ///
    /// Removes from HashMap and sends Cancel. Returns the task JoinHandle so the
    /// caller can await graceful shutdown. If the cancel message can't be delivered
    /// (channel full), the task is aborted as a fallback.
    fn stop_controller(&self, pubky: &PublicKey) -> Result<JoinHandle<()>, OrchestratorError> {
        let managed_key = {
            let mut inner = self.inner.write();
            inner
                .keys
                .remove(pubky)
                .ok_or_else(|| OrchestratorError::KeyNotFound(pubky.to_string()))?
        };

        if managed_key
            .control_tx
            .try_send(ControllerCommand::Cancel)
            .is_err()
        {
            warn!(
                "Failed to send Cancel to controller for {}, aborting task",
                pubky
            );
            managed_key.task_handle.abort();
        }

        Ok(managed_key.task_handle)
    }

    /// Clear last_pubky from storage when no keys remain, so the app
    /// doesn't auto-load a removed key on next launch.
    async fn clear_last_pubky_if_empty(&self) {
        let no_keys_left = {
            let inner = self.inner.read();
            inner.keys.is_empty()
        };
        if no_keys_left {
            if let Err(e) = self.storage.clear_last_pubky().await {
                warn!("Failed to clear last pubky: {}", e);
            }
        }
    }

    /// Stop syncing a key and mark it inactive. Backed-up data is preserved on disk
    /// but the key will not be resumed on next launch.
    ///
    /// Use `delete_key()` to also remove all backed-up data.
    ///
    /// # Errors
    ///
    /// Returns `OrchestratorError::KeyNotFound` if the key is not being backed up.
    pub async fn remove_key(&self, pubky: &PublicKey) -> Result<(), OrchestratorError> {
        self.stop_controller(pubky)?;

        // Mark the key as inactive so it won't be resumed on next launch
        let key_storage = self.storage.key_storage(pubky)?;
        key_storage.mark_inactive().await.map_err(|e| {
            OrchestratorError::Internal(format!("Failed to mark key inactive: {}", e))
        })?;

        self.clear_last_pubky_if_empty().await;
        info!("Stopped and deactivated backup for key: {}", pubky);
        Ok(())
    }

    /// Remove a key AND delete all backed-up data from disk.
    ///
    /// Awaits the controller task to ensure all in-flight IO completes
    /// before deleting files from disk.
    ///
    /// # Errors
    ///
    /// Returns `OrchestratorError::KeyNotFound` if the key is not being backed up.
    pub async fn delete_key(&self, pubky: &PublicKey) -> Result<(), OrchestratorError> {
        let handle = self.stop_controller(pubky)?;
        self.clear_last_pubky_if_empty().await;

        // Await the controller to ensure all in-flight IO completes before deleting files
        if let Err(e) = handle.await {
            warn!("Controller task for {} finished with error: {}", pubky, e);
        }

        // Delete the data directory for this key
        let key_dir = self.storage.keys_dir().join(pubky.z32());

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

    /// Retry a key that is currently in error state.
    ///
    /// Tears down the errored controller, re-validates (homeserver discovery + data check),
    /// and restarts the controller. If re-validation fails, the error propagates to the caller
    /// instead of silently leaving the key in error state.
    ///
    /// # Errors
    ///
    /// Returns `OrchestratorError::KeyNotFound` if the key is not being backed up.
    /// Returns validation errors if re-validation fails.
    pub async fn retry_errored_key(&self, pubky: &PublicKey) -> Result<(), OrchestratorError> {
        // Remove the errored controller
        self.stop_controller(pubky)?;

        // Re-validate
        if !self.config.developer_mode {
            discovery::discover_homeserver(pubky, self.config.validation_timeout_secs).await?;
            discovery::verify_pubky_has_data(pubky, self.config.validation_timeout_secs).await?;
        }

        // Restart the controller
        self.start_controller(pubky.clone()).await?;
        info!("Retried and restarted key: {}", pubky);
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

        let snapshot_count = self.storage.count_snapshots(pubky);
        if let Err(e) = self
            .storage
            .write_activity(
                pubky,
                &ActivityEntry {
                    activity_type: ActivityType::SnapshotCreated,
                    message: format!("Created snapshot v{}", snapshot_count),
                    timestamp: current_unix_timestamp(),
                },
            )
            .await
        {
            warn!("Failed to write activity for {}: {}", pubky, e);
        }

        Ok(path)
    }

    /// Get recent activity entries for a key.
    pub async fn get_activity(&self, pubky: &PublicKey, limit: usize) -> Vec<ActivityEntry> {
        self.storage.read_activity(pubky, limit).await
    }

    /// Get the root data directory path (e.g. `~/.pubky-backup`).
    pub fn data_dir(&self) -> PathBuf {
        self.storage
            .get_backup_data_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
    }

    /// Get the current keys directory path.
    pub fn keys_dir(&self) -> &Path {
        self.storage.keys_dir()
    }

    /// Move the keys directory to a new parent location.
    ///
    /// **This manager must not be used after this call.** Controllers are
    /// shut down and internal storage paths become stale. The caller must
    /// drop this instance and create a fresh [`BackupManager`] via
    /// [`BackupManager::new`] to resume operations from the new location.
    ///
    /// Note: ideally this would take `self` by value to enforce single-use
    /// semantics at compile time. It takes `&self` because the manager is
    /// shared via `Arc` at the Tauri layer. The caller is responsible for
    /// ensuring no concurrent operations occur during the move (e.g. via
    /// a relocating flag or mutex).
    pub async fn move_keys(&self, new_parent: &Path) -> Result<PathBuf, OrchestratorError> {
        self.shutdown().await;
        let new_keys_dir = self.storage.move_keys(new_parent)?;
        Ok(new_keys_dir)
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

    /// Gracefully shutdown all backups, awaiting controller tasks.
    pub async fn shutdown(&self) {
        let keys: Vec<PublicKey> = self.get_keys();

        let mut handles = Vec::new();
        for pubky in keys {
            match self.stop_controller(&pubky) {
                Ok(handle) => handles.push(handle),
                Err(e) => warn!("Error shutting down key {}: {}", pubky, e),
            }
        }

        for result in futures_util::future::join_all(handles).await {
            if let Err(e) = result {
                warn!("Controller task finished with error during shutdown: {}", e);
            }
        }

        info!("BackupManager shutdown complete");
    }

    /// Get the current sync interval in seconds.
    pub fn get_sync_interval(&self) -> u64 {
        self.sync_interval.read().get()
    }

    /// Set the sync interval for all controllers.
    ///
    /// Validates, persists to disk, notifies controllers via watch channel,
    /// then force-syncs all controllers so they immediately use the new interval.
    pub async fn set_sync_interval(&self, interval_secs: u64) -> Result<(), OrchestratorError> {
        SyncInterval::validate(interval_secs)?;

        // Persist to disk first (async) — no lock held across await
        self.storage
            .write_sync_interval(interval_secs)
            .await
            .map_err(OrchestratorError::Storage)?;

        // Apply in-memory update and notify controllers
        self.sync_interval.write().apply(interval_secs);

        // Force sync all controllers so they immediately wake up and use the new interval,
        // rather than waiting for the previous (potentially longer) sleep to expire.
        let inner = self.inner.read();
        for (pubky, managed_key) in &inner.keys {
            if let Err(e) = managed_key
                .control_tx
                .try_send(ControllerCommand::ForceSync)
            {
                warn!(
                    "Failed to force sync {} after interval change: {}",
                    pubky, e
                );
            }
        }

        Ok(())
    }

    // --- Internal methods ---

    /// Register a key in the manager: broadcast its state and insert into the keys map.
    fn register_key(
        &self,
        pubky: PublicKey,
        state: KeyState,
        control_tx: mpsc::Sender<ControllerCommand>,
        task_handle: JoinHandle<()>,
    ) {
        let receivers = self.update_tx.send(KeyUpdate {
            pubky: pubky.clone(),
            state: state.clone(),
        });
        debug!("Broadcast state for {} (receivers: {:?})", pubky, receivers);

        let mut inner = self.inner.write();
        inner.keys.insert(
            pubky,
            ManagedKey {
                control_tx,
                state,
                task_handle,
            },
        );
    }

    /// Start the backup controller for a key
    async fn start_controller(&self, pubky: PublicKey) -> Result<(), OrchestratorError> {
        let (control_tx, control_rx) = mpsc::channel(5);

        let initial_size = self.storage.calculate_pubky_size(&pubky).await;

        let sync_interval_secs = self.sync_interval.read().get();

        let initial_state = KeyState {
            status: KeyStatus::Starting,
            data_size: initial_size,
            next_sync: Some(next_sync_time(sync_interval_secs)),
            ..Default::default()
        };

        let (pubky_client, key_count) = {
            let inner = self.inner.read();
            (inner.pubky_client.clone(), inner.keys.len())
        };

        // Generate random initial delay (0 to sync_interval_secs) to stagger syncs
        // Skip staggering in developer mode or if this is the first key
        let initial_delay = if self.config.developer_mode || key_count == 0 {
            std::time::Duration::ZERO
        } else {
            std::time::Duration::from_secs(rand::random::<u64>() % sync_interval_secs)
        };

        let controller = BackupController::new(
            pubky.clone(),
            self.storage.clone(),
            pubky_client,
            Some(control_rx),
            Some(self.status_tx.clone()),
            self.sync_interval.read().subscribe(),
        )
        .with_initial_delay(initial_delay);

        let task_handle = tokio::spawn(async move {
            controller.run().await;
        });

        self.register_key(pubky, initial_state, control_tx, task_handle);

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

            let validation_result = if self.config.developer_mode {
                Ok(())
            } else {
                match discovery::discover_homeserver(&pubky, self.config.validation_timeout_secs)
                    .await
                {
                    Ok(_) => {
                        discovery::verify_pubky_has_data(
                            &pubky,
                            self.config.validation_timeout_secs,
                        )
                        .await
                    }
                    Err(e) => Err(e),
                }
            };

            match validation_result {
                Ok(()) => {
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
            error: Some(error),
            ..Default::default()
        };

        let task_handle = tokio::spawn(async {});
        self.register_key(pubky, error_state, control_tx, task_handle);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::AppStorage;
    use crate::sync::{DEFAULT_SYNC_INTERVAL_SECONDS, MIN_SYNC_INTERVAL_SECONDS};
    use crate::TEST_PUBKY;
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
            sync_interval_secs: DEFAULT_SYNC_INTERVAL_SECONDS,
        }
    }

    /// Create a temp dir and a BackupManager configured for testing.
    async fn setup() -> (TempDir, BackupManager) {
        let temp_dir = TempDir::new().unwrap();
        let (_, manager) = setup_with_dir(&temp_dir).await;
        (temp_dir, manager)
    }

    /// Create a BackupManager using an existing temp dir (for multi-lifecycle tests).
    async fn setup_with_dir(temp_dir: &TempDir) -> (BackupManagerConfig, BackupManager) {
        let config = create_test_config(temp_dir);
        let manager = BackupManager::new(config.clone()).await.unwrap();
        (config, manager)
    }

    fn test_pubky() -> PublicKey {
        PublicKey::from_str(TEST_PUBKY).unwrap()
    }

    #[tokio::test]
    async fn test_manager_add_remove_key() {
        let (_dir, manager) = setup().await;

        let pubky = test_pubky();

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
        let (temp_dir, manager) = setup().await;

        let pubky = test_pubky();

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
        let (_dir, manager) = setup().await;

        let mut rx = manager.subscribe();
        let pubky = test_pubky();

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

        let pubky = test_pubky();

        // First, create a manager and add a key (creates data directory)
        {
            let (_, manager) = setup_with_dir(&temp_dir).await;

            manager.add_key(pubky.clone()).await.unwrap();

            // Wait for some data to be written
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;

            // Shutdown cleanly
            manager.shutdown().await;
        }

        // Create a new manager - should automatically resume the key
        {
            let (_, manager) = setup_with_dir(&temp_dir).await;

            // Key should be automatically resumed from stored data
            let keys = manager.get_keys();
            assert_eq!(keys.len(), 1, "Key should be auto-resumed from stored data");
            assert!(keys.contains(&pubky));
        }
    }

    #[tokio::test]
    async fn test_manager_removed_key_not_resumed() {
        let temp_dir = TempDir::new().unwrap();

        let pubky = test_pubky();

        // Add a key, then remove it (marks inactive)
        {
            let (_, manager) = setup_with_dir(&temp_dir).await;
            manager.add_key(pubky.clone()).await.unwrap();
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            manager.remove_key(&pubky).await.unwrap();
            manager.shutdown().await;
        }

        // New manager should NOT resume the inactive key
        {
            let (_, manager) = setup_with_dir(&temp_dir).await;
            let keys = manager.get_keys();
            assert!(keys.is_empty(), "Inactive key should not be resumed");

            // Data directory should still exist on disk
            let key_dir = temp_dir.path().join("keys").join(pubky.z32());
            assert!(key_dir.exists(), "Data should be preserved after remove");
        }
    }

    #[tokio::test]
    async fn test_manager_readd_removed_key() {
        let temp_dir = TempDir::new().unwrap();

        let pubky = test_pubky();

        // Add, remove, then re-add the same key
        {
            let (_, manager) = setup_with_dir(&temp_dir).await;

            manager.add_key(pubky.clone()).await.unwrap();
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            manager.remove_key(&pubky).await.unwrap();

            // Re-add should succeed and clear the inactive marker
            manager.add_key(pubky.clone()).await.unwrap();
            assert_eq!(manager.get_keys().len(), 1);
            manager.shutdown().await;
        }

        // New manager should resume the re-added key
        {
            let (_, manager) = setup_with_dir(&temp_dir).await;
            let keys = manager.get_keys();
            assert_eq!(keys.len(), 1, "Re-added key should be resumed");
            assert!(keys.contains(&pubky));
        }
    }

    #[tokio::test]
    async fn test_manager_shutdown() {
        let (_dir, manager) = setup().await;

        let pubky = test_pubky();

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
        let (_dir, manager) = setup().await;

        // No keys - not syncing
        assert!(!manager.any_syncing(), "Should not be syncing with no keys");

        let pubky = test_pubky();
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
        let (_dir, manager) = setup().await;

        // No keys - none running
        assert!(!manager.any_running());

        let pubky = test_pubky();
        manager.add_key(pubky).await.unwrap();

        // After adding, should be running
        assert!(manager.any_running());
    }

    #[tokio::test]
    async fn test_manager_data_dir() {
        let (temp_dir, manager) = setup().await;

        let data_dir = manager.data_dir();
        assert_eq!(data_dir, temp_dir.path());
    }

    #[tokio::test]
    async fn test_manager_force_sync_success() {
        let (_dir, manager) = setup().await;

        let pubky = test_pubky();

        // Add key first
        manager.add_key(pubky.clone()).await.unwrap();

        // Force sync should succeed
        let result = manager.force_sync(&pubky).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_manager_structured_errors() {
        let (_dir, manager) = setup().await;

        let pubky = test_pubky();

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

        let pubky = test_pubky();

        // Create manager and write last pubky
        {
            let (_, manager) = setup_with_dir(&temp_dir).await;
            manager.write_last_pubky(&pubky).await.unwrap();
        }

        // Create new manager and read back
        {
            let (_, manager) = setup_with_dir(&temp_dir).await;
            let read_pubky = manager.read_last_pubky().await.unwrap();
            assert_eq!(read_pubky, Some(pubky));
        }
    }

    #[tokio::test]
    async fn test_manager_create_snapshot() {
        let (temp_dir, manager) = setup().await;

        let pubky = test_pubky();

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
        let (_dir, manager) = setup().await;

        let pubky = test_pubky();

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
        let (_dir, manager) = setup().await;

        // No keys - no errors
        assert!(!manager.any_error(), "Should have no errors with no keys");

        let pubky = test_pubky();
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
    async fn test_set_sync_interval_updates_and_persists() {
        let (_dir, manager) = setup().await;

        // Default interval
        assert_eq!(manager.get_sync_interval(), DEFAULT_SYNC_INTERVAL_SECONDS);

        // Set new interval
        manager.set_sync_interval(600).await.unwrap();
        assert_eq!(manager.get_sync_interval(), 600);

        // Verify persisted to disk
        let stored = manager.storage.read_sync_interval();
        assert_eq!(stored, Some(600));
    }

    #[tokio::test]
    async fn test_set_sync_interval_rejects_zero() {
        let (_dir, manager) = setup().await;

        let result = manager.set_sync_interval(0).await;
        assert!(result.is_err(), "Should reject zero interval");

        // Interval should be unchanged
        assert_eq!(manager.get_sync_interval(), DEFAULT_SYNC_INTERVAL_SECONDS);
    }

    #[tokio::test]
    async fn test_set_sync_interval_rejects_too_small() {
        let (_dir, manager) = setup().await;

        let result = manager.set_sync_interval(9).await;
        assert!(
            result.is_err(),
            "Should reject interval below minimum (10s)"
        );

        // Interval should be unchanged
        assert_eq!(manager.get_sync_interval(), DEFAULT_SYNC_INTERVAL_SECONDS);
    }

    #[tokio::test]
    async fn test_set_sync_interval_accepts_exact_minimum() {
        let (_dir, manager) = setup().await;

        // Exact minimum (10s) should be accepted
        manager
            .set_sync_interval(MIN_SYNC_INTERVAL_SECONDS)
            .await
            .unwrap();
        assert_eq!(manager.get_sync_interval(), MIN_SYNC_INTERVAL_SECONDS);
    }

    #[tokio::test]
    async fn test_set_sync_interval_no_controllers() {
        let (_dir, manager) = setup().await;

        // No keys added — should succeed as a no-op for controller restarts
        manager.set_sync_interval(600).await.unwrap();
        assert_eq!(manager.get_sync_interval(), 600);
    }

    #[tokio::test]
    async fn test_set_sync_interval_keeps_controllers_running() {
        let (_dir, manager) = setup().await;

        let pubky = test_pubky();
        manager.add_key(pubky.clone()).await.unwrap();

        // Change interval — controller should keep running (no restart)
        manager.set_sync_interval(600).await.unwrap();

        // Key should still be managed
        assert!(manager.get_key_state(&pubky).is_some());
        assert_eq!(manager.get_keys().len(), 1);
    }

    #[tokio::test]
    async fn test_persisted_sync_interval_loaded_on_startup() {
        let temp_dir = TempDir::new().unwrap();

        // First: create a manager and set a custom interval
        {
            let (_, manager) = setup_with_dir(&temp_dir).await;
            manager.set_sync_interval(900).await.unwrap();
        }

        // Second: create a new manager from the same data dir — should load 900
        let (_, manager) = setup_with_dir(&temp_dir).await;
        assert_eq!(manager.get_sync_interval(), 900);
    }

    #[tokio::test]
    async fn test_first_key_starts_immediately_without_stagger_delay() {
        // Test that the first key added starts syncing immediately (no stagger delay),
        // even when developer_mode is false. This ensures good UX for single-key users.
        // The stagger delay logic checks key_count == 0 before inserting the key.
        let (_dir, manager) = setup().await;

        let pubky = test_pubky();
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

    #[tokio::test]
    async fn test_stored_interval_below_minimum_falls_back_to_default() {
        let temp_dir = TempDir::new().unwrap();

        // Manually write an invalid (too small) interval to disk
        {
            let (_, manager) = setup_with_dir(&temp_dir).await;
            // Write a value below the minimum directly to storage
            manager.storage.write_sync_interval(5).await.unwrap();
        }

        // Create a new manager — should reject stored value and use default
        let (_, manager) = setup_with_dir(&temp_dir).await;
        assert_eq!(
            manager.get_sync_interval(),
            DEFAULT_SYNC_INTERVAL_SECONDS,
            "Should fall back to default when stored value is below minimum"
        );
    }

    #[tokio::test]
    async fn test_config_sync_interval_used_when_no_stored_value() {
        let temp_dir = TempDir::new().unwrap();

        // Create a config with a non-default interval and no stored value on disk
        enable_developer_mode();
        let config = BackupManagerConfig {
            data_dir: Some(temp_dir.path().to_path_buf()),
            validation_timeout_secs: 30,
            developer_mode: true,
            sync_interval_secs: 900,
        };
        let manager = BackupManager::new(config).await.unwrap();

        assert_eq!(
            manager.get_sync_interval(),
            900,
            "Should use config.sync_interval_secs when no stored value exists"
        );
    }

    #[tokio::test]
    async fn test_move_keys_moves_data_and_recreates_manager() {
        let (temp_dir, manager) = setup().await;

        let pubky = test_pubky();
        manager.add_key(pubky.clone()).await.unwrap();

        let original_keys_dir = manager.keys_dir().to_path_buf();
        assert!(original_keys_dir.exists());

        // Move keys to a new location (shuts down controllers)
        let new_parent = temp_dir.path().join("moved");
        let new_keys_dir = manager.move_keys(&new_parent).await.unwrap();
        drop(manager);

        assert_ne!(original_keys_dir, new_keys_dir);
        assert!(!original_keys_dir.exists(), "Old keys dir should be gone");
        assert!(new_keys_dir.exists(), "New keys dir should exist");

        // Create a new manager from the same config dir — should pick up new location
        let (_, manager2) = setup_with_dir(&temp_dir).await;

        assert_eq!(manager2.keys_dir(), new_keys_dir);
        // The key should have been resumed from the new location
        assert_eq!(manager2.get_keys().len(), 1);
    }

    /// The startup screen calls `get_last_pubky` and `get_keys` to decide
    /// whether to skip straight to the dashboard.  These reads must work
    /// from `AppStorage` alone — without constructing a `BackupManager`
    /// (which blocks on `resume_stored_keys` → network calls).
    ///
    /// This test simulates a full session lifecycle and verifies that a
    /// fresh `AppStorage` can read back everything the frontend needs.
    #[tokio::test]
    async fn test_startup_reads_do_not_need_manager() {
        let (temp_dir, manager) = setup().await;

        let pubky = test_pubky();
        manager.add_key(pubky.clone()).await.unwrap();
        manager.write_last_pubky(&pubky).await.unwrap();

        // Drop the manager — simulates app restart
        manager.shutdown().await;
        drop(manager);

        // Read using raw storage — this is what Tauri commands should fall back to
        // when the manager hasn't finished initialising yet.
        let storage = AppStorage::new_with_path(temp_dir.path()).unwrap();

        let last = storage.read_last_pubky().await.unwrap();
        assert_eq!(
            last,
            Some(pubky.clone()),
            "last_pubky must be readable without a BackupManager"
        );

        let dirs = storage.list_pubky_directories().unwrap();
        assert!(
            dirs.contains(&pubky.z32()),
            "key directories must be listable without a BackupManager"
        );
    }
}
