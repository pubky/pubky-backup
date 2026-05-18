use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use log::{debug, warn};
use parking_lot::RwLock;
use pubky::PublicKey;
use tokio::sync::broadcast;

use super::sync_interval::SyncInterval;
use super::types::{
    ActivityEntry, ActivityType, KeyError, KeyErrorCode, KeyState, KeyStatus, KeyUpdate,
};
use crate::storage::AppStorage;
use crate::sync::ControllerStatus;

/// Handles a controller status update: resolves activity, writes it to the log,
/// and maps the controller status to a [`KeyState`] for external consumers.
pub(crate) struct StatusHandler {
    pub(crate) pubky: PublicKey,
    pub(crate) storage: Arc<AppStorage>,
    pub(crate) data_size: u64,
    pub(crate) last_sync: Option<u64>,
    pub(crate) next_sync: Option<u64>,
    pub(crate) sync_interval_secs: u64,
    pub(crate) previous_status: KeyStatus,
}

impl StatusHandler {
    /// Build a [`KeyState`] carrying forward the current data_size, last_sync and next_sync.
    fn base_state(&self, status: KeyStatus) -> KeyState {
        KeyState {
            status,
            data_size: self.data_size,
            last_sync: self.last_sync,
            next_sync: self.next_sync,
            ..Default::default()
        }
    }

    /// Process a controller status update: log activity and produce new [`KeyState`].
    pub async fn handle(&self, status: &ControllerStatus) -> KeyState {
        if let Some(entry) = self.resolve_activity(status).await {
            if let Err(e) = self.storage.write_activity(&self.pubky, &entry).await {
                warn!("Failed to write activity for {}: {}", self.pubky, e);
            }
        }

        match status {
            ControllerStatus::Starting { .. } => self.base_state(KeyStatus::Starting),
            ControllerStatus::Syncing {
                events_processed, ..
            } => {
                let data_size = if *events_processed > 0 {
                    self.storage.calculate_pubky_size(&self.pubky).await
                } else {
                    self.data_size
                };

                KeyState {
                    data_size,
                    ..self.base_state(KeyStatus::Syncing {
                        events_processed: *events_processed,
                    })
                }
            }
            ControllerStatus::Idle { .. } => {
                let data_size = self.storage.calculate_pubky_size(&self.pubky).await;
                let now = current_unix_timestamp();

                KeyState {
                    data_size,
                    last_sync: Some(now),
                    next_sync: Some(next_sync_time(self.sync_interval_secs)),
                    ..self.base_state(KeyStatus::Idle)
                }
            }
            ControllerStatus::Ended { .. } => self.base_state(KeyStatus::Stopped),
            ControllerStatus::Error { message, .. } => KeyState {
                error: Some(KeyError {
                    code: KeyErrorCode::Internal,
                    message: message.clone(),
                    recoverable: false,
                }),
                ..self.base_state(KeyStatus::Error)
            },
        }
    }

    /// Determine which activity entry (if any) to log for this status transition.
    async fn resolve_activity(&self, status: &ControllerStatus) -> Option<ActivityEntry> {
        let now = current_unix_timestamp();
        match status {
            ControllerStatus::Idle { .. } => {
                // A key is "new" (first-ever sync) when last_sync is None (no prior
                // Idle this session) and the activity log has no InitialBackup entry.
                // We scan the most recent entries for a prior InitialBackup.
                // 50 is generous — InitialBackup is always among the first entries.
                const INITIAL_BACKUP_SCAN_LIMIT: usize = 50;
                let is_initial = self.last_sync.is_none()
                    && !self
                        .storage
                        .read_activity(&self.pubky, INITIAL_BACKUP_SCAN_LIMIT)
                        .await
                        .iter()
                        .any(|e| e.activity_type == ActivityType::InitialBackup);

                if is_initial {
                    let count = match &self.previous_status {
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
                } else if let KeyStatus::Syncing { events_processed } = &self.previous_status {
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
            ControllerStatus::Error { message, .. } => Some(ActivityEntry {
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

/// Spawn a centralized status listener task that processes all controller status updates.
///
/// This single task replaces the per-key listener tasks. It receives status updates
/// from all controllers via the shared status channel, updates internal state,
/// and broadcasts KeyUpdate messages to external subscribers.
pub(crate) fn spawn_status_listener(
    inner: Arc<RwLock<super::manager::ManagerInner>>,
    sync_interval: Arc<RwLock<SyncInterval>>,
    storage: Arc<AppStorage>,
    update_tx: broadcast::Sender<KeyUpdate>,
    mut status_rx: broadcast::Receiver<ControllerStatus>,
) {
    tokio::spawn(async move {
        loop {
            match status_rx.recv().await {
                Ok(status) => {
                    let pubky = status.pubky().clone();

                    // Skip updates for keys that have been removed
                    let handler = {
                        let inner_read = inner.read();
                        let Some(snapshot) = inner_read.key_snapshot(&pubky) else {
                            debug!("Ignoring status update for removed key {}", pubky);
                            continue;
                        };
                        StatusHandler {
                            pubky: pubky.clone(),
                            storage: storage.clone(),
                            data_size: snapshot.data_size,
                            last_sync: snapshot.last_sync,
                            next_sync: snapshot.next_sync,
                            sync_interval_secs: sync_interval.read().get(),
                            previous_status: snapshot.status,
                        }
                    };

                    let new_state = handler.handle(&status).await;

                    {
                        let mut inner_write = inner.write();
                        inner_write.update_key_state(&pubky, new_state.clone());
                    }

                    let receivers = update_tx.send(KeyUpdate {
                        pubky: pubky.clone(),
                        state: new_state,
                    });
                    debug!(
                        "Broadcast status update for {} (receivers: {:?})",
                        pubky, receivers
                    );
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    warn!("Status listener lagged, skipped {} messages", n);
                    continue;
                }
                Err(broadcast::error::RecvError::Closed) => {
                    debug!("Status listener channel closed");
                    break;
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::AppStorage;
    use crate::sync::DEFAULT_SYNC_INTERVAL_SECONDS;
    use std::str::FromStr;
    use tempfile::TempDir;

    const TEST_PUBKY: &str = "z4e8s17cou9qmuia76iczxkwakp8xnck5qrfbzh8rbpiq1sugbuo";

    /// Create a fresh temp storage and pubky for a test.
    fn setup() -> (TempDir, Arc<AppStorage>, PublicKey) {
        let temp_dir = TempDir::new().unwrap();
        let storage = Arc::new(AppStorage::new_with_path(&temp_dir.path().to_path_buf()).unwrap());
        let pubky = PublicKey::from_str(TEST_PUBKY).unwrap();
        (temp_dir, storage, pubky)
    }

    /// Build a `StatusHandler` with common defaults, allowing callers to override fields.
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
            data_size,
            last_sync,
            next_sync: None,
            sync_interval_secs: DEFAULT_SYNC_INTERVAL_SECONDS,
            previous_status,
        }
    }

    #[tokio::test]
    async fn test_idle_after_initial_sync_writes_initial_backup_activity() {
        let (_dir, storage, pubky) = setup();

        // last_sync is None and no InitialBackup in activity log → initial backup
        handler(&pubky, &storage, 0, None, KeyStatus::Starting)
            .handle(&ControllerStatus::Idle {
                pubky: pubky.clone(),
            })
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
            .handle(&ControllerStatus::Idle {
                pubky: pubky.clone(),
            })
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
            .handle(&ControllerStatus::Idle {
                pubky: pubky.clone(),
            })
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
            .handle(&ControllerStatus::Idle {
                pubky: pubky.clone(),
            })
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
            .handle(&ControllerStatus::Idle {
                pubky: pubky.clone(),
            })
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
            .handle(&ControllerStatus::Idle {
                pubky: pubky.clone(),
            })
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
                pubky: pubky.clone(),
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

        h.handle(&ControllerStatus::Starting {
            pubky: pubky.clone(),
        })
        .await;

        h.handle(&ControllerStatus::Ended {
            pubky: pubky.clone(),
        })
        .await;

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
                pubky: pubky.clone(),
                message: "Test error message".to_string(),
            })
            .await;

        assert!(matches!(state.status, KeyStatus::Error));
        assert!(state.error.is_some());
        let error = state.error.unwrap();
        assert_eq!(error.code, KeyErrorCode::Internal);
        assert_eq!(error.message, "Test error message");
        assert!(!error.recoverable);
        assert_eq!(state.data_size, 1000);
    }

    #[tokio::test]
    async fn test_handle_starting_state_mapping() {
        let (_dir, storage, pubky) = setup();

        let state = handler(&pubky, &storage, 1234, None, KeyStatus::Starting)
            .handle(&ControllerStatus::Starting {
                pubky: pubky.clone(),
            })
            .await;

        assert!(matches!(state.status, KeyStatus::Starting));
        assert_eq!(state.data_size, 1234);
        assert!(state.error.is_none());
    }
}
