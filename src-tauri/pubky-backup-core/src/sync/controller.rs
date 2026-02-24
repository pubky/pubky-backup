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
use crate::DEV_MODE_PUBKY;
use futures_util::StreamExt;
use log::{debug, error, info, warn};
use pubky::{Event, EventType, Pubky, PublicKey};
use std::ops::ControlFlow;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;
use tokio::time;

/// Default sync interval in seconds between backup batches.
pub const SYNC_INTERVAL_SECONDS: u64 = 30;

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
    /// Controller encountered a critical error and stopped
    Error {
        /// The public key this status is for
        pubky: PublicKey,
        /// Human-readable error message
        message: String,
    },
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
/// use tokio::sync::{broadcast, mpsc};
///
/// #[tokio::main]
/// async fn main() -> Result<(), Box<dyn std::error::Error>> {
///     // Initialize storage
///     let storage = Arc::new(AppStorage::new()?);
///
///     // Initialize Pubky client
///     let pubky_client = Arc::new(Pubky::new()?);
///
///     // Parse the pubky to backup
///     let pubky = PublicKey::from_str("your_pubky_here")?;
///
///     // Create channels for control (mpsc) and status (broadcast)
///     let (control_tx, control_rx) = mpsc::channel(5);
///     let (status_tx, mut status_rx) = broadcast::channel(5);
///
///     // Create and spawn the backup controller
///     let controller = BackupController::new(
///         pubky.clone(),
///         storage,
///         pubky_client,
///         Some(control_rx),
///         Some(status_tx),
///     );
///
///     // Spawn the controller in a background task
///     tokio::spawn(controller.run());
///
///     // Listen for status updates
///     tokio::spawn(async move {
///         while let Ok(status) = status_rx.recv().await {
///             match status {
///                 ControllerStatus::Syncing { pubky, events_processed } => {
///                     println!("{}: Syncing... ({} events)", pubky, events_processed)
///                 },
///                 ControllerStatus::Idle { pubky } => println!("{}: Idle", pubky),
///                 ControllerStatus::Ended { pubky } => println!("{}: Ended", pubky),
///                 ControllerStatus::Error { pubky, message } => {
///                     println!("{}: Error: {}", pubky, message)
///                 },
///             }
///         }
///     });
///
///     // Send control messages as needed
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
    /// Initial delay before first sync (for staggering multiple controllers)
    initial_delay: Duration,
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
    ///
    /// # Returns
    ///
    /// A new `BackupController` ready to be run via [`BackupController::run`]
    ///
    /// # Developer Mode
    ///
    /// Enable developer mode by setting the `PUBKY_DEVELOPER_MODE` environment variable.
    /// In developer mode, the controller uses mock data instead of real network calls.
    pub fn new(
        pubky: PublicKey,
        storage: Arc<AppStorage>,
        pubky_client: Arc<Pubky>,
        control_rx: Option<tokio::sync::mpsc::Receiver<ControllerCommand>>,
        status_tx: Option<broadcast::Sender<ControllerStatus>>,
    ) -> Self {
        Self::with_initial_delay(
            pubky,
            storage,
            pubky_client,
            control_rx,
            status_tx,
            Duration::ZERO,
        )
    }

    /// Creates a new backup controller with an initial delay before the first sync.
    ///
    /// The initial delay is used to stagger multiple controllers started at the same time,
    /// preventing them from all syncing simultaneously.
    pub fn with_initial_delay(
        pubky: PublicKey,
        storage: Arc<AppStorage>,
        pubky_client: Arc<Pubky>,
        control_rx: Option<tokio::sync::mpsc::Receiver<ControllerCommand>>,
        status_tx: Option<broadcast::Sender<ControllerStatus>>,
        initial_delay: Duration,
    ) -> Self {
        Self {
            pubky,
            storage,
            pubky_client,
            control_rx,
            status_tx,
            initial_delay,
        }
    }

    /// Runs the backup controller loop.
    ///
    /// This method consumes `self` and runs until:
    /// - A `Cancel` message is received via the control channel
    /// - A critical error occurs during syncing
    /// - The control channel is closed
    ///
    /// The controller will:
    /// 1. Poll for new events from the pubky's homeserver
    /// 2. Download and store new/updated resources
    /// 3. Delete resources that have been removed
    /// 4. Wait for the next sync interval (30 seconds)
    /// 5. Emit status updates via the status channel
    ///
    /// # Panics
    ///
    /// This method should not panic under normal circumstances. All errors are
    /// logged and result in an `Error` status being sent before the controller stops.
    pub async fn run(mut self) {
        // Apply initial delay to stagger sync starts across multiple controllers
        let start_time = time::Instant::now() + self.initial_delay;
        let mut interval =
            time::interval_at(start_time, Duration::from_secs(SYNC_INTERVAL_SECONDS));

        loop {
            tokio::select! {
                _ = interval.tick() => {

                    self.send_status(ControllerStatus::Syncing {
                        pubky: self.pubky.clone(),
                        events_processed: 0,
                    });

                    match self.perform_sync_batch().await {
                        Ok(ControlFlow::Continue(events_processed)) => {
                            // More events available, send status and keep syncing immediately
                            self.send_status(ControllerStatus::Syncing {
                                pubky: self.pubky.clone(),
                                events_processed,
                            });
                            interval = time::interval_at(
                                time::Instant::now(),
                                Duration::from_secs(SYNC_INTERVAL_SECONDS)
                            );
                        }
                        Ok(ControlFlow::Break(())) => {
                            // Sync complete
                        }
                        Err(e) => {
                            let error_msg = format!("Critical sync batch failure: {}", e);
                            let _ = self.storage.write_error(&self.pubky, "sync", &error_msg).await;
                            self.send_status(ControllerStatus::Error {
                                pubky: self.pubky.clone(),
                                message: error_msg,
                            });
                            return;
                        }
                    }

                    self.send_status(ControllerStatus::Idle {
                        pubky: self.pubky.clone(),
                    });
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
                            // Break out of loop ending task
                            break;
                        }
                        Some(ControllerCommand::ForceSync) => {
                            info!("Force sync triggered");
                            // Reset interval to trigger immediately
                            interval = time::interval_at(
                                time::Instant::now(),
                                Duration::from_secs(SYNC_INTERVAL_SECONDS)
                            );
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

    /// Process events by streaming from the homeserver (or mock stream in developer mode).
    ///
    /// This method performs a single sync batch, fetching events from the cursor position
    /// and processing them. Returns `ControlFlow::Continue(count)` if more events are available,
    /// or `ControlFlow::Break(())` if sync is complete.
    async fn perform_sync_batch(&self) -> Result<ControlFlow<(), usize>, SyncError> {
        let cursor = self.storage.read_cursor(&self.pubky).await?;

        // Get event stream - mock stream in developer mode, real stream otherwise
        let event_stream = if is_developer_mode() {
            events::create_mock_event_stream(cursor)
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
                        .write_error(
                            &self.pubky,
                            "event_stream",
                            &format!("Event stream error: {}", e),
                        )
                        .await;
                    // Save progress and break out of stream loop. The next sync interval will reconnect
                    self.save_cursor_if_present(last_cursor).await?;
                    break;
                }
            }
        }

        info!("Processed {} events", events_processed);

        // Save final cursor
        if events_processed > 0 {
            self.save_cursor_if_present(last_cursor).await?;
        }

        if events_processed > 0 {
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
                            .write_error(
                                &self.pubky,
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

    /// Helper to enable developer mode for tests that need mock data
    fn enable_developer_mode() {
        std::env::set_var("PUBKY_DEVELOPER_MODE", "1");
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
        let pubky = PublicKey::from_str(DEV_MODE_PUBKY).unwrap();
        let pubky_client = create_test_pubky_client();

        let (control_tx, control_rx) = tokio::sync::mpsc::channel(5);
        let (status_tx, mut status_rx) = broadcast::channel(10);

        let controller = BackupController::new(
            pubky,
            storage,
            pubky_client,
            Some(control_rx),
            Some(status_tx),
        );

        // Spawn the controller
        let handle = tokio::spawn(controller.run());

        // Wait for first status update (should be Syncing or Idle)
        let status = tokio::time::timeout(Duration::from_secs(5), status_rx.recv())
            .await
            .expect("Should receive status")
            .unwrap();

        match status {
            ControllerStatus::Syncing { .. } | ControllerStatus::Idle { .. } => {}
            _ => panic!("Unexpected ControllerStatus"),
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
        let pubky = PublicKey::from_str(DEV_MODE_PUBKY).unwrap();
        let pubky_client = create_test_pubky_client();

        let (control_tx, control_rx) = tokio::sync::mpsc::channel(5);
        let (status_tx, mut status_rx) = broadcast::channel(10);

        let controller = BackupController::new(
            pubky,
            storage,
            pubky_client,
            Some(control_rx),
            Some(status_tx),
        );

        tokio::spawn(controller.run());

        // Trigger force sync
        control_tx.send(ControllerCommand::ForceSync).await.unwrap();

        // Should receive Syncing status
        let status = tokio::time::timeout(Duration::from_secs(5), status_rx.recv())
            .await
            .expect("Should receive status")
            .unwrap();

        assert!(matches!(status, ControllerStatus::Syncing { .. }));

        // Cleanup
        control_tx.send(ControllerCommand::Cancel).await.unwrap();
    }

    #[tokio::test]
    async fn test_perform_sync_batch_initial_sync() {
        enable_developer_mode();
        let (storage, _temp_dir) = create_test_storage();
        let pubky = PublicKey::from_str(DEV_MODE_PUBKY).unwrap();
        let pubky_client = create_test_pubky_client();

        let controller =
            BackupController::new(pubky.clone(), storage.clone(), pubky_client, None, None);

        // First sync should return Continue (more events available)
        let result = controller.perform_sync_batch().await.unwrap();
        assert!(matches!(result, ControlFlow::Continue(_)));

        // Cursor should have been updated
        let cursor = storage.read_cursor(&pubky).await.unwrap();
        assert!(cursor.is_some());
        assert_eq!(cursor, Some(3)); // First batch ends at cursor 3
    }

    #[tokio::test]
    async fn test_perform_sync_batch_completes() {
        enable_developer_mode();
        let (storage, _temp_dir) = create_test_storage();
        let pubky = PublicKey::from_str(DEV_MODE_PUBKY).unwrap();
        let pubky_client = create_test_pubky_client();

        let controller =
            BackupController::new(pubky.clone(), storage.clone(), pubky_client, None, None);

        // Perform multiple syncs until completion
        let mut iterations = 0;
        loop {
            match controller.perform_sync_batch().await.unwrap() {
                ControlFlow::Continue(_) => {
                    iterations += 1;
                    if iterations > 10 {
                        panic!("Too many iterations - sync should complete");
                    }
                }
                ControlFlow::Break(()) => break,
            }
        }

        // Should have completed after processing all mock events
        assert!(iterations > 0);
    }

    #[tokio::test]
    async fn test_perform_sync_batch_stores_data() {
        enable_developer_mode();
        let (storage, _temp_dir) = create_test_storage();
        let pubky = PublicKey::from_str(DEV_MODE_PUBKY).unwrap();
        let pubky_client = create_test_pubky_client();

        let controller =
            BackupController::new(pubky.clone(), storage.clone(), pubky_client, None, None);

        // Perform sync
        let _ = controller.perform_sync_batch().await.unwrap();

        // Check that data was stored
        let size = storage.calculate_pubky_size(&pubky).await;
        assert!(size > 0, "Data should have been stored");
    }

    #[tokio::test]
    async fn test_process_single_event_handles_put() {
        enable_developer_mode();
        let (storage, _temp_dir) = create_test_storage();
        let pubky = PublicKey::from_str(DEV_MODE_PUBKY).unwrap();
        let pubky_client = create_test_pubky_client();

        let controller =
            BackupController::new(pubky.clone(), storage.clone(), pubky_client, None, None);

        // Create a PUT event
        let resource = PubkyResource::new(pubky.clone(), "/pub/test.json").unwrap();
        let event = Event {
            event_type: EventType::Put,
            resource: resource.clone(),
            cursor: pubky::EventCursor::new(1),
            content_hash: None,
        };

        // Process event
        controller.process_single_event(&event).await.unwrap();

        // Verify data was written
        let data = storage.read(&resource).await.unwrap();
        assert!(!data.is_empty());
    }

    #[tokio::test]
    async fn test_process_single_event_handles_delete() {
        let (storage, _temp_dir) = create_test_storage();
        let pubky = PublicKey::from_str(DEV_MODE_PUBKY).unwrap();
        let pubky_client = create_test_pubky_client();

        let controller =
            BackupController::new(pubky.clone(), storage.clone(), pubky_client, None, None);

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
        let pubky1 = PublicKey::from_str(DEV_MODE_PUBKY).unwrap();
        let pubky2 =
            PublicKey::from_str("o4dksfbqk85ogzdb5osziw6befigbuxmuxkuxq8434q89uj56uxo").unwrap();
        let pubky_client = create_test_pubky_client();

        let controller =
            BackupController::new(pubky1.clone(), storage.clone(), pubky_client, None, None);

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
        let pubky = PublicKey::from_str(DEV_MODE_PUBKY).unwrap();
        let pubky_client = create_test_pubky_client();

        let controller =
            BackupController::new(pubky.clone(), storage.clone(), pubky_client, None, None);

        // Create a stream that yields 3 events then fails
        let failing_stream = events::create_failing_mock_event_stream(3);

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
        let pubky = PublicKey::from_str(DEV_MODE_PUBKY).unwrap();
        let pubky_client = create_test_pubky_client();

        // Create controller without any channels
        let controller =
            BackupController::new(pubky.clone(), storage.clone(), pubky_client, None, None);

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
        let pubky = PublicKey::from_str(DEV_MODE_PUBKY).unwrap();
        let pubky_client = create_test_pubky_client();

        let (control_tx, control_rx) = tokio::sync::mpsc::channel(5);
        let (status_tx, mut status_rx) = broadcast::channel(10);

        let controller = BackupController::new(
            pubky,
            storage,
            pubky_client,
            Some(control_rx),
            Some(status_tx),
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
}
