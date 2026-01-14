mod error;
mod events;
mod storage;
mod storage_migration;
mod utils;

pub use error::{BackupError, EventsError, StorageError};
pub use storage::{get_data_directory, AppStorage};
pub use utils::retry_with_backoff;

// Re-export SDK types used in our public API
pub use pubky::{Event, EventType};

use futures_util::StreamExt;
use log::{debug, error, info, warn};
use pubky::{Pubky, PubkyResource, PublicKey, PublicStorage};
use std::env;
use std::ops::ControlFlow;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;
use tokio::time;

/// Developer mode mock pubky (for testing without real pubky)
pub const DEV_MODE_PUBKY: &str = "g1b6wp8bhhxtsksy3td7rj6mgg7s5k8c68663sajkfscshwj8g5y";
const SYNC_INTERVAL_SECONDS: u64 = 30;

/// Check if developer mode is enabled via environment variable.
///
/// Developer mode uses mock data instead of real network calls.
/// Enable by setting `PUBKY_DEVELOPER_MODE` environment variable.
///
/// # Example
///
/// ```bash
/// PUBKY_DEVELOPER_MODE=1 cargo run
/// ```
pub fn is_developer_mode() -> bool {
    env::var("PUBKY_DEVELOPER_MODE").is_ok()
}

/// Messages that can be sent to control the backup controller
#[derive(Debug, Clone)]
pub enum BackupControllerMessage {
    /// Stop the backup controller
    Cancel,
    /// Trigger an immediate sync (bypasses the interval timer)
    ForceSync,
}

/// Status updates emitted by the backup controller
#[derive(Debug, Clone)]
pub enum BackupControllerStatus {
    /// Controller is actively syncing data
    Syncing {
        /// Number of events processed in this sync batch
        events_processed: usize,
    },
    /// Controller is idle, waiting for next sync interval
    Idle,
    /// Controller has been stopped gracefully
    Ended,
    /// Controller encountered a critical error and stopped
    Error { message: String },
}

/// Main backup controller which manages the backup process for a Pubky user.
///
/// The controller continuously syncs data from a Pubky homeserver to local storage,
/// processing events as they stream from the homeserver.
///
/// # Example
///
/// ```no_run
/// use pubky_backup_core::{AppStorage, BackupController, BackupControllerMessage, BackupControllerStatus};
/// use pubky::{Pubky, PublicKey};
/// use std::sync::Arc;
/// use std::str::FromStr;
/// use tokio::sync::broadcast;
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
///     // Create channels for control and status
///     let (control_tx, control_rx) = broadcast::channel(5);
///     let (status_tx, mut status_rx) = broadcast::channel(5);
///
///     // Create and spawn the backup controller
///     let controller = BackupController::new(
///         pubky,
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
///                 BackupControllerStatus::Syncing { events_processed } => {
///                     println!("Syncing... ({} events)", events_processed)
///                 },
///                 BackupControllerStatus::Idle => println!("Idle"),
///                 BackupControllerStatus::Ended => println!("Ended"),
///                 BackupControllerStatus::Error { message } => println!("Error: {}", message),
///             }
///         }
///     });
///
///     // Send control messages as needed
///     control_tx.send(BackupControllerMessage::ForceSync)?;
///
///     Ok(())
/// }
/// ```
pub struct BackupController {
    pubky: PublicKey,
    storage: Arc<AppStorage>,
    pubky_client: Arc<Pubky>,
    control_rx: Option<broadcast::Receiver<BackupControllerMessage>>,
    status_tx: Option<broadcast::Sender<BackupControllerStatus>>,
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
    /// * `status_tx` - Optional sender for status updates
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
        control_rx: Option<broadcast::Receiver<BackupControllerMessage>>,
        status_tx: Option<broadcast::Sender<BackupControllerStatus>>,
    ) -> Self {
        Self {
            pubky,
            storage,
            pubky_client,
            control_rx,
            status_tx,
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
        let mut interval = time::interval(Duration::from_secs(SYNC_INTERVAL_SECONDS));

        loop {
            tokio::select! {
                _ = interval.tick() => {

                    self.send_status(BackupControllerStatus::Syncing{events_processed: 0});

                    match self.perform_sync_batch().await {
                        Ok(ControlFlow::Continue(events_processed)) => {
                            // More events available, send status and keep syncing immediately
                            self.send_status(BackupControllerStatus::Syncing { events_processed });
                            interval = time::interval_at(
                                time::Instant::now(),
                                Duration::from_secs(SYNC_INTERVAL_SECONDS)
                            );
                        }
                        Ok(ControlFlow::Break(())) => {
                            // Sync complete
                        }
                        Err(e) => {
                            error!("Critical sync batch failure - terminating backup controller: {}", e);
                            self.send_status(BackupControllerStatus::Error {
                                message: format!("Critical sync batch failure: {}", e),
                            });
                            return;
                        }
                    }

                    self.send_status(BackupControllerStatus::Idle);
                }
                msg = async {
                    if let Some(ref mut rx) = self.control_rx {
                        rx.recv().await
                    } else {
                        std::future::pending().await
                    }
                } => {
                    match msg {
                        Ok(BackupControllerMessage::Cancel) => {
                            info!("Backup controller task cancelled");
                            self.send_status(BackupControllerStatus::Ended);
                            // Break out of loop ending task
                            break;
                        }
                        Ok(BackupControllerMessage::ForceSync) => {
                            info!("Force sync triggered");
                            // Reset interval to trigger immediately
                            interval = time::interval_at(
                                time::Instant::now(),
                                Duration::from_secs(SYNC_INTERVAL_SECONDS)
                            );
                        }
                        Err(e) => {
                            warn!("Backup controller task closed: {}", e);
                            self.send_status(BackupControllerStatus::Ended);
                            break;
                        }
                    }
                }
            }
        }
    }

    fn send_status(&self, status: BackupControllerStatus) {
        if let Some(tx) = &self.status_tx {
            if let Err(e) = tx.send(status.clone()) {
                warn!(
                    "Failed to send status update (no receivers or lagging): {:?}",
                    e
                );
            }
        }
    }

    /// Process events by streaming from the homeserver (or mock stream in developer mode)
    async fn perform_sync_batch(&self) -> Result<ControlFlow<(), usize>, BackupError> {
        let cursor = self.storage.read_cursor(&self.pubky).await?;

        // Get event stream - mock stream in developer mode, real stream otherwise
        let mut event_stream = if is_developer_mode() {
            events::create_mock_event_stream(cursor)
        } else {
            match events::create_event_stream(&self.pubky_client, &self.pubky, cursor).await {
                Ok(stream) => stream,
                Err(crate::EventsError::FetchFailed(msg)) => {
                    error!("Sync events fetch failed: {}", msg);
                    // Treat network fetch failures as recoverable - retry on next sync interval
                    return Ok(ControlFlow::Break(()));
                }
                Err(e) => {
                    // Other errors are critical
                    error!("Critical sync error: {}", e);
                    self.storage
                        .write_error(&self.pubky, "/events/", &format!("Critical error: {}", e))
                        .await?;
                    return Err(e.into());
                }
            }
        };

        let mut events_processed = 0;
        let mut last_cursor: Option<u64> = cursor;

        // Process events as they stream in
        while let Some(event_result) = event_stream.next().await {
            match event_result {
                Ok(event) => {
                    let event_cursor = event.cursor.id();
                    self.process_single_event(&event).await?;
                    events_processed += 1;
                    last_cursor = Some(event_cursor);

                    // Save cursor periodically (every 100 events)
                    if events_processed % 100 == 0 {
                        if let Some(c) = last_cursor {
                            self.storage.write_cursor(&self.pubky, c).await?;
                        }
                    }
                }
                Err(e) => {
                    error!("Event stream error: {}", e);
                    // Save progress before returning
                    if let Some(c) = last_cursor {
                        self.storage.write_cursor(&self.pubky, c).await?;
                    }
                    return Err(e.into());
                }
            }
        }

        info!("Processed {} events", events_processed);

        // Save final cursor
        if let Some(c) = last_cursor {
            if events_processed > 0 {
                self.storage.write_cursor(&self.pubky, c).await?;
            }
        }

        if events_processed > 0 {
            Ok(ControlFlow::Continue(events_processed))
        } else {
            Ok(ControlFlow::Break(()))
        }
    }

    /// Process a single event
    async fn process_single_event(&self, event: &Event) -> Result<(), BackupError> {
        // Skip events for other pubkys
        if event.resource.owner != self.pubky {
            return Ok(());
        }

        match event.event_type {
            EventType::Put => {
                debug!("Processing PUT event for: {}", event.resource);
                match self.fetch_pubky_resource_data(&event.resource).await {
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

    /// Fetch data from a PubkyResource url
    async fn fetch_pubky_resource_data(
        &self,
        resource: &PubkyResource,
    ) -> Result<Vec<u8>, BackupError> {
        if is_developer_mode() {
            return Ok(get_mock_pubky_resource_data(&resource.to_string()));
        }

        let response = match retry_with_backoff(|| async {
            PublicStorage::new()
                .map_err(|e| format!("Failed to create PublicStorage: {}", e))?
                .get(resource)
                .await
                .map_err(|e| format!("{}", e))
        })
        .await
        {
            Ok(response) => response,
            Err(e) => {
                // TODO: Is it correct that 404s are returned as Error rather than Ok response with status = 404?
                if e.contains("404") || e.to_lowercase().contains("not found") {
                    info!("404 response: Returning empty data for {}", resource);
                    return Ok(Vec::new());
                }
                return Err(BackupError::Internal(format!(
                    "Failed to fetch data for {}: {}",
                    resource, e
                )));
            }
        };

        let data = response.bytes().await.map_err(|e| {
            BackupError::Internal(format!(
                "Failed to read response bytes for {}: {}",
                resource, e
            ))
        })?;

        let data_vec = data.to_vec();
        debug!("Successfully fetched data: {} bytes", data_vec.len());
        Ok(data_vec)
    }
}

/// Generate mock data for a given pubky URL in developer mode
fn get_mock_pubky_resource_data(url: &str) -> Vec<u8> {
    if url.contains("/profile") {
        r#"{"name":"Mock User","bio":"This is mock profile data for development","avatar":"https://example.com/avatar.jpg"}"#.as_bytes().to_vec()
    } else if url.contains("/posts/") {
        let post_id = url.split('/').next_back().unwrap_or("unknown");
        format!(r#"{{"id":"{}","content":"This is mock post content for {}","timestamp":"2024-01-01T12:00:00Z","author":"Mock User"}}"#, post_id, post_id).as_bytes().to_vec()
    } else if url.contains("/follows") {
        r#"{"following":["pubky1","pubky2","pubky3"],"followers":["pubky4","pubky5"]}"#
            .as_bytes()
            .to_vec()
    } else {
        // Generic mock data
        format!(
            r#"{{"url":"{}","data":"Mock data for development","type":"generic"}}"#,
            url
        )
        .as_bytes()
        .to_vec()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;
    use tempfile::TempDir;

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
        let (storage, _temp_dir) = create_test_storage();
        let pubky = PublicKey::from_str(DEV_MODE_PUBKY).unwrap();
        let pubky_client = create_test_pubky_client();

        let (control_tx, control_rx) = broadcast::channel(5);
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
            BackupControllerStatus::Syncing { .. } | BackupControllerStatus::Idle => {}
            _ => panic!("Unexpected BackupControllerStatus"),
        }

        // Send cancel message
        control_tx.send(BackupControllerMessage::Cancel).unwrap();

        // Controller should finish
        tokio::time::timeout(Duration::from_secs(2), handle)
            .await
            .expect("Controller should finish")
            .unwrap();
    }

    #[tokio::test]
    async fn test_controller_force_sync() {
        let (storage, _temp_dir) = create_test_storage();
        let pubky = PublicKey::from_str(DEV_MODE_PUBKY).unwrap();
        let pubky_client = create_test_pubky_client();

        let (control_tx, control_rx) = broadcast::channel(5);
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
        control_tx.send(BackupControllerMessage::ForceSync).unwrap();

        // Should receive Syncing status
        let status = tokio::time::timeout(Duration::from_secs(5), status_rx.recv())
            .await
            .expect("Should receive status")
            .unwrap();

        assert!(matches!(status, BackupControllerStatus::Syncing { .. }));

        // Cleanup
        control_tx.send(BackupControllerMessage::Cancel).unwrap();
    }

    #[tokio::test]
    async fn test_perform_sync_batch_initial_sync() {
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
    async fn test_create_mock_event_stream_yields_events() {
        use futures_util::StreamExt;

        // Test initial stream (no cursor) yields 3 PUT events with cursors 1, 2, 3
        let mut stream = events::create_mock_event_stream(None);
        let mut events: Vec<Event> = vec![];
        while let Some(result) = stream.next().await {
            events.push(result.unwrap());
        }
        assert_eq!(events.len(), 3, "Initial batch should have 3 events");
        assert_eq!(events[0].cursor.id(), 1);
        assert_eq!(events[1].cursor.id(), 2);
        assert_eq!(events[2].cursor.id(), 3);
        assert!(events.iter().all(|e| e.event_type == EventType::Put));

        // Test stream with cursor=3 yields 2 events (PUT and DELETE)
        let mut stream = events::create_mock_event_stream(Some(3));
        let mut events: Vec<Event> = vec![];
        while let Some(result) = stream.next().await {
            events.push(result.unwrap());
        }
        assert_eq!(events.len(), 2, "Second batch should have 2 events");
        assert_eq!(events[0].cursor.id(), 4);
        assert_eq!(events[0].event_type, EventType::Put);
        assert_eq!(events[1].cursor.id(), 5);
        assert_eq!(events[1].event_type, EventType::Delete);

        // Test stream with cursor=5 yields 1 event (third batch)
        let mut stream = events::create_mock_event_stream(Some(5));
        let mut events: Vec<Event> = vec![];
        while let Some(result) = stream.next().await {
            events.push(result.unwrap());
        }
        assert_eq!(events.len(), 1, "Third batch should have 1 event");
        assert_eq!(events[0].cursor.id(), 6);

        // Test stream with cursor=6 yields 0 events (exhausted)
        let stream = events::create_mock_event_stream(Some(6));
        let events: Vec<_> = stream.collect().await;
        assert_eq!(events.len(), 0, "Exhausted stream should have 0 events");
    }

    #[tokio::test]
    async fn test_mock_pubky_resource_data() {
        // Test profile data
        let profile_data = get_mock_pubky_resource_data("pubky://test/pub/profile.json");
        assert!(!profile_data.is_empty());
        assert!(String::from_utf8(profile_data)
            .unwrap()
            .contains("Mock User"));

        // Test posts data
        let post_data = get_mock_pubky_resource_data("pubky://test/pub/posts/123");
        assert!(!post_data.is_empty());
        assert!(String::from_utf8(post_data).unwrap().contains("123"));

        // Test follows data
        let follows_data = get_mock_pubky_resource_data("pubky://test/pub/follows");
        assert!(!follows_data.is_empty());
        assert!(String::from_utf8(follows_data).unwrap().contains("pubky1"));

        // Test generic data
        let generic_data = get_mock_pubky_resource_data("pubky://test/pub/other");
        assert!(!generic_data.is_empty());
        assert!(String::from_utf8(generic_data)
            .unwrap()
            .contains("Mock data"));
    }

    #[tokio::test]
    async fn test_fetch_pubky_resource_data_developer_mode() {
        let (storage, _temp_dir) = create_test_storage();
        let pubky = PublicKey::from_str(DEV_MODE_PUBKY).unwrap();
        let pubky_client = create_test_pubky_client();

        let controller =
            BackupController::new(pubky.clone(), storage.clone(), pubky_client, None, None);

        // Fetch mock resource data
        let resource = PubkyResource::new(pubky.clone(), "/pub/profile.json").unwrap();
        let data = controller
            .fetch_pubky_resource_data(&resource)
            .await
            .unwrap();

        // Should return mock data
        assert!(!data.is_empty());
        assert!(String::from_utf8(data).unwrap().contains("Mock User"));
    }
}
