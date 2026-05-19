use std::sync::Arc;

use log::{error, info};
use tokio::sync::watch;

use super::error::OrchestratorError;
use crate::storage::AppStorage;
use crate::sync::{DEFAULT_SYNC_INTERVAL_SECONDS, MIN_SYNC_INTERVAL_SECONDS};

/// Manages the sync interval: current value, validation, and controller notification.
///
/// Disk persistence is handled by the caller (to avoid holding locks across async IO).
pub(crate) struct SyncInterval {
    current_secs: u64,
    watch_tx: watch::Sender<u64>,
}

impl SyncInterval {
    /// Load the sync interval from disk, falling back to config or default.
    pub fn load(storage: Arc<AppStorage>, config_secs: u64) -> Self {
        let current_secs = match storage.read_sync_interval() {
            Some(secs) if secs >= MIN_SYNC_INTERVAL_SECONDS => secs,
            Some(invalid) => {
                error!(
                    "Stored sync interval {}s is below minimum {}s, using default {}s",
                    invalid, MIN_SYNC_INTERVAL_SECONDS, DEFAULT_SYNC_INTERVAL_SECONDS
                );
                DEFAULT_SYNC_INTERVAL_SECONDS
            }
            None => config_secs,
        };

        let (watch_tx, _) = watch::channel(current_secs);

        SyncInterval {
            current_secs,
            watch_tx,
        }
    }

    /// Get the current sync interval in seconds.
    pub fn get(&self) -> u64 {
        self.current_secs
    }

    /// Validate a proposed interval value.
    pub fn validate(interval_secs: u64) -> Result<(), OrchestratorError> {
        if interval_secs < MIN_SYNC_INTERVAL_SECONDS {
            return Err(OrchestratorError::InvalidConfig(format!(
                "Sync interval must be at least {}s, got {}s",
                MIN_SYNC_INTERVAL_SECONDS, interval_secs
            )));
        }
        Ok(())
    }

    /// Apply a new interval value after it has been persisted.
    ///
    /// Call [`validate`](Self::validate) first, then persist to disk,
    /// then call this to update the in-memory state and notify controllers.
    pub fn apply(&mut self, interval_secs: u64) {
        self.current_secs = interval_secs;
        let _ = self.watch_tx.send(interval_secs);
        info!("Sync interval updated to {}s", interval_secs);
    }

    /// Subscribe to interval changes (for controllers).
    pub fn subscribe(&self) -> watch::Receiver<u64> {
        self.watch_tx.subscribe()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup() -> (TempDir, Arc<AppStorage>) {
        let temp_dir = TempDir::new().unwrap();
        let storage = Arc::new(AppStorage::new_with_path(temp_dir.path()).unwrap());
        (temp_dir, storage)
    }

    #[tokio::test]
    async fn test_load_uses_config_when_no_stored_value() {
        let (_dir, storage) = setup();
        let interval = SyncInterval::load(storage, 900);
        assert_eq!(interval.get(), 900);
    }

    #[tokio::test]
    async fn test_load_uses_stored_value_over_config() {
        let (_dir, storage) = setup();
        storage.write_sync_interval(600).await.unwrap();

        let interval = SyncInterval::load(storage, 900);
        assert_eq!(interval.get(), 600);
    }

    #[tokio::test]
    async fn test_load_rejects_stored_below_minimum() {
        let (_dir, storage) = setup();
        storage.write_sync_interval(5).await.unwrap();

        let interval = SyncInterval::load(storage, 900);
        assert_eq!(interval.get(), DEFAULT_SYNC_INTERVAL_SECONDS);
    }

    #[test]
    fn test_apply_updates_and_notifies() {
        let (_dir, storage) = setup();
        let mut interval = SyncInterval::load(storage, DEFAULT_SYNC_INTERVAL_SECONDS);
        let mut rx = interval.subscribe();

        SyncInterval::validate(600).unwrap();
        interval.apply(600);

        assert_eq!(interval.get(), 600);
        assert!(rx.has_changed().unwrap());
        assert_eq!(*rx.borrow_and_update(), 600);
    }

    #[test]
    fn test_validate_rejects_below_minimum() {
        assert!(SyncInterval::validate(0).is_err());
        assert!(SyncInterval::validate(9).is_err());
    }

    #[test]
    fn test_validate_accepts_exact_minimum() {
        SyncInterval::validate(MIN_SYNC_INTERVAL_SECONDS).unwrap();
    }
}
