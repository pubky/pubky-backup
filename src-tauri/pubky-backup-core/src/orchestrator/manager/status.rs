//! Applying controller status updates to the manager's state.
//!
//! A single listener task receives the status updates of all controllers,
//! turns each into the key's new [`SyncState`], logs the activity it implies
//! and tells subscribers.

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use log::{debug, warn};
use parking_lot::RwLock;
use pubky::PublicKey;
use tokio::sync::broadcast;

use super::{BackupManager, ManagerInner};
use crate::orchestrator::sync_interval::SyncInterval;
use crate::orchestrator::types::{
    ActivityEntry, ActivityType, AuthStatus, KeyError, KeyState, KeyStatus, KeyUpdate, MirrorStatus,
};
use crate::storage::AppStorage;
use crate::sync::{ControllerStatus, ControllerUpdate, SessionSecret};

/// The part of a key's state that its controller reports: everything but the
/// sign-in and mirror status, which the manager keeps.
#[derive(Clone, Debug)]
pub(super) struct SyncState {
    pub(super) status: KeyStatus,
    pub(super) data_size: u64,
    pub(super) last_sync: Option<u64>,
    pub(super) next_sync: Option<u64>,
    pub(super) error: Option<KeyError>,
}

impl Default for SyncState {
    fn default() -> Self {
        Self {
            status: KeyStatus::Starting,
            data_size: 0,
            last_sync: None,
            next_sync: None,
            error: None,
        }
    }
}

impl SyncState {
    /// The key's full state, given its sign-in and mirror status.
    pub(super) fn into_key_state(self, auth: AuthStatus, mirror: MirrorStatus) -> KeyState {
        KeyState {
            status: self.status,
            data_size: self.data_size,
            last_sync: self.last_sync,
            next_sync: self.next_sync,
            error: self.error,
            auth,
            mirror,
        }
    }
}

/// Handles a controller status update: resolves activity, writes it to the log,
/// and maps the controller status to the key's new [`SyncState`].
struct StatusHandler {
    pubky: PublicKey,
    storage: Arc<AppStorage>,
    /// The key's state before the update
    previous: SyncState,
    sync_interval_secs: u64,
}

impl StatusHandler {
    /// The key's state with a new status, carrying forward its sizes and times.
    fn base_state(&self, status: KeyStatus) -> SyncState {
        SyncState {
            status,
            error: None,
            ..self.previous.clone()
        }
    }

    /// Process a controller status update: log activity and produce the new [`SyncState`].
    async fn handle(&self, status: &ControllerStatus) -> SyncState {
        if let Some(entry) = self.resolve_activity(status).await {
            if let Err(e) = self.storage.write_activity(&self.pubky, &entry).await {
                warn!("Failed to write activity for {}: {}", self.pubky, e);
            }
        }

        match status {
            ControllerStatus::Starting => self.base_state(KeyStatus::Starting),
            ControllerStatus::Syncing { events_processed } => {
                let data_size = if *events_processed > 0 {
                    self.storage.calculate_pubky_size(&self.pubky).await
                } else {
                    self.previous.data_size
                };

                SyncState {
                    data_size,
                    ..self.base_state(KeyStatus::Syncing {
                        events_processed: *events_processed,
                    })
                }
            }
            ControllerStatus::Idle => {
                let data_size = self.storage.calculate_pubky_size(&self.pubky).await;
                let now = current_unix_timestamp();

                SyncState {
                    data_size,
                    last_sync: Some(now),
                    next_sync: Some(next_sync_time(self.sync_interval_secs)),
                    ..self.base_state(KeyStatus::Idle)
                }
            }
            ControllerStatus::Ended => self.base_state(KeyStatus::Stopped),
            // Never reached: the listener handles notices about the key's
            // session or mirror with its sign-in or mirror status instead of
            // calling `handle`. Returning the previous state keeps the match
            // exhaustive without a panic.
            ControllerStatus::SessionRejected { .. }
            | ControllerStatus::MirrorSessionRejected { .. }
            | ControllerStatus::MirrorFailed { .. }
            | ControllerStatus::MirrorRecovered => self.previous.clone(),
            ControllerStatus::Error { message } => SyncState {
                error: Some(KeyError {
                    message: message.clone(),
                }),
                ..self.base_state(KeyStatus::Error)
            },
        }
    }

    /// Determine which activity entry (if any) to log for this status transition.
    async fn resolve_activity(&self, status: &ControllerStatus) -> Option<ActivityEntry> {
        let now = current_unix_timestamp();
        match status {
            ControllerStatus::Idle => {
                // A key is "new" (first-ever sync) when last_sync is None (no prior
                // Idle this session) and the activity log has no InitialBackup entry.
                // We scan the most recent entries for a prior InitialBackup.
                // 50 is generous — InitialBackup is always among the first entries.
                const INITIAL_BACKUP_SCAN_LIMIT: usize = 50;
                let is_initial = self.previous.last_sync.is_none()
                    && !self
                        .storage
                        .read_activity(&self.pubky, INITIAL_BACKUP_SCAN_LIMIT)
                        .await
                        .iter()
                        .any(|e| e.activity_type == ActivityType::InitialBackup);

                if is_initial {
                    let count = match &self.previous.status {
                        KeyStatus::Syncing { events_processed } => *events_processed,
                        _ => 0,
                    };
                    let message = format!(
                        "Initial backup successful — {}",
                        format_files_backed_up(count)
                    );
                    Some(ActivityEntry {
                        activity_type: ActivityType::InitialBackup,
                        message,
                        timestamp: now,
                    })
                } else if let KeyStatus::Syncing { events_processed } = &self.previous.status {
                    if *events_processed > 0 {
                        Some(ActivityEntry {
                            activity_type: ActivityType::FilesBackedUp,
                            message: format_files_backed_up(*events_processed),
                            timestamp: now,
                        })
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            ControllerStatus::Error { message } => Some(ActivityEntry {
                activity_type: ActivityType::SyncFailed,
                message: message.clone(),
                timestamp: now,
            }),
            _ => None,
        }
    }
}

/// Get current unix timestamp in seconds.
pub(crate) fn current_unix_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

fn format_files_backed_up(count: usize) -> String {
    format!(
        "{} new {} backed up",
        count,
        if count == 1 { "file" } else { "files" }
    )
}

/// Calculate next sync time.
pub(crate) fn next_sync_time(interval_secs: u64) -> u64 {
    current_unix_timestamp() + interval_secs
}

/// Append an entry to a key's activity log. Failures are logged, not returned.
pub(super) async fn log_activity(
    storage: &AppStorage,
    pubky: &PublicKey,
    activity_type: ActivityType,
    message: String,
) {
    let entry = ActivityEntry {
        activity_type,
        message,
        timestamp: current_unix_timestamp(),
    };
    if let Err(e) = storage.write_activity(pubky, &entry).await {
        warn!("Failed to write activity for {}: {}", pubky, e);
    }
}

/// Applies the status updates of all controllers to the manager's state.
///
/// Holds only what that takes, and in particular no status sender: once the
/// manager has shut down its controllers and is dropped, the status channel
/// closes and the listener exits with it.
struct StatusListener {
    inner: Arc<RwLock<ManagerInner>>,
    storage: Arc<AppStorage>,
    sync_interval: Arc<RwLock<SyncInterval>>,
    update_tx: broadcast::Sender<KeyUpdate>,
}

impl BackupManager {
    /// Spawn the task that applies the status updates of all controllers to
    /// this manager's state.
    pub(super) fn spawn_status_listener(&self, status_rx: broadcast::Receiver<ControllerUpdate>) {
        StatusListener {
            inner: self.inner.clone(),
            storage: self.storage.clone(),
            sync_interval: self.sync_interval.clone(),
            update_tx: self.update_tx.clone(),
        }
        .spawn(status_rx);
    }
}

impl StatusListener {
    /// Run the listener as a task until the status channel closes.
    fn spawn(self, mut status_rx: broadcast::Receiver<ControllerUpdate>) {
        tokio::spawn(async move {
            loop {
                match status_rx.recv().await {
                    Ok(update) => self.apply(update).await,
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        warn!("Status listener lagged, skipped {} messages", n);
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        debug!("Status listener channel closed");
                        break;
                    }
                }
            }
        });
    }

    /// Apply a controller's status update to its key, and tell subscribers.
    async fn apply(&self, update: ControllerUpdate) {
        let ControllerUpdate { pubky, status } = update;
        // Notices about the key's session or mirror change its sign-in or
        // mirror status, which the manager keeps, rather than its sync status
        match &status {
            ControllerStatus::SessionRejected { secret } => {
                self.expire_rejected_session(&pubky, secret).await;
                return;
            }
            ControllerStatus::MirrorSessionRejected { secret } => {
                self.expire_rejected_mirror_session(&pubky, secret).await;
                return;
            }
            ControllerStatus::MirrorFailed { message } => {
                self.note_mirror_failure(&pubky, message).await;
                return;
            }
            ControllerStatus::MirrorRecovered => {
                self.note_mirror_recovery(&pubky);
                return;
            }
            _ => {}
        }

        // A controller ends only after `stop_controller` has stopped tracking it,
        // so its Ended never describes the key's current controller: the key is
        // gone, or was retried or re-added with a new controller since
        if matches!(status, ControllerStatus::Ended) {
            debug!("Ignoring end of an untracked controller of {}", pubky);
            return;
        }

        // Skip updates for keys that have been removed
        let Some(previous) = self.inner.read().sync_state(&pubky) else {
            debug!("Ignoring status update for removed key {}", pubky);
            return;
        };
        let handler = StatusHandler {
            pubky: pubky.clone(),
            storage: self.storage.clone(),
            previous,
            sync_interval_secs: self.sync_interval.read().get(),
        };
        let new_state = handler.handle(&status).await;

        // Skip the broadcast if the key was removed while handling the update
        let Some(state) = self.inner.write().apply_sync_state(&pubky, new_state) else {
            return;
        };
        let receivers = self.update_tx.send(KeyUpdate {
            pubky: pubky.clone(),
            state,
        });
        debug!(
            "Broadcast status update for {} (receivers: {:?})",
            pubky, receivers
        );
    }

    /// Mark a key's session as expired after its homeserver rejected it, and
    /// tell subscribers.
    async fn expire_rejected_session(&self, pubky: &PublicKey, secret: &SessionSecret) {
        let expired = self
            .inner
            .write()
            .expire_session(&self.storage, pubky, secret);
        let Some(state) = expired else {
            return;
        };
        log_activity(
            &self.storage,
            pubky,
            ActivityType::SessionExpired,
            "Session expired — sign in again to back up private data".to_string(),
        )
        .await;
        let _ = self.update_tx.send(KeyUpdate {
            pubky: pubky.clone(),
            state,
        });
    }

    /// Mark a key's mirror session as expired after the mirror rejected it,
    /// and tell subscribers.
    async fn expire_rejected_mirror_session(&self, pubky: &PublicKey, secret: &SessionSecret) {
        let expired = self
            .inner
            .write()
            .expire_mirror_session(&self.storage, pubky, secret);
        let Some(state) = expired else {
            return;
        };
        log_activity(
            &self.storage,
            pubky,
            ActivityType::MirrorSessionExpired,
            "Mirror session expired — approve the mirror again to keep mirroring".to_string(),
        )
        .await;
        let _ = self.update_tx.send(KeyUpdate {
            pubky: pubky.clone(),
            state,
        });
    }

    /// Mark a key's mirror as failing, log the failure and tell subscribers.
    async fn note_mirror_failure(&self, pubky: &PublicKey, message: &str) {
        let Some(state) = self
            .inner
            .write()
            .apply_mirror_failure(pubky, Some(message))
        else {
            return;
        };
        log_activity(
            &self.storage,
            pubky,
            ActivityType::MirrorFailed,
            message.to_string(),
        )
        .await;
        let _ = self.update_tx.send(KeyUpdate {
            pubky: pubky.clone(),
            state,
        });
    }

    /// Mark a key's mirror as working again, and tell subscribers.
    fn note_mirror_recovery(&self, pubky: &PublicKey) {
        if let Some(state) = self.inner.write().apply_mirror_failure(pubky, None) {
            let _ = self.update_tx.send(KeyUpdate {
                pubky: pubky.clone(),
                state,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sync::DEFAULT_SYNC_INTERVAL_SECONDS;
    use std::str::FromStr;
    use tempfile::TempDir;

    const TEST_PUBKY: &str = "z4e8s17cou9qmuia76iczxkwakp8xnck5qrfbzh8rbpiq1sugbuo";

    /// Create a fresh temp storage and pubky for a test.
    fn setup() -> (TempDir, Arc<AppStorage>, PublicKey) {
        let temp_dir = TempDir::new().unwrap();
        let storage = Arc::new(AppStorage::new_with_path(temp_dir.path()).unwrap());
        let pubky = PublicKey::from_str(TEST_PUBKY).unwrap();
        (temp_dir, storage, pubky)
    }

    /// Build a `StatusHandler` for a key in the given previous state.
    fn handler(
        pubky: &PublicKey,
        storage: &Arc<AppStorage>,
        data_size: u64,
        last_sync: Option<u64>,
        previous_status: KeyStatus,
    ) -> StatusHandler {
        StatusHandler {
            pubky: pubky.clone(),
            storage: storage.clone(),
            previous: SyncState {
                status: previous_status,
                data_size,
                last_sync,
                ..Default::default()
            },
            sync_interval_secs: DEFAULT_SYNC_INTERVAL_SECONDS,
        }
    }

    #[tokio::test]
    async fn test_idle_after_initial_sync_writes_initial_backup_activity() {
        let (_dir, storage, pubky) = setup();

        // last_sync is None and no InitialBackup in activity log → initial backup
        handler(&pubky, &storage, 0, None, KeyStatus::Starting)
            .handle(&ControllerStatus::Idle)
            .await;

        let entries = storage.read_activity(&pubky, 10).await;
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].activity_type, ActivityType::InitialBackup);
        assert_eq!(
            entries[0].message,
            "Initial backup successful — 0 new files backed up"
        );
    }

    #[tokio::test]
    async fn test_initial_backup_includes_file_count() {
        let (_dir, storage, pubky) = setup();

        let prev = KeyStatus::Syncing {
            events_processed: 18,
        };
        handler(&pubky, &storage, 0, None, prev)
            .handle(&ControllerStatus::Idle)
            .await;

        let entries = storage.read_activity(&pubky, 10).await;
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].activity_type, ActivityType::InitialBackup);
        assert_eq!(
            entries[0].message,
            "Initial backup successful — 18 new files backed up"
        );
    }

    #[tokio::test]
    async fn test_resumed_key_with_prior_initial_backup_writes_files_backed_up() {
        let (_dir, storage, pubky) = setup();

        // Simulate a resumed key: activity log already has an InitialBackup entry
        storage
            .write_activity(
                &pubky,
                &ActivityEntry {
                    activity_type: ActivityType::InitialBackup,
                    message: "Initial backup successful".to_string(),
                    timestamp: 1000,
                },
            )
            .await
            .unwrap();

        // last_sync is None (fresh app start) but InitialBackup exists in activity log
        let prev = KeyStatus::Syncing {
            events_processed: 3,
        };
        handler(&pubky, &storage, 0, None, prev)
            .handle(&ControllerStatus::Idle)
            .await;

        let entries = storage.read_activity(&pubky, 10).await;
        assert_eq!(entries.len(), 2);
        // Newest should be FilesBackedUp, NOT a second InitialBackup
        assert_eq!(entries[0].activity_type, ActivityType::FilesBackedUp);
        assert_eq!(entries[0].message, "3 new files backed up");
    }

    #[tokio::test]
    async fn test_idle_after_syncing_with_events_writes_files_backed_up_activity() {
        let (_dir, storage, pubky) = setup();

        let prev = KeyStatus::Syncing {
            events_processed: 5,
        };
        handler(&pubky, &storage, 500, Some(1000), prev)
            .handle(&ControllerStatus::Idle)
            .await;

        let entries = storage.read_activity(&pubky, 10).await;
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].activity_type, ActivityType::FilesBackedUp);
        assert_eq!(entries[0].message, "5 new files backed up");
    }

    #[tokio::test]
    async fn test_idle_after_syncing_one_event_writes_singular_file() {
        let (_dir, storage, pubky) = setup();

        let prev = KeyStatus::Syncing {
            events_processed: 1,
        };
        handler(&pubky, &storage, 500, Some(1000), prev)
            .handle(&ControllerStatus::Idle)
            .await;

        let entries = storage.read_activity(&pubky, 10).await;
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].message, "1 new file backed up");
    }

    #[tokio::test]
    async fn test_idle_after_syncing_zero_events_writes_no_activity() {
        let (_dir, storage, pubky) = setup();

        let prev = KeyStatus::Syncing {
            events_processed: 0,
        };
        handler(&pubky, &storage, 500, Some(1000), prev)
            .handle(&ControllerStatus::Idle)
            .await;

        let entries = storage.read_activity(&pubky, 10).await;
        assert!(entries.is_empty(), "No activity for zero-event sync");
    }

    #[tokio::test]
    async fn test_error_status_writes_sync_failed_activity() {
        let (_dir, storage, pubky) = setup();

        let prev = KeyStatus::Syncing {
            events_processed: 0,
        };
        handler(&pubky, &storage, 100, Some(500), prev)
            .handle(&ControllerStatus::Error {
                message: "Connection refused".to_string(),
            })
            .await;

        let entries = storage.read_activity(&pubky, 10).await;
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].activity_type, ActivityType::SyncFailed);
        assert_eq!(entries[0].message, "Connection refused");
    }

    #[tokio::test]
    async fn test_starting_and_ended_write_no_activity() {
        let (_dir, storage, pubky) = setup();

        let h = handler(&pubky, &storage, 0, None, KeyStatus::Starting);

        h.handle(&ControllerStatus::Starting).await;
        h.handle(&ControllerStatus::Ended).await;

        let entries = storage.read_activity(&pubky, 10).await;
        assert!(
            entries.is_empty(),
            "Starting and Ended should not produce activity entries"
        );
    }

    #[tokio::test]
    async fn test_handle_error_state_mapping() {
        let (_dir, storage, pubky) = setup();

        let state = handler(&pubky, &storage, 1000, None, KeyStatus::Starting)
            .handle(&ControllerStatus::Error {
                message: "Test error message".to_string(),
            })
            .await;

        assert!(matches!(state.status, KeyStatus::Error));
        assert_eq!(state.error.unwrap().message, "Test error message");
        assert_eq!(state.data_size, 1000);
    }

    #[tokio::test]
    async fn test_handle_starting_state_mapping() {
        let (_dir, storage, pubky) = setup();

        let state = handler(&pubky, &storage, 1234, None, KeyStatus::Starting)
            .handle(&ControllerStatus::Starting)
            .await;

        assert!(matches!(state.status, KeyStatus::Starting));
        assert_eq!(state.data_size, 1234);
        assert!(state.error.is_none());
    }

    #[tokio::test]
    async fn test_mirror_notices_keep_sync_state_and_write_no_activity() {
        let (_dir, storage, pubky) = setup();
        let prev = KeyStatus::Syncing {
            events_processed: 3,
        };
        let notices = [
            ControllerStatus::MirrorFailed {
                message: "Could not reach the mirror".to_string(),
            },
            ControllerStatus::MirrorRecovered,
            ControllerStatus::MirrorSessionRejected {
                secret: SessionSecret::new("secret".to_string()),
            },
        ];

        for notice in &notices {
            let state = handler(&pubky, &storage, 100, Some(500), prev.clone())
                .handle(notice)
                .await;

            // The backup itself carries on, so this is not a sync error; the
            // listener logs what these notices mean for the mirror
            assert_eq!(state.status, prev);
            assert!(state.error.is_none());
        }
        assert!(storage.read_activity(&pubky, 10).await.is_empty());
    }

    #[tokio::test]
    async fn test_starting_after_error_clears_the_error() {
        let (_dir, storage, pubky) = setup();
        let mut h = handler(&pubky, &storage, 0, None, KeyStatus::Error);
        h.previous.error = Some(KeyError {
            message: "old".to_string(),
        });

        let state = h.handle(&ControllerStatus::Starting).await;

        assert!(state.error.is_none());
    }
}
