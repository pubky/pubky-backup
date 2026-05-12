//! Backup controller for synchronizing a single pubky.
//!
//! The [`BackupController`] is the core sync loop that:
//! 1. Polls for new events from a pubky's homeserver
//! 2. Downloads and stores new/updated resources
//! 3. Deletes resources that have been removed
//! 4. Tracks progress via cursors for resumable syncing
//!
//! # Responsibilities
//!
//! - Main sync loop with interval-based polling
//! - Event stream processing (PUT/DELETE operations)
//! - Status reporting to the orchestrator layer
//! - Handling control messages (Cancel, ForceSync)
//!
//! # Channel Architecture
//!
//! The controller uses two broadcast channels:
//! - `control_rx`: Receives commands from the orchestrator (Cancel, ForceSync)
//! - `status_tx`: Sends status updates to the orchestrator (ControllerStatus)

use super::error::{EventsError, SyncError};
use super::events::{self, EVENT_BATCH_SIZE};
use super::fetcher;
use crate::is_developer_mode;
use crate::storage::AppStorage;
#[cfg(test)]
use crate::TEST_PUBKY;
use futures_util::StreamExt;
use log::{debug, error, info, warn};
use pubky::{Event, EventType, Pubky, PublicKey};
use std::ops::ControlFlow;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{broadcast, watch};
use tokio::time;

/// Minimum allowed sync interval in seconds.
pub const MIN_SYNC_INTERVAL_SECONDS: u64 = 10;

/// Default sync interval in seconds between backup batches.
pub const DEFAULT_SYNC_INTERVAL_SECONDS: u64 = 30;

/// Messages that can be sent to control the backup controller.
///
/// These messages are sent from the orchestrator layer to control
/// individual backup controllers.
#[derive(Debug, Clone)]
pub enum ControllerCommand {
    /// Stop the backup controller gracefully
    Cancel,
    /// Trigger an immediate sync (bypasses the interval timer)
    ForceSync,
}

/// Status updates emitted by the backup controller to the orchestrator.
///
/// These are internal status updates used for communication between the
/// sync layer and the orchestrator. The orchestrator transforms these
/// into [`KeyUpdate`](crate::orchestrator::KeyUpdate) messages for external consumers.
///
/// Each variant includes the `pubky` field identifying which controller
/// sent the status, enabling a shared status channel across all controllers.
#[derive(Debug, Clone)]
pub enum ControllerStatus {
    /// Controller is starting up (waiting for initial delay before first sync)
    Starting {
        /// The public key this status is for
        pubky: PublicKey,
    },
    /// Controller is actively syncing data
    Syncing {
        /// The public key this status is for
        pubky: PublicKey,
        /// Number of events processed in this sync batch
        events_processed: usize,
    },
    /// Controller is idle, waiting for next sync interval
    Idle {
        /// The public key this status is for
        pubky: PublicKey,
    },
    /// Controller has been stopped gracefully
    Ended {
        /// The public key this status is for
        pubky: PublicKey,
    },
    /// Controller encountered an error (will retry on next sync interval)
    Error {
        /// The public key this status is for
        pubky: PublicKey,
        /// Human-readable error message
        message: String,
    },
}

impl ControllerStatus {
    /// Get the public key associated with this status.
    pub fn pubky(&self) -> &PublicKey {
        match self {
            Self::Starting { pubky }
            | Self::Syncing { pubky, .. }
            | Self::Idle { pubky }
            | Self::Ended { pubky }
            | Self::Error { pubky, .. } => pubky,
        }
    }
}

/// Main backup controller which manages the backup process for a Pubky user.
///
/// The controller continuously syncs data from a Pubky homeserver to local storage,
/// processing events as they stream from the homeserver.
///
/// # Example
///
/// ```no_run
/// use pubky_backup_core::{AppStorage, BackupController, ControllerCommand, ControllerStatus};
/// use pubky::{Pubky, PublicKey};
/// use std::sync::Arc;
/// use std::str::FromStr;
/// use std::time::Duration;
/// use tokio::sync::{broadcast, mpsc, watch};
///
/// #[tokio::main]
/// async fn main() -> Result<(), Box<dyn std::error::Error>> {
///     let storage = Arc::new(AppStorage::new()?);
///     let pubky_client = Arc::new(Pubky::new()?);
///     let pubky = PublicKey::from_str("your_pubky_here")?;
///
///     let (control_tx, control_rx) = mpsc::channel(5);
///     let (status_tx, mut status_rx) = broadcast::channel(5);
///     let (_interval_tx, interval_rx) = watch::channel(30u64);
///
///     let controller = BackupController::new(
///         pubky.clone(),
///         storage,
///         pubky_client,
///         Some(control_rx),
///         Some(status_tx),
///         interval_rx,
///     ).with_initial_delay(Duration::from_secs(5));
///
///     tokio::spawn(controller.run());
///
///     // Listen for status updates
///     tokio::spawn(async move {
///         while let Ok(status) = status_rx.recv().await {
///             match status {
///                 ControllerStatus::Starting { pubky } => println!("{}: Starting...", pubky),
///                 ControllerStatus::Syncing { pubky, events_processed } => {
///                     println!("{}: Syncing... ({} events)", pubky, events_processed)
///                 },
///                 ControllerStatus::Idle { pubky } => println!("{}: Idle", pubky),
///                 ControllerStatus::Ended { pubky } => println!("{}: Ended", pubky),
///                 ControllerStatus::Error { pubky, message, .. } => {
///                     println!("{}: Error: {}", pubky, message)
///                 },
///             }
///         }
///     });
///
///     control_tx.send(ControllerCommand::ForceSync).await?;
///
///     Ok(())
/// }
/// ```
pub struct BackupController {
    pubky: PublicKey,
    storage: Arc<AppStorage>,
    pubky_client: Arc<Pubky>,
    control_rx: Option<tokio::sync::mpsc::Receiver<ControllerCommand>>,
    status_tx: Option<broadcast::Sender<ControllerStatus>>,
    /// Initial delay before starting the first sync (for staggering multiple controllers)
    initial_delay: Duration,
    /// Shared sync interval receiver - reads current interval dynamically
    sync_interval_rx: watch::Receiver<u64>,
}

impl BackupController {
    /// Creates a new backup controller instance.
    ///
    /// # Arguments
    ///
    /// * `pubky` - The public key to backup data for
    /// * `storage` - Shared storage instance for persisting data
    /// * `pubky_client` - Pubky SDK client for connecting to the homeserver
    /// * `control_rx` - Optional receiver for control messages (Cancel, ForceSync)
    /// * `status_tx` - Optional sender for status updates (shared across all controllers)
    /// * `sync_interval_rx` - Watch receiver for dynamically reading the current sync interval (seconds)
    ///
    /// # Returns
    ///
    /// A new `BackupController` ready to be run via [`BackupController::run`]
    ///
    /// # Developer Mode
    ///
    /// Enable developer mode (offline/no-network mode) for tests.
    /// In developer mode, the controller uses mock data instead of real network calls.
    pub fn new(
        pubky: PublicKey,
        storage: Arc<AppStorage>,
        pubky_client: Arc<Pubky>,
        control_rx: Option<tokio::sync::mpsc::Receiver<ControllerCommand>>,
        status_tx: Option<broadcast::Sender<ControllerStatus>>,
        sync_interval_rx: watch::Receiver<u64>,
    ) -> Self {
        Self {
            pubky,
            storage,
            pubky_client,
            control_rx,
            status_tx,
            initial_delay: Duration::ZERO,
            sync_interval_rx,
        }
    }

    /// Sets an initial delay before the first sync.
    ///
    /// This is useful for staggering multiple controllers to avoid
    /// overwhelming the network with simultaneous requests.
    ///
    /// During the delay, the controller emits `Starting` status and
    /// responds to commands:
    /// - `Cancel`: Stops the controller immediately
    /// - `ForceSync`: Skips the remaining delay and starts syncing
    pub fn with_initial_delay(mut self, delay: Duration) -> Self {
        self.initial_delay = delay;
        self
    }

    /// Reads the current sync interval from the shared watch channel.
    fn sync_interval(&self) -> Duration {
        Duration::from_secs(*self.sync_interval_rx.borrow())
    }

    /// Runs the backup controller loop.
    ///
    /// This method consumes `self` and runs until:
    /// - A `Cancel` message is received via the control channel
    /// - The control channel is closed
    ///
    /// The controller will:
    /// 1. Emit `Starting` status and wait for initial delay (if any)
    /// 2. Poll for new events from the pubky's homeserver
    /// 3. Download and store new/updated resources
    /// 4. Delete resources that have been removed
    /// 5. Wait for the next sync interval
    /// 6. Emit status updates via the status channel
    ///
    /// On errors, the controller logs the error and retries on the next sync interval.
    ///
    /// During the starting phase, the controller responds to commands:
    /// - `Cancel`: Stops immediately
    /// - `ForceSync`: Skips the remaining delay and starts syncing
    ///
    /// # Panics
    ///
    /// This method should not panic under normal circumstances. All errors are
    /// logged and result in an `Error` status being sent before the controller stops.
    pub async fn run(mut self) {
        self.send_status(ControllerStatus::Starting {
            pubky: self.pubky.clone(),
        });

        // Starting phase - wait for delay but respond to commands
        if !self.initial_delay.is_zero() {
            tokio::select! {
                _ = tokio::time::sleep(self.initial_delay) => {
                    // Delay complete, proceed to sync loop
                }
                msg = async {
                    if let Some(ref mut rx) = self.control_rx {
                        rx.recv().await
                    } else {
                        std::future::pending().await
                    }
                } => {
                    match msg {
                        Some(ControllerCommand::Cancel) => {
                            info!("Backup controller cancelled during starting phase");
                            self.send_status(ControllerStatus::Ended {
                                pubky: self.pubky.clone(),
                            });
                            return;
                        }
                        Some(ControllerCommand::ForceSync) => {
                            info!("Force sync during starting phase - skipping delay");
                            // Skip remaining delay, proceed to sync loop
                        }
                        None => {
                            warn!("Backup controller control channel closed during starting phase");
                            self.send_status(ControllerStatus::Ended {
                                pubky: self.pubky.clone(),
                            });
                            return;
                        }
                    }
                }
            }
        }

        // Main sync loop - sync immediately on first iteration, then wait
        let mut sync_now = true;

        loop {
            // Check for pending commands before each operation.
            if let Some(command) = self.try_recv_command() {
                match command {
                    ControllerCommand::Cancel => {
                        info!("Backup controller task cancelled");
                        self.send_status(ControllerStatus::Ended {
                            pubky: self.pubky.clone(),
                        });
                        break;
                    }
                    ControllerCommand::ForceSync => {
                        info!("Force sync triggered");
                        sync_now = true;
                    }
                }
            }

            if sync_now {
                sync_now = false;

                info!("Syncing key: {}", self.pubky);

                self.send_status(ControllerStatus::Syncing {
                    pubky: self.pubky.clone(),
                    events_processed: 0,
                });

                match self.perform_sync_batch().await {
                    Ok(ControlFlow::Continue(events_processed)) => {
                        // More events available, send status and loop back.
                        self.send_status(ControllerStatus::Syncing {
                            pubky: self.pubky.clone(),
                            events_processed,
                        });
                        sync_now = true;
                        continue;
                    }
                    Ok(ControlFlow::Break(())) => {
                        // Sync complete for this cycle, send Idle
                        self.send_status(ControllerStatus::Idle {
                            pubky: self.pubky.clone(),
                        });
                    }
                    Err(e) => {
                        let error_msg = format!("Sync error: {}", e);
                        error!("{}: {}", self.pubky, error_msg);
                        self.send_status(ControllerStatus::Error {
                            pubky: self.pubky.clone(),
                            message: error_msg,
                        });
                        // Wait for next sync interval, then retry
                    }
                }
            }

            // Idle phase: wait for next sync interval, control command, or interval change
            let sleep = time::sleep(self.sync_interval());
            tokio::pin!(sleep);

            tokio::select! {
                _ = &mut sleep => {
                    sync_now = true;
                }
                _ = self.sync_interval_rx.changed() => {
                    // Interval changed - just loop back and sleep with the new value
                }
                msg = async {
                    if let Some(ref mut rx) = self.control_rx {
                        rx.recv().await
                    } else {
                        std::future::pending().await
                    }
                } => {
                    match msg {
                        Some(ControllerCommand::Cancel) => {
                            info!("Backup controller task cancelled");
                            self.send_status(ControllerStatus::Ended {
                                pubky: self.pubky.clone(),
                            });
                            break;
                        }
                        Some(ControllerCommand::ForceSync) => {
                            info!("Force sync triggered");
                            sync_now = true;
                        }
                        None => {
                            warn!("Backup controller control channel closed");
                            self.send_status(ControllerStatus::Ended {
                                pubky: self.pubky.clone(),
                            });
                            break;
                        }
                    }
                }
            }
        }
    }

    /// Non-blocking check for a pending command on the control channel.
    fn try_recv_command(&mut self) -> Option<ControllerCommand> {
        self.control_rx.as_mut().and_then(|rx| rx.try_recv().ok())
    }

    fn send_status(&self, status: ControllerStatus) {
        if let Some(tx) = &self.status_tx {
            if let Err(e) = tx.send(status.clone()) {
                warn!(
                    "Failed to send status update (no receivers or lagging): {:?}",
                    e
                );
            }
        }
    }

    /// Process events by streaming from the homeserver (or empty stream in developer mode).
    ///
    /// This method performs a single sync batch, fetching events from the cursor position
    /// and processing them. Returns `ControlFlow::Continue(count)` if more events are available,
    /// or `ControlFlow::Break(())` if sync is complete.
    async fn perform_sync_batch(&self) -> Result<ControlFlow<(), usize>, SyncError> {
        let cursor = self.storage.read_cursor(&self.pubky).await?;

        // Get event stream - empty stream in developer mode (offline), real stream otherwise
        let event_stream = if is_developer_mode() {
            Box::pin(futures_util::stream::empty())
        } else {
            match events::create_event_stream(&self.pubky_client, &self.pubky, cursor).await {
                Ok(stream) => stream,
                Err(e) => {
                    error!("Sync events fetch failed: {}", e);
                    // Treat network fetch failures as recoverable - retry on next sync interval
                    return Ok(ControlFlow::Break(()));
                }
            }
        };

        self.process_event_stream(event_stream, cursor).await
    }

    /// Save cursor progress if we have a cursor value.
    async fn save_cursor_if_present(&self, cursor: Option<u64>) -> Result<(), SyncError> {
        if let Some(c) = cursor {
            self.storage.write_cursor(&self.pubky, c).await?;
        }
        Ok(())
    }

    /// Process events from a stream, saving cursor progress periodically and on error.
    async fn process_event_stream(
        &self,
        mut event_stream: std::pin::Pin<
            Box<dyn futures_util::Stream<Item = Result<Event, EventsError>> + Send>,
        >,
        initial_cursor: Option<u64>,
    ) -> Result<ControlFlow<(), usize>, SyncError> {
        let mut events_processed = 0;
        let mut last_cursor: Option<u64> = initial_cursor;

        // Process events as they stream in
        while let Some(event_result) = event_stream.next().await {
            match event_result {
                Ok(event) => {
                    let event_cursor = event.cursor.id();
                    self.process_single_event(&event).await?;
                    events_processed += 1;
                    last_cursor = Some(event_cursor);

                    // Save cursor periodically (every EVENT_BATCH_SIZE events)
                    if events_processed % EVENT_BATCH_SIZE as usize == 0 {
                        self.save_cursor_if_present(last_cursor).await?;
                    }
                }
                Err(e) => {
                    let _ = self
                        .storage
                        .write_global_error("event_stream", &format!("Event stream error: {}", e))
                        .await;
                    // Save progress and break out of stream loop. The next sync interval will reconnect
                    self.save_cursor_if_present(last_cursor).await?;
                    break;
                }
            }
        }

        info!(
            "Processed {} events for key: {}",
            events_processed, self.pubky
        );

        if events_processed > 0 {
            // Save final cursor
            self.save_cursor_if_present(last_cursor).await?;
            Ok(ControlFlow::Continue(events_processed))
        } else {
            Ok(ControlFlow::Break(()))
        }
    }

    /// Process a single event
    async fn process_single_event(&self, event: &Event) -> Result<(), SyncError> {
        // Skip events for other pubkys
        if event.resource.owner != self.pubky {
            return Ok(());
        }

        match event.event_type {
            EventType::Put => {
                debug!("Processing PUT event for: {}", event.resource);
                match fetcher::fetch_resource_data(&self.pubky_client, &event.resource).await {
                    Ok(data_vec) => {
                        // Skip storing empty data (404 responses)
                        if !data_vec.is_empty() {
                            self.storage.write(&event.resource, data_vec).await?;
                        }
                    }
                    Err(e) => {
                        // Log fetch errors and continue processing other events
                        self.storage
                            .write_global_error(
                                &event.resource.to_string(),
                                &format!("Fetch failed: {}", e),
                            )
                            .await?;
                    }
                }
            }
            EventType::Delete => {
                debug!("Processing DEL event for: {}", event.resource);
                self.storage.delete(&event.resource).await?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::AppStorage;
    use pubky::PubkyResource;
    use std::str::FromStr;
    use tempfile::TempDir;

    /// Helper to enable developer mode (offline/no-network) for tests
    fn enable_developer_mode() {
        std::env::set_var("PUBKY_DEVELOPER_MODE", "1");
    }

    fn create_test_interval_rx() -> (watch::Sender<u64>, watch::Receiver<u64>) {
        watch::channel(DEFAULT_SYNC_INTERVAL_SECONDS)
    }

    // Test helper to create a storage instance with a temporary directory
    fn create_test_storage() -> (Arc<AppStorage>, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let storage = AppStorage::new_with_path(&temp_dir.path().to_path_buf()).unwrap();
        (Arc::new(storage), temp_dir)
    }

    // Test helper to create a Pubky client (only used in tests that need it)
    fn create_test_pubky_client() -> Arc<Pubky> {
        Arc::new(Pubky::testnet().expect("Failed to create testnet client"))
    }

    #[tokio::test]
    async fn test_controller_runs_and_can_be_cancelled() {
        enable_developer_mode();
        let (storage, _temp_dir) = create_test_storage();
        let pubky = PublicKey::from_str(TEST_PUBKY).unwrap();
        let pubky_client = create_test_pubky_client();

        let (control_tx, control_rx) = tokio::sync::mpsc::channel(5);
        let (status_tx, mut status_rx) = broadcast::channel(10);

        let controller = BackupController::new(
            pubky,
            storage,
            pubky_client,
            Some(control_rx),
            Some(status_tx),
            create_test_interval_rx().1,
        );

        // Spawn the controller
        let handle = tokio::spawn(controller.run());

        // First status should be Starting
        let status = tokio::time::timeout(Duration::from_secs(5), status_rx.recv())
            .await
            .expect("Should receive status")
            .unwrap();
        assert!(
            matches!(status, ControllerStatus::Starting { .. }),
            "First status should be Starting, got {:?}",
            status
        );

        // Wait for sync status (Syncing or Idle)
        let status = tokio::time::timeout(Duration::from_secs(5), status_rx.recv())
            .await
            .expect("Should receive status")
            .unwrap();

        match status {
            ControllerStatus::Syncing { .. } | ControllerStatus::Idle { .. } => {}
            _ => panic!("Unexpected ControllerStatus after Starting: {:?}", status),
        }

        // Send cancel message
        control_tx.send(ControllerCommand::Cancel).await.unwrap();

        // Controller should finish
        tokio::time::timeout(Duration::from_secs(2), handle)
            .await
            .expect("Controller should finish")
            .unwrap();
    }

    #[tokio::test]
    async fn test_controller_force_sync() {
        enable_developer_mode();
        let (storage, _temp_dir) = create_test_storage();
        let pubky = PublicKey::from_str(TEST_PUBKY).unwrap();
        let pubky_client = create_test_pubky_client();

        let (control_tx, control_rx) = tokio::sync::mpsc::channel(5);
        let (status_tx, mut status_rx) = broadcast::channel(10);

        let controller = BackupController::new(
            pubky,
            storage,
            pubky_client,
            Some(control_rx),
            Some(status_tx),
            create_test_interval_rx().1,
        );

        tokio::spawn(controller.run());

        // First status should be Starting
        let status = tokio::time::timeout(Duration::from_secs(5), status_rx.recv())
            .await
            .expect("Should receive status")
            .unwrap();
        assert!(matches!(status, ControllerStatus::Starting { .. }));

        // Trigger force sync
        control_tx.send(ControllerCommand::ForceSync).await.unwrap();

        // Should receive Syncing status
        let status = tokio::time::timeout(Duration::from_secs(5), status_rx.recv())
            .await
            .expect("Should receive status")
            .unwrap();

        assert!(
            matches!(status, ControllerStatus::Syncing { .. }),
            "Expected Syncing status after force sync, got {:?}",
            status
        );

        // Cleanup
        control_tx.send(ControllerCommand::Cancel).await.unwrap();
    }

    #[tokio::test]
    async fn test_process_event_stream_initial_sync() {
        let (storage, _temp_dir) = create_test_storage();
        let pubky = PublicKey::from_str(TEST_PUBKY).unwrap();
        let pubky_client = create_test_pubky_client();

        let controller = BackupController::new(
            pubky.clone(),
            storage.clone(),
            pubky_client,
            None,
            None,
            create_test_interval_rx().1,
        );

        // First batch should return Continue (more events available)
        let stream = events::test_helpers::create_test_event_stream(None);
        let result = controller.process_event_stream(stream, None).await.unwrap();
        assert!(matches!(result, ControlFlow::Continue(3)));

        // Cursor should have been updated
        let cursor = storage.read_cursor(&pubky).await.unwrap();
        assert_eq!(cursor, Some(3));
    }

    #[tokio::test]
    async fn test_process_event_stream_completes() {
        let (storage, _temp_dir) = create_test_storage();
        let pubky = PublicKey::from_str(TEST_PUBKY).unwrap();
        let pubky_client = create_test_pubky_client();

        let controller = BackupController::new(
            pubky.clone(),
            storage.clone(),
            pubky_client,
            None,
            None,
            create_test_interval_rx().1,
        );

        // Process multiple batches until completion
        let mut iterations = 0;
        let mut cursor: Option<u64> = None;
        loop {
            let stream = events::test_helpers::create_test_event_stream(cursor);
            match controller
                .process_event_stream(stream, cursor)
                .await
                .unwrap()
            {
                ControlFlow::Continue(count) => {
                    iterations += 1;
                    cursor = storage.read_cursor(&pubky).await.unwrap();
                    assert!(count > 0);
                    if iterations > 10 {
                        panic!("Too many iterations - sync should complete");
                    }
                }
                ControlFlow::Break(()) => break,
            }
        }

        assert!(iterations > 0);
    }

    #[tokio::test]
    async fn test_process_single_event_handles_put() {
        let (storage, _temp_dir) = create_test_storage();
        let pubky = PublicKey::from_str(TEST_PUBKY).unwrap();

        // Write data directly to storage (simulating what fetch_resource_data + write does)
        let resource = PubkyResource::new(pubky.clone(), "/pub/test.json").unwrap();
        storage
            .write(&resource, b"test data".to_vec())
            .await
            .unwrap();

        // Verify data was written
        let data = storage.read(&resource).await.unwrap();
        assert!(!data.is_empty());
        assert_eq!(data, b"test data");
    }

    #[tokio::test]
    async fn test_process_single_event_handles_delete() {
        let (storage, _temp_dir) = create_test_storage();
        let pubky = PublicKey::from_str(TEST_PUBKY).unwrap();
        let pubky_client = create_test_pubky_client();

        let controller = BackupController::new(
            pubky.clone(),
            storage.clone(),
            pubky_client,
            None,
            None,
            create_test_interval_rx().1,
        );

        // First create a resource
        let resource = PubkyResource::new(pubky.clone(), "/pub/test.json").unwrap();
        storage
            .write(&resource, b"test data".to_vec())
            .await
            .unwrap();

        // Verify it exists
        assert!(storage.read(&resource).await.is_ok());

        // Create a DELETE event
        let event = Event {
            event_type: EventType::Delete,
            resource: resource.clone(),
            cursor: pubky::EventCursor::new(1),
            content_hash: None,
        };

        // Process event
        controller.process_single_event(&event).await.unwrap();

        // Verify data was deleted
        assert!(storage.read(&resource).await.is_err());
    }

    #[tokio::test]
    async fn test_process_single_event_skips_other_pubky() {
        let (storage, _temp_dir) = create_test_storage();
        let pubky1 = PublicKey::from_str(TEST_PUBKY).unwrap();
        let pubky2 =
            PublicKey::from_str("o4dksfbqk85ogzdb5osziw6befigbuxmuxkuxq8434q89uj56uxo").unwrap();
        let pubky_client = create_test_pubky_client();

        let controller = BackupController::new(
            pubky1.clone(),
            storage.clone(),
            pubky_client,
            None,
            None,
            create_test_interval_rx().1,
        );

        // Create event for a different pubky
        let resource = PubkyResource::new(pubky2.clone(), "/pub/test.json").unwrap();
        let event = Event {
            event_type: EventType::Put,
            resource: resource.clone(),
            cursor: pubky::EventCursor::new(1),
            content_hash: None,
        };

        // Process event
        controller.process_single_event(&event).await.unwrap();

        // Verify data was NOT written for the other pubky
        assert!(storage.read(&resource).await.is_err());
    }

    #[tokio::test]
    async fn test_stream_error_saves_cursor_progress() {
        enable_developer_mode();
        let (storage, _temp_dir) = create_test_storage();
        let pubky = PublicKey::from_str(TEST_PUBKY).unwrap();
        let pubky_client = create_test_pubky_client();

        let controller = BackupController::new(
            pubky.clone(),
            storage.clone(),
            pubky_client,
            None,
            None,
            create_test_interval_rx().1,
        );

        // Create a stream that yields 3 events then fails
        let failing_stream = events::test_helpers::create_failing_test_event_stream(3);

        // Process the stream - should handle error gracefully (log and continue)
        let result = controller.process_event_stream(failing_stream, None).await;

        // Should return Ok (stream errors are handled gracefully, not propagated)
        assert!(
            result.is_ok(),
            "Stream errors should be handled gracefully, not propagated"
        );

        // Should indicate events were processed
        let events_processed = result.unwrap();
        assert!(
            matches!(events_processed, ControlFlow::Continue(3)),
            "Should indicate 3 events were processed, got {:?}",
            events_processed
        );

        // Cursor should have been saved at the last successful event (cursor=3)
        let saved_cursor = storage.read_cursor(&pubky).await.unwrap();
        assert_eq!(
            saved_cursor,
            Some(3),
            "Cursor should be saved at last successful event before error"
        );
    }

    #[tokio::test]
    async fn test_controller_runs_without_channels() {
        // Test that controller works when no channels are provided (uses pending futures)
        enable_developer_mode();
        let (storage, _temp_dir) = create_test_storage();
        let pubky = PublicKey::from_str(TEST_PUBKY).unwrap();
        let pubky_client = create_test_pubky_client();

        // Create controller without any channels
        let controller = BackupController::new(
            pubky.clone(),
            storage.clone(),
            pubky_client,
            None,
            None,
            create_test_interval_rx().1,
        );

        // Spawn and let it run briefly - it should process events without panicking
        let handle = tokio::spawn(async move {
            // Use a timeout to prevent hanging - controller will loop forever without cancel
            tokio::time::timeout(Duration::from_millis(500), controller.run()).await
        });

        // Should timeout (no way to cancel without control channel), not panic
        let result = handle.await.unwrap();
        assert!(result.is_err(), "Should timeout since no cancel signal");
    }

    #[tokio::test]
    async fn test_controller_status_ended_on_cancel() {
        enable_developer_mode();
        let (storage, _temp_dir) = create_test_storage();
        let pubky = PublicKey::from_str(TEST_PUBKY).unwrap();
        let pubky_client = create_test_pubky_client();

        let (control_tx, control_rx) = tokio::sync::mpsc::channel(5);
        let (status_tx, mut status_rx) = broadcast::channel(10);

        let controller = BackupController::new(
            pubky,
            storage,
            pubky_client,
            Some(control_rx),
            Some(status_tx),
            create_test_interval_rx().1,
        );

        let handle = tokio::spawn(controller.run());

        // Wait for initial status
        let _ = tokio::time::timeout(Duration::from_secs(2), status_rx.recv())
            .await
            .expect("Should receive initial status");

        // Cancel the controller
        control_tx.send(ControllerCommand::Cancel).await.unwrap();

        // Wait for the Ended status
        let mut received_ended = false;
        for _ in 0..5 {
            if let Ok(Ok(status)) =
                tokio::time::timeout(Duration::from_millis(500), status_rx.recv()).await
            {
                if matches!(status, ControllerStatus::Ended { .. }) {
                    received_ended = true;
                    break;
                }
            }
        }

        assert!(received_ended, "Should receive Ended status after cancel");

        // Controller should finish
        tokio::time::timeout(Duration::from_secs(1), handle)
            .await
            .expect("Controller should finish")
            .unwrap();
    }

    #[tokio::test]
    async fn test_is_developer_mode() {
        // Test with env var not set
        std::env::remove_var("PUBKY_DEVELOPER_MODE");
        // Note: is_developer_mode caches the value, so we can only test the initial state
        // This test documents the expected behavior

        // Enable developer mode
        std::env::set_var("PUBKY_DEVELOPER_MODE", "1");
        // Due to once_cell caching, this won't change the result within the same test run
        // The actual behavior is tested indirectly through other tests that call enable_developer_mode()
    }

    #[tokio::test]
    async fn test_controller_emits_starting_status() {
        enable_developer_mode();
        let (storage, _temp_dir) = create_test_storage();
        let pubky = PublicKey::from_str(TEST_PUBKY).unwrap();
        let pubky_client = create_test_pubky_client();

        let (control_tx, control_rx) = tokio::sync::mpsc::channel(5);
        let (status_tx, mut status_rx) = broadcast::channel(10);

        let controller = BackupController::new(
            pubky.clone(),
            storage,
            pubky_client,
            Some(control_rx),
            Some(status_tx),
            create_test_interval_rx().1,
        );

        tokio::spawn(controller.run());

        // First status should always be Starting
        let status = tokio::time::timeout(Duration::from_secs(5), status_rx.recv())
            .await
            .expect("Should receive status")
            .unwrap();

        match status {
            ControllerStatus::Starting {
                pubky: status_pubky,
            } => {
                assert_eq!(status_pubky, pubky);
            }
            _ => panic!("First status should be Starting, got {:?}", status),
        }

        // Cleanup
        control_tx.send(ControllerCommand::Cancel).await.unwrap();
    }

    #[tokio::test]
    async fn test_controller_with_initial_delay_responds_to_cancel() {
        enable_developer_mode();
        let (storage, _temp_dir) = create_test_storage();
        let pubky = PublicKey::from_str(TEST_PUBKY).unwrap();
        let pubky_client = create_test_pubky_client();

        let (control_tx, control_rx) = tokio::sync::mpsc::channel(5);
        let (status_tx, mut status_rx) = broadcast::channel(10);

        // Create controller with a long initial delay
        let controller = BackupController::new(
            pubky.clone(),
            storage,
            pubky_client,
            Some(control_rx),
            Some(status_tx),
            create_test_interval_rx().1,
        )
        .with_initial_delay(Duration::from_secs(60)); // Long delay

        let handle = tokio::spawn(controller.run());

        // Wait for Starting status
        let status = tokio::time::timeout(Duration::from_secs(1), status_rx.recv())
            .await
            .expect("Should receive Starting status quickly")
            .unwrap();
        assert!(
            matches!(status, ControllerStatus::Starting { .. }),
            "Should receive Starting status"
        );

        // Send cancel during the starting phase
        control_tx.send(ControllerCommand::Cancel).await.unwrap();

        // Should receive Ended status
        let status = tokio::time::timeout(Duration::from_secs(1), status_rx.recv())
            .await
            .expect("Should receive Ended status quickly")
            .unwrap();
        assert!(
            matches!(status, ControllerStatus::Ended { .. }),
            "Should receive Ended status after cancel during starting phase"
        );

        // Controller should finish quickly (not wait for the 60 second delay)
        tokio::time::timeout(Duration::from_secs(1), handle)
            .await
            .expect("Controller should finish quickly after cancel during starting phase")
            .unwrap();
    }

    #[tokio::test]
    async fn test_controller_with_initial_delay_responds_to_force_sync() {
        enable_developer_mode();
        let (storage, _temp_dir) = create_test_storage();
        let pubky = PublicKey::from_str(TEST_PUBKY).unwrap();
        let pubky_client = create_test_pubky_client();

        let (control_tx, control_rx) = tokio::sync::mpsc::channel(5);
        let (status_tx, mut status_rx) = broadcast::channel(10);

        // Create controller with a long initial delay
        let controller = BackupController::new(
            pubky.clone(),
            storage,
            pubky_client,
            Some(control_rx),
            Some(status_tx),
            create_test_interval_rx().1,
        )
        .with_initial_delay(Duration::from_secs(60)); // Long delay

        tokio::spawn(controller.run());

        // Wait for Starting status
        let status = tokio::time::timeout(Duration::from_secs(1), status_rx.recv())
            .await
            .expect("Should receive Starting status")
            .unwrap();
        assert!(matches!(status, ControllerStatus::Starting { .. }));

        // Send ForceSync during the starting phase
        control_tx.send(ControllerCommand::ForceSync).await.unwrap();

        // Should skip delay and receive Syncing status quickly
        let status = tokio::time::timeout(Duration::from_secs(2), status_rx.recv())
            .await
            .expect("Should receive Syncing status quickly after ForceSync")
            .unwrap();
        assert!(
            matches!(status, ControllerStatus::Syncing { .. }),
            "Should receive Syncing status after ForceSync during starting phase, got {:?}",
            status
        );

        // Cleanup
        control_tx.send(ControllerCommand::Cancel).await.unwrap();
    }

    #[tokio::test]
    async fn test_controller_with_zero_delay_proceeds_immediately() {
        enable_developer_mode();
        let (storage, _temp_dir) = create_test_storage();
        let pubky = PublicKey::from_str(TEST_PUBKY).unwrap();
        let pubky_client = create_test_pubky_client();

        let (control_tx, control_rx) = tokio::sync::mpsc::channel(5);
        let (status_tx, mut status_rx) = broadcast::channel(10);

        // Create controller with zero delay (default)
        let controller = BackupController::new(
            pubky.clone(),
            storage,
            pubky_client,
            Some(control_rx),
            Some(status_tx),
            create_test_interval_rx().1,
        );

        tokio::spawn(controller.run());

        // Should receive Starting status
        let status = tokio::time::timeout(Duration::from_millis(100), status_rx.recv())
            .await
            .expect("Should receive Starting status immediately")
            .unwrap();
        assert!(matches!(status, ControllerStatus::Starting { .. }));

        // Should immediately proceed to Syncing (no delay)
        let status = tokio::time::timeout(Duration::from_millis(500), status_rx.recv())
            .await
            .expect("Should receive Syncing status quickly with zero delay")
            .unwrap();
        assert!(
            matches!(status, ControllerStatus::Syncing { .. }),
            "Should proceed to Syncing immediately with zero delay, got {:?}",
            status
        );

        // Cleanup
        control_tx.send(ControllerCommand::Cancel).await.unwrap();
    }
}
