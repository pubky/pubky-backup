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

use log::{debug, info, warn};
use parking_lot::RwLock;
use pubky::{Pubky, PublicKey};
use tokio::sync::{broadcast, mpsc};
use tokio::task::JoinHandle;

use super::auth;
use super::discovery;
use super::error::OrchestratorError;
use super::sync_interval::SyncInterval;
use super::types::{
    ActivityEntry, ActivityType, AuthStatus, BackupManagerConfig, KeyState, KeyStatus, KeyUpdate,
    MirrorStatus,
};
use crate::storage::AppStorage;
use crate::sync::{BackupController, ControllerCommand, ControllerUpdate};

mod approval;
mod mirror;
mod sign_in;
mod status;

use approval::PendingApproval;
use status::{log_activity, next_sync_time, SyncState};

/// Maximum number of keys that can be backed up simultaneously.
///
/// This limit prevents resource exhaustion from too many concurrent backup controllers.
/// Should be more flexible in the future but this is fine for current use cases.
pub const MAX_KEYS: usize = 50;

/// Per-key multiplier (in seconds) for the startup stagger window.
///
/// When resuming keys on startup, each controller is given a random initial
/// delay in `0..(key_count * STAGGER_MULTIPLE)` seconds to avoid overwhelming
/// the network with simultaneous requests. For example, with 10 keys this
/// spreads first syncs over ~50 seconds.
pub const STAGGER_MULTIPLE: usize = 5;

/// HTTP relay of a local Pubky testnet, which listens on a fixed port.
const TESTNET_HTTP_RELAY: &str = "http://localhost:15412/link/";

/// Internal state for a managed key.
struct ManagedKey {
    /// Sender for control messages to the backup controller (mpsc - single receiver)
    control_tx: mpsc::Sender<ControllerCommand>,
    /// State as the key's controller last reported it
    sync: SyncState,
    /// Whether the key is signed in
    auth: AuthStatus,
    /// Whether the key's data is also copied to a second homeserver
    mirror: MirrorStatus,
    /// Handle to the controller task
    task_handle: JoinHandle<()>,
    /// A sign-in of this key that awaits approval
    pending_sign_in: Option<PendingApproval>,
    /// A mirror request of this key that awaits approval
    pending_mirror: Option<PendingApproval>,
}

impl ManagedKey {
    /// The key's state, as reported to subscribers.
    fn state(&self) -> KeyState {
        self.sync
            .clone()
            .into_key_state(self.auth.clone(), self.mirror.clone())
    }
}

/// The manager's mutable state, behind its lock.
struct ManagerInner {
    /// Map of public keys to their managed state
    keys: HashMap<PublicKey, ManagedKey>,
    /// A sign-in that adds whichever key approves it, while it awaits approval
    new_key_sign_in: Option<PendingApproval>,
    /// Set by [`BackupManager::shutdown`], after which no key is added
    is_shut_down: bool,
}

impl ManagerInner {
    /// A key's state as its controller last reported it, or `None` if the key
    /// is not tracked.
    fn sync_state(&self, pubky: &PublicKey) -> Option<SyncState> {
        self.keys.get(pubky).map(|managed| managed.sync.clone())
    }

    /// Apply a state derived from a controller status update and return the
    /// state the key now has, or `None` if the key is not tracked.
    fn apply_sync_state(&mut self, pubky: &PublicKey, sync: SyncState) -> Option<KeyState> {
        let managed = self.keys.get_mut(pubky)?;
        managed.sync = sync;
        Some(managed.state())
    }
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
    /// Shared pubky client for SDK calls
    pubky_client: Arc<Pubky>,
    /// Channel for external subscribers to receive key updates
    update_tx: broadcast::Sender<KeyUpdate>,
    /// Shared status channel sender - all controllers send to this channel
    status_tx: broadcast::Sender<ControllerUpdate>,
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
        let pubky_client = Pubky::new().map_err(|e| OrchestratorError::Internal(e.to_string()))?;
        Self::with_client(config, pubky_client).await
    }

    /// Create a new BackupManager that backs up from a local Pubky testnet
    /// instead of mainnet.
    ///
    /// The testnet must be running on this machine with its well-known ports, as
    /// started by `pubky-testnet` or this crate's `dev_testnet` example. Unless
    /// the config names one, sign-ins go through the testnet's HTTP relay.
    ///
    /// Otherwise behaves like [`BackupManager::new`].
    ///
    /// # Errors
    ///
    /// Returns `OrchestratorError` if storage initialization fails.
    pub async fn testnet(mut config: BackupManagerConfig) -> Result<Self, OrchestratorError> {
        let pubky_client =
            Pubky::testnet().map_err(|e| OrchestratorError::Internal(e.to_string()))?;
        if config.http_relay.is_none() {
            let relay = TESTNET_HTTP_RELAY
                .parse()
                .map_err(|e| OrchestratorError::Internal(format!("Invalid relay URL: {}", e)))?;
            config.http_relay = Some(relay);
        }

        Self::with_client(config, pubky_client).await
    }

    /// Create a new BackupManager that makes all its network calls through the
    /// given Pubky client, e.g. one configured for a testnet or with custom timeouts.
    ///
    /// Otherwise behaves like [`BackupManager::new`].
    ///
    /// # Errors
    ///
    /// Returns `OrchestratorError` if storage initialization fails.
    pub async fn with_client(
        config: BackupManagerConfig,
        pubky_client: Pubky,
    ) -> Result<Self, OrchestratorError> {
        // Initialize storage
        let storage = if let Some(ref data_dir) = config.data_dir {
            AppStorage::new_with_path(data_dir)?
        } else {
            AppStorage::new()?
        };
        let storage = Arc::new(storage);
        let sync_interval = SyncInterval::load(storage.clone(), config.sync_interval_secs);

        // Create update broadcast channel for external subscribers
        let (update_tx, _) = broadcast::channel(100);

        // Create shared status channel - all controllers send to this channel
        let (status_tx, status_rx) = broadcast::channel(100);

        let manager = BackupManager {
            inner: Arc::new(RwLock::new(ManagerInner {
                keys: HashMap::new(),
                new_key_sign_in: None,
                is_shut_down: false,
            })),
            storage,
            config,
            pubky_client: Arc::new(pubky_client),
            update_tx,
            status_tx,
            sync_interval: Arc::new(RwLock::new(sync_interval)),
        };

        manager.spawn_status_listener(status_rx);

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
    /// This method checks that the key has a homeserver (it need not have any data yet),
    /// then starts syncing. If backup data already exists on disk, it resumes
    /// from where it left off.
    ///
    /// # Errors
    ///
    /// Returns `OrchestratorError::KeyAlreadyExists` if the key is already being backed up.
    /// Returns `OrchestratorError::KeyLimitReached` if MAX_KEYS limit is reached.
    /// Returns `OrchestratorError::HomeserverNotFound` if the key has no homeserver.
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

        self.validate_key(&pubky).await?;

        // Clear inactive marker if re-adding a previously removed key
        let key_storage = self.storage.key_storage(&pubky)?;
        key_storage.clear_inactive().await.map_err(|e| {
            OrchestratorError::Internal(format!("Failed to clear inactive marker: {}", e))
        })?;

        self.start_controller(pubky.clone(), false).await?;

        info!("Added key for backup: {}", pubky);
        Ok(())
    }

    /// Back up a key if it isn't yet, or retry it if its backup is in error.
    ///
    /// Unlike [`add_key`](Self::add_key), a key that is already backed up is
    /// not an error: this is the method for "make sure this key is backed up".
    ///
    /// # Errors
    ///
    /// Returns the errors of [`add_key`](Self::add_key) or
    /// [`retry_errored_key`](Self::retry_errored_key).
    pub async fn ensure_backed_up(&self, pubky: &PublicKey) -> Result<(), OrchestratorError> {
        match self.get_key_state(pubky) {
            None => match self.add_key(pubky.clone()).await {
                // Added in the meantime by someone else
                Ok(()) | Err(OrchestratorError::KeyAlreadyExists(_)) => Ok(()),
                Err(e) => Err(e),
            },
            Some(state) if state.status == KeyStatus::Error => self.retry_errored_key(pubky).await,
            Some(_) => Ok(()),
        }
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

        for pending in [managed_key.pending_sign_in, managed_key.pending_mirror]
            .into_iter()
            .flatten()
        {
            pending.cancel();
        }

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
    fn clear_last_pubky_if_empty(&self) {
        let no_keys_left = self.inner.read().keys.is_empty();
        if no_keys_left {
            if let Err(e) = self.storage.clear_last_pubky() {
                warn!("Failed to clear last pubky: {}", e);
            }
        }
    }

    /// Stop syncing a key and mark it inactive. Backed-up data is preserved on disk
    /// but the key will not be resumed on next launch. A signed-in key is signed
    /// out, on its mirror too.
    ///
    /// Use `delete_key()` to also remove all backed-up data.
    ///
    /// # Errors
    ///
    /// Returns `OrchestratorError::KeyNotFound` if the key is not being backed up.
    pub async fn remove_key(&self, pubky: &PublicKey) -> Result<(), OrchestratorError> {
        self.stop_controller(pubky)?;
        self.discard_session_of_removed_key(pubky);
        self.discard_mirror_session_of_removed_key(pubky);

        // Mark the key as inactive so it won't be resumed on next launch
        let key_storage = self.storage.key_storage(pubky)?;
        key_storage.mark_inactive().await.map_err(|e| {
            OrchestratorError::Internal(format!("Failed to mark key inactive: {}", e))
        })?;

        self.clear_last_pubky_if_empty();
        info!("Stopped and deactivated backup for key: {}", pubky);
        Ok(())
    }

    /// Remove a key AND delete all backed-up data from disk. A signed-in key is
    /// signed out, on its mirror too. Data on the mirror stays.
    ///
    /// Awaits the controller task to ensure all in-flight IO completes
    /// before deleting files from disk.
    ///
    /// # Errors
    ///
    /// Returns `OrchestratorError::KeyNotFound` if the key is not being backed up.
    pub async fn delete_key(&self, pubky: &PublicKey) -> Result<(), OrchestratorError> {
        let handle = self.stop_controller(pubky)?;
        self.discard_session_of_removed_key(pubky);
        self.discard_mirror_session_of_removed_key(pubky);
        self.clear_last_pubky_if_empty();

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
    /// Re-validates (homeserver discovery), then replaces the errored controller
    /// with a new one. If re-validation fails, the error propagates to the caller
    /// and the errored controller keeps running, so the key is still backed up
    /// and retried on the next sync interval.
    ///
    /// # Errors
    ///
    /// Returns `OrchestratorError::KeyNotFound` if the key is not being backed up.
    /// Returns validation errors if re-validation fails.
    pub async fn retry_errored_key(&self, pubky: &PublicKey) -> Result<(), OrchestratorError> {
        if !self.inner.read().keys.contains_key(pubky) {
            return Err(OrchestratorError::KeyNotFound(pubky.to_string()));
        }

        // Re-validate before touching the errored controller
        self.validate_key(pubky).await?;

        // Remove the errored controller and let it end, so that it no longer
        // writes the key's files and its last status comes before any of the
        // new controller's
        let handle = self.stop_controller(pubky)?;
        if let Err(e) = handle.await {
            warn!("Controller task for {} finished with error: {}", pubky, e);
        }

        // Restart the controller
        self.start_controller(pubky.clone(), false).await?;
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
        inner.keys.get(pubky).map(ManagedKey::state)
    }

    /// Get current state of all managed keys.
    ///
    /// Returns a map of pubky string (z32) to KeyState.
    pub fn get_all_key_states(&self) -> HashMap<String, KeyState> {
        let inner = self.inner.read();
        inner
            .keys
            .iter()
            .map(|(pubky, managed)| (pubky.z32(), managed.state()))
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
            .any(|k| matches!(k.sync.status, KeyStatus::Syncing { .. }))
    }

    /// Check if any key has an error.
    pub fn any_error(&self) -> bool {
        let inner = self.inner.read();
        inner
            .keys
            .values()
            .any(|k| matches!(k.sync.status, KeyStatus::Error))
    }

    /// Check if any key is running (not stopped).
    pub fn any_running(&self) -> bool {
        let inner = self.inner.read();
        inner
            .keys
            .values()
            .any(|k| !matches!(k.sync.status, KeyStatus::Stopped))
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

        let snapshot_count = self.storage.count_snapshots(pubky).await;
        log_activity(
            &self.storage,
            pubky,
            ActivityType::SnapshotCreated,
            format!("Created snapshot v{}", snapshot_count),
        )
        .await;

        Ok(path)
    }

    /// Get recent activity entries for a key.
    pub async fn get_activity(&self, pubky: &PublicKey, limit: usize) -> Vec<ActivityEntry> {
        self.storage.read_activity(pubky, limit).await
    }

    /// Get the root data directory path (e.g. `~/.pubky-backup`).
    pub fn data_dir(&self) -> &Path {
        self.storage.data_dir()
    }

    /// Get the current keys directory path.
    pub fn keys_dir(&self) -> &Path {
        self.storage.keys_dir()
    }

    /// Get the snapshots directory path for a key.
    pub fn snapshots_dir(&self, pubky: &PublicKey) -> PathBuf {
        self.storage.snapshots_dir(pubky)
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
    pub fn write_last_pubky(&self, pubky: &PublicKey) -> Result<(), OrchestratorError> {
        Ok(self.storage.write_last_pubky(pubky)?)
    }

    /// Read the last used pubky from storage.
    ///
    /// Returns `None` if no last pubky was saved.
    pub fn read_last_pubky(&self) -> Result<Option<PublicKey>, OrchestratorError> {
        Ok(self.storage.read_last_pubky()?)
    }

    /// List pubky directories that have backup data stored.
    ///
    /// Returns a list of z32-encoded pubky strings.
    pub fn list_pubky_directories(&self) -> Result<Vec<String>, OrchestratorError> {
        Ok(self.storage.list_pubky_directories()?)
    }

    /// Gracefully shutdown all backups, awaiting controller tasks.
    pub async fn shutdown(&self) {
        // Keys added from here on, e.g. by a sign-in that was just approved,
        // would otherwise outlive the shutdown
        self.inner.write().is_shut_down = true;
        self.cancel_sign_in_for_new_key();
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
    pub fn set_sync_interval(&self, interval_secs: u64) -> Result<(), OrchestratorError> {
        SyncInterval::validate(interval_secs)?;

        // Persist first, so that a failure leaves the interval unchanged
        self.storage.write_sync_interval(interval_secs)?;

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

    /// Check that a key has a homeserver to back up from.
    ///
    /// The key need not have any data yet: it may be new, or all its data may
    /// be private, which only shows once it is signed in.
    ///
    /// Skipped offline.
    async fn validate_key(&self, pubky: &PublicKey) -> Result<(), OrchestratorError> {
        if self.config.offline {
            return Ok(());
        }

        let timeout_secs = self.config.validation_timeout_secs;
        discovery::discover_homeserver(&self.pubky_client, pubky, timeout_secs).await?;
        Ok(())
    }

    /// Run `update` on a tracked key under the state lock, then broadcast the
    /// key's state if its sign-in or mirror status changed.
    ///
    /// Changes to what is stored for the key belong inside `update`, so that
    /// they can't interleave with another change of status. `update` must not
    /// use the manager's state itself. Returns `None` if the key is not tracked.
    fn update_key<R>(
        &self,
        pubky: &PublicKey,
        update: impl FnOnce(&mut ManagedKey) -> R,
    ) -> Option<R> {
        let (result, changed_state) = {
            let mut inner = self.inner.write();
            let managed = inner.keys.get_mut(pubky)?;
            let previous = (managed.auth.clone(), managed.mirror.clone());
            let result = update(managed);
            let changed = managed.auth != previous.0 || managed.mirror != previous.1;
            (result, changed.then(|| managed.state()))
        };

        if let Some(state) = changed_state {
            let _ = self.update_tx.send(KeyUpdate {
                pubky: pubky.clone(),
                state,
            });
        }
        Some(result)
    }

    /// End a session that was already forgotten locally, on the homeserver its
    /// stored secret names: the key's own, or its mirror.
    fn end_session_on_homeserver(&self, secret: String, pubky: &PublicKey) {
        if self.config.offline {
            return;
        }
        auth::end_stored_session(secret, pubky.clone(), self.pubky_client.as_ref().clone());
    }

    /// Register a key in the manager: insert it into the keys map and broadcast
    /// its state. Its sign-in and mirror status come from what is stored for it.
    ///
    /// # Errors
    ///
    /// Returns `OrchestratorError::Internal`, and stops the key's controller, if
    /// the manager has shut down.
    fn register_key(
        &self,
        pubky: PublicKey,
        sync: SyncState,
        control_tx: mpsc::Sender<ControllerCommand>,
        task_handle: JoinHandle<()>,
    ) -> Result<(), OrchestratorError> {
        let managed = ManagedKey {
            control_tx,
            sync,
            auth: self.stored_auth_status(&pubky),
            mirror: self.stored_mirror_status(&pubky),
            task_handle,
            pending_sign_in: None,
            pending_mirror: None,
        };
        let state = managed.state();

        {
            let mut inner = self.inner.write();
            if inner.is_shut_down {
                managed.task_handle.abort();
                return Err(OrchestratorError::Internal(
                    "The backup manager has shut down".to_string(),
                ));
            }
            inner.keys.insert(pubky.clone(), managed);
        }

        let receivers = self.update_tx.send(KeyUpdate {
            pubky: pubky.clone(),
            state,
        });
        debug!("Broadcast state for {} (receivers: {:?})", pubky, receivers);
        Ok(())
    }

    /// Start the backup controller for a key.
    ///
    /// When `stagger` is true, a random initial delay (0..sync_interval) is applied
    /// to avoid thundering-herd on startup. User-initiated actions (add_key, retry)
    /// pass `false` so the first sync starts immediately.
    async fn start_controller(
        &self,
        pubky: PublicKey,
        stagger: bool,
    ) -> Result<(), OrchestratorError> {
        let (control_tx, control_rx) = mpsc::channel(5);

        let initial_size = self.storage.calculate_pubky_size(&pubky).await;

        let initial_state = SyncState {
            status: KeyStatus::Starting,
            data_size: initial_size,
            next_sync: Some(next_sync_time(self.sync_interval.read().get())),
            ..Default::default()
        };

        let key_count = self.inner.read().keys.len();

        // Stagger resumed keys at startup to avoid thundering-herd.
        // Window = key_count * STAGGER_MULTIPLE, so 10 keys spread their stagger over 10*STAGGER_MULTIPLE.
        let initial_delay = if stagger && !self.config.offline && key_count > 0 {
            let window_secs = (key_count * STAGGER_MULTIPLE) as u64;
            std::time::Duration::from_secs(rand::random::<u64>() % window_secs)
        } else {
            std::time::Duration::ZERO
        };

        let controller = BackupController::new(
            pubky.clone(),
            self.storage.clone(),
            self.pubky_client.clone(),
            Some(control_rx),
            Some(self.status_tx.clone()),
            self.sync_interval.read().subscribe(),
        )
        .with_initial_delay(initial_delay)
        .offline(self.config.offline);

        let task_handle = tokio::spawn(async move {
            controller.run().await;
        });

        self.register_key(pubky, initial_state, control_tx, task_handle)
    }

    /// Resume all keys that have stored data on disk
    async fn resume_stored_keys(&self) {
        let key_dirs = match self.storage.list_pubky_directories() {
            Ok(dirs) => dirs,
            Err(e) => {
                self.log_resume_failure(format!("Failed to list key directories: {}", e))
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
                    self.log_resume_failure(format!(
                        "Invalid pubky directory name {}: {}",
                        pubky_str, e
                    ))
                    .await;
                    continue;
                }
            };

            // Skip validation on resume — these keys were validated when first added
            // and already have data on disk. The controller's sync loop will handle
            // transient network errors with its built-in retry mechanism.
            if let Err(e) = self.start_controller(pubky.clone(), true).await {
                self.log_resume_failure(format!("Failed to resume key {}: {}", pubky, e))
                    .await;
            }
        }
    }

    /// Log a failure to resume stored keys, and record it in the global error log.
    async fn log_resume_failure(&self, message: String) {
        warn!("{}", message);
        let _ = self
            .storage
            .write_global_error("resume_stored_keys", &message)
            .await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orchestrator::types::KeyError;
    use crate::storage::AppStorage;
    use crate::sync::{DEFAULT_SYNC_INTERVAL_SECONDS, MIN_SYNC_INTERVAL_SECONDS};
    use crate::TEST_PUBKY;
    use std::str::FromStr;
    use std::time::Duration;
    use tempfile::TempDir;
    use url::Url;

    /// An offline config, so that tests make no network calls.
    pub(super) fn create_test_config(temp_dir: &TempDir) -> BackupManagerConfig {
        BackupManagerConfig {
            data_dir: Some(temp_dir.path().to_path_buf()),
            offline: true,
            ..Default::default()
        }
    }

    /// A client that would only ever reach a local testnet, never mainnet.
    pub(super) fn create_test_client() -> Pubky {
        Pubky::testnet().unwrap()
    }

    /// Create a temp dir and a BackupManager configured for testing.
    pub(super) async fn setup() -> (TempDir, BackupManager) {
        let temp_dir = TempDir::new().unwrap();
        let (_, manager) = setup_with_dir(&temp_dir).await;
        (temp_dir, manager)
    }

    /// Create a BackupManager using an existing temp dir (for multi-lifecycle tests).
    pub(super) async fn setup_with_dir(temp_dir: &TempDir) -> (BackupManagerConfig, BackupManager) {
        let config = create_test_config(temp_dir);
        let manager = BackupManager::with_client(config.clone(), create_test_client())
            .await
            .unwrap();
        (config, manager)
    }

    pub(super) fn test_pubky() -> PublicKey {
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

    /// The command sender of a key's current controller.
    fn control_tx(manager: &BackupManager, pubky: &PublicKey) -> mpsc::Sender<ControllerCommand> {
        manager.inner.read().keys[pubky].control_tx.clone()
    }

    #[tokio::test]
    async fn test_ensure_backed_up_adds_a_new_key() {
        let (_dir, manager) = setup().await;
        let pubky = test_pubky();

        manager.ensure_backed_up(&pubky).await.unwrap();

        assert!(manager.get_key_state(&pubky).is_some());
    }

    #[tokio::test]
    async fn test_ensure_backed_up_leaves_a_backed_up_key_alone() {
        let (_dir, manager) = setup().await;
        let pubky = test_pubky();
        manager.add_key(pubky.clone()).await.unwrap();
        let before = control_tx(&manager, &pubky);

        manager.ensure_backed_up(&pubky).await.unwrap();

        // The key kept its controller
        assert!(before.same_channel(&control_tx(&manager, &pubky)));
        assert_eq!(manager.get_keys().len(), 1);
    }

    #[tokio::test]
    async fn test_ensure_backed_up_retries_an_errored_key() {
        let (_dir, manager) = setup().await;
        let pubky = test_pubky();
        manager.add_key(pubky.clone()).await.unwrap();
        let errored = SyncState {
            status: KeyStatus::Error,
            error: Some(KeyError {
                message: "boom".to_string(),
            }),
            ..Default::default()
        };
        manager.inner.write().apply_sync_state(&pubky, errored);
        let before = control_tx(&manager, &pubky);

        manager.ensure_backed_up(&pubky).await.unwrap();

        // A new controller replaced the errored one
        assert!(!before.same_channel(&control_tx(&manager, &pubky)));
        assert_ne!(
            manager.get_key_state(&pubky).unwrap().status,
            KeyStatus::Error
        );
    }

    #[tokio::test]
    async fn test_failed_retry_keeps_the_errored_key_backed_up() {
        let temp_dir = TempDir::new().unwrap();
        // Validated against a testnet that isn't running, so discovery fails
        let config = BackupManagerConfig {
            offline: false,
            validation_timeout_secs: 1,
            ..create_test_config(&temp_dir)
        };
        let manager = BackupManager::with_client(config, create_test_client())
            .await
            .unwrap();
        let pubky = test_pubky();
        manager
            .start_controller(pubky.clone(), false)
            .await
            .unwrap();
        let errored = SyncState {
            status: KeyStatus::Error,
            error: Some(KeyError {
                message: "boom".to_string(),
            }),
            ..Default::default()
        };
        manager.inner.write().apply_sync_state(&pubky, errored);
        let before = control_tx(&manager, &pubky);

        let result = manager.retry_errored_key(&pubky).await;

        assert!(
            matches!(result, Err(OrchestratorError::HomeserverNotFound(_))),
            "Unexpected result: {result:?}"
        );
        // The errored controller is still there to retry on the next interval
        assert!(before.same_channel(&control_tx(&manager, &pubky)));
        assert!(!before.is_closed());
    }

    #[tokio::test]
    async fn test_retry_of_an_untracked_key_is_refused() {
        let (_dir, manager) = setup().await;

        let result = manager.retry_errored_key(&test_pubky()).await;

        assert!(
            matches!(result, Err(OrchestratorError::KeyNotFound(_))),
            "Unexpected result: {result:?}"
        );
    }

    #[tokio::test]
    async fn test_shut_down_manager_closes_its_status_channel_when_dropped() {
        let (_dir, manager) = setup().await;
        manager.add_key(test_pubky()).await.unwrap();
        let mut status_rx = manager.status_tx.subscribe();

        manager.shutdown().await;
        drop(manager);

        // Nothing else holds a sender, the listener included, so the channel closes
        let closed = tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                match status_rx.recv().await {
                    Ok(_) | Err(broadcast::error::RecvError::Lagged(_)) => {}
                    Err(broadcast::error::RecvError::Closed) => return,
                }
            }
        })
        .await;
        assert!(closed.is_ok(), "The status channel should close");
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
            manager.write_last_pubky(&pubky).unwrap();
        }

        // Create new manager and read back
        {
            let (_, manager) = setup_with_dir(&temp_dir).await;
            let read_pubky = manager.read_last_pubky().unwrap();
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
        manager.set_sync_interval(600).unwrap();
        assert_eq!(manager.get_sync_interval(), 600);

        // Verify persisted to disk
        let stored = manager.storage.read_sync_interval();
        assert_eq!(stored, Some(600));
    }

    #[tokio::test]
    async fn test_set_sync_interval_rejects_zero() {
        let (_dir, manager) = setup().await;

        let result = manager.set_sync_interval(0);
        assert!(result.is_err(), "Should reject zero interval");

        // Interval should be unchanged
        assert_eq!(manager.get_sync_interval(), DEFAULT_SYNC_INTERVAL_SECONDS);
    }

    #[tokio::test]
    async fn test_set_sync_interval_rejects_too_small() {
        let (_dir, manager) = setup().await;

        let result = manager.set_sync_interval(9);
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
            .unwrap();
        assert_eq!(manager.get_sync_interval(), MIN_SYNC_INTERVAL_SECONDS);
    }

    #[tokio::test]
    async fn test_set_sync_interval_no_controllers() {
        let (_dir, manager) = setup().await;

        // No keys added — should succeed as a no-op for controller restarts
        manager.set_sync_interval(600).unwrap();
        assert_eq!(manager.get_sync_interval(), 600);
    }

    #[tokio::test]
    async fn test_set_sync_interval_keeps_controllers_running() {
        let (_dir, manager) = setup().await;

        let pubky = test_pubky();
        manager.add_key(pubky.clone()).await.unwrap();

        // Change interval — controller should keep running (no restart)
        manager.set_sync_interval(600).unwrap();

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
            manager.set_sync_interval(900).unwrap();
        }

        // Second: create a new manager from the same data dir — should load 900
        let (_, manager) = setup_with_dir(&temp_dir).await;
        assert_eq!(manager.get_sync_interval(), 900);
    }

    #[tokio::test]
    async fn test_first_key_starts_immediately_without_stagger_delay() {
        // Test that the first key added starts syncing immediately (no stagger delay).
        // This ensures good UX for single-key users.
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
            manager.storage.write_sync_interval(5).unwrap();
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
        let config = BackupManagerConfig {
            sync_interval_secs: 900,
            ..create_test_config(&temp_dir)
        };
        let manager = BackupManager::with_client(config, create_test_client())
            .await
            .unwrap();

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
        manager.write_last_pubky(&pubky).unwrap();

        // Drop the manager — simulates app restart
        manager.shutdown().await;
        drop(manager);

        // Read using raw storage — this is what Tauri commands should fall back to
        // when the manager hasn't finished initialising yet.
        let storage = AppStorage::new_with_path(temp_dir.path()).unwrap();

        let last = storage.read_last_pubky().unwrap();
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

    #[tokio::test]
    async fn test_testnet_manager_signs_in_through_local_relay() {
        let temp_dir = TempDir::new().unwrap();

        let manager = BackupManager::testnet(create_test_config(&temp_dir))
            .await
            .unwrap();

        let relay = manager.config.http_relay.as_ref().unwrap();
        assert_eq!(relay.as_str(), TESTNET_HTTP_RELAY);
    }

    #[tokio::test]
    async fn test_testnet_manager_keeps_configured_relay() {
        let temp_dir = TempDir::new().unwrap();
        let relay: Url = "http://relay.example/link/".parse().unwrap();
        let config = BackupManagerConfig {
            http_relay: Some(relay.clone()),
            ..create_test_config(&temp_dir)
        };

        let manager = BackupManager::testnet(config).await.unwrap();

        assert_eq!(manager.config.http_relay, Some(relay));
    }
}
