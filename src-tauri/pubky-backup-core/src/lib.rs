mod error;
mod events;
mod storage;
mod utils;
mod watcher;

pub use error::{EventsError, StorageError, SyncError};
pub use events::{Event, EventsResponse, Operation};
pub use storage::{get_data_directory, AppStorage};
pub use utils::retry_with_backoff;
pub use watcher::{FileSystemEvent, FileSystemWatcher};

use log::{debug, error, info, warn};
use pubky::{Keypair, PubkyResource, PubkySigner, PublicKey, PublicStorage, SessionStorage};
use std::env;
use std::ops::ControlFlow;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{broadcast, mpsc};
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

/// Sync mode configuration - determines whether sync is read-only or bidirectional
#[derive(Debug)]
pub enum SyncMode {
    /// Read-only sync: downloads from homeserver to local storage
    ReadOnly { pubky: PublicKey },
    /// Two-way sync: bidirectional sync between local and homeserver
    /// Requires authenticated session
    TwoWay {
        session: SessionStorage,
        pubky: PublicKey,
    },
}

impl SyncMode {
    /// Create a two-way sync mode from a secret key hex string
    ///
    /// # Arguments
    ///
    /// * `secret_key_hex` - Secret key as a 64-character hex string
    ///
    /// # Returns
    ///
    /// A `SyncMode::TwoWay` variant with authenticated session
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Secret key hex string is invalid
    /// - Authentication with homeserver fails
    pub async fn two_way_from_secret(secret_key_hex: &str) -> Result<Self, SyncError> {
        if is_developer_mode() {
            return Err(SyncError::Authentication(
                "Two-way sync not supported in developer mode".to_string(),
            ));
        }

        let hex_string = secret_key_hex.trim();
        if hex_string.len() % 2 != 0 {
            return Err(SyncError::Authentication(
                "Invalid hex string length".to_string(),
            ));
        }

        let mut secret_key_bytes_vec = vec![];
        for i in (0..hex_string.len()).step_by(2) {
            let byte_str = &hex_string[i..i + 2];
            let byte = u8::from_str_radix(byte_str, 16)
                .map_err(|_| SyncError::Authentication("Invalid hex string".to_string()))?;
            secret_key_bytes_vec.push(byte);
        }

        let secret_key_bytes: [u8; 32] = secret_key_bytes_vec.try_into().map_err(|_| {
            SyncError::Authentication("Invalid secret key length (expected 32 bytes)".to_string())
        })?;

        let keypair = Keypair::from_secret_key(&secret_key_bytes);
        let pubky = keypair.public_key();

        // Create signer and sign in
        let signer = PubkySigner::new(keypair)
            .map_err(|e| SyncError::Authentication(format!("Failed to create signer: {}", e)))?;
        let session = signer
            .signin()
            .await
            .map_err(|e| SyncError::Authentication(format!("Failed to sign in: {}", e)))?;

        info!("Two-way sync mode created - authenticated session established");

        Ok(SyncMode::TwoWay {
            session: session.storage(),
            pubky,
        })
    }

    /// Get the public key for this sync mode
    pub fn pubky(&self) -> &PublicKey {
        match self {
            SyncMode::ReadOnly { pubky } => pubky,
            SyncMode::TwoWay { pubky, .. } => pubky,
        }
    }

    /// Get the session storage if in two-way mode
    fn session(&self) -> Option<&SessionStorage> {
        match self {
            SyncMode::ReadOnly { .. } => None,
            SyncMode::TwoWay { session, .. } => Some(session),
        }
    }
}

/// Messages that can be sent to control the sync controller
#[derive(Debug, Clone)]
pub enum SyncControllerMessage {
    /// Stop the sync controller
    Cancel,
    /// Trigger an immediate sync (bypasses the interval timer)
    ForceSync,
}

/// Status updates emitted by the sync controller
#[derive(Debug, Clone)]
pub enum SyncControllerStatus {
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

/// Main sync controller which manages syncing for a Pubky user.
///
/// The controller continuously syncs data between a Pubky homeserver and local storage,
/// polling for new events at regular intervals. Supports both one-way (read-only) and
/// two-way (bidirectional) sync modes.
///
/// # Example (Read-only mode)
///
/// ```no_run
/// use pubky_backup_core::{AppStorage, SyncController, SyncMode, SyncControllerMessage, SyncControllerStatus};
/// use pubky::PublicKey;
/// use std::sync::Arc;
/// use std::str::FromStr;
/// use tokio::sync::broadcast;
///
/// #[tokio::main]
/// async fn main() -> Result<(), Box<dyn std::error::Error>> {
///     // Initialize storage
///     let storage = Arc::new(AppStorage::new()?);
///
///     // Parse the pubky to sync
///     let pubky = PublicKey::from_str("your_pubky_here")?;
///     let mode = SyncMode::ReadOnly { pubky };
///
///     // Create channels for control and status
///     let (control_tx, control_rx) = broadcast::channel(5);
///     let (status_tx, mut status_rx) = broadcast::channel(5);
///
///     // Create and spawn the sync controller
///     let controller = SyncController::new(
///         mode,
///         storage,
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
///                 SyncControllerStatus::Syncing { events_processed } => {
///                     println!("Syncing... ({} events)", events_processed)
///                 },
///                 SyncControllerStatus::Idle => println!("Idle"),
///                 SyncControllerStatus::Ended => println!("Ended"),
///                 SyncControllerStatus::Error { message } => println!("Error: {}", message),
///             }
///         }
///     });
///
///     // Send control messages as needed
///     control_tx.send(SyncControllerMessage::ForceSync)?;
///
///     Ok(())
/// }
/// ```
pub struct SyncController {
    mode: SyncMode,
    storage: Arc<AppStorage>,
    control_rx: Option<broadcast::Receiver<SyncControllerMessage>>,
    status_tx: Option<broadcast::Sender<SyncControllerStatus>>,
}

impl SyncController {
    /// Creates a new sync controller instance.
    ///
    /// # Arguments
    ///
    /// * `mode` - Sync mode (ReadOnly or TwoWay)
    /// * `storage` - Shared storage instance for persisting data
    /// * `control_rx` - Optional receiver for control messages (Cancel, ForceSync)
    /// * `status_tx` - Optional sender for status updates
    ///
    /// # Returns
    ///
    /// A new `SyncController` ready to be run via [`SyncController::run`]
    ///
    /// # Developer Mode
    ///
    /// Enable developer mode by setting the `PUBKY_DEVELOPER_MODE` environment variable.
    /// In developer mode, the controller uses mock data instead of real network calls.
    pub fn new(
        mode: SyncMode,
        storage: Arc<AppStorage>,
        control_rx: Option<broadcast::Receiver<SyncControllerMessage>>,
        status_tx: Option<broadcast::Sender<SyncControllerStatus>>,
    ) -> Self {
        Self {
            mode,
            storage,
            control_rx,
            status_tx,
        }
    }

    /// Get the public key for this controller
    fn pubky(&self) -> &PublicKey {
        self.mode.pubky()
    }

    /// Get the session storage if in two-way mode
    fn session(&self) -> Option<&SessionStorage> {
        self.mode.session()
    }

    /// Handle local file creation or modification - upload to homeserver
    async fn handle_local_file_change(&self, path: PathBuf) -> Result<(), SyncError> {
        let session = self
            .session()
            .ok_or_else(|| SyncError::Upload("No authenticated session available".to_string()))?;

        let data = tokio::fs::read(&path).await.map_err(|e| {
            SyncError::Upload(format!("Failed to read file {}: {}", path.display(), e))
        })?;
        let resource = self.storage.path_to_resource(
            &self.storage.get_pubky_directory_path(self.pubky())?,
            &path,
            self.pubky(),
        )?;

        debug!(
            "Uploading local change: {} ({} bytes)",
            resource,
            data.len()
        );
        session
            .put(&resource.to_string(), data)
            .await
            .map_err(|e| SyncError::Upload(format!("Failed to upload {}: {}", resource, e)))?;

        Ok(())
    }

    /// Handle local file deletion - delete from homeserver
    async fn handle_local_file_delete(&self, resource: &PubkyResource) -> Result<(), SyncError> {
        let session = self
            .session()
            .ok_or_else(|| SyncError::Delete("No authenticated session available".to_string()))?;

        info!("Deleting from homeserver: {}", resource);
        session
            .delete(&resource.to_string())
            .await
            .map_err(|e| SyncError::Delete(format!("Failed to delete {}: {}", resource, e)))?;

        Ok(())
    }

    /// Runs the sync controller loop.
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
    /// 4. If in two-way mode: upload local changes and deletions
    /// 5. Wait for the next sync interval (30 seconds)
    /// 6. Emit status updates via the status channel
    ///
    /// # Panics
    ///
    /// This method should not panic under normal circumstances. All errors are
    /// logged and result in an `Error` status being sent before the controller stops.
    pub async fn run(mut self) {
        // Start filesystem watcher if two-way sync is enabled
        let mut fs_event_rx: Option<mpsc::UnboundedReceiver<FileSystemEvent>> = None;
        if self.session().is_some() {
            match FileSystemWatcher::new(self.pubky().clone(), self.storage.clone()) {
                Ok(watcher) => match watcher.start() {
                    Ok(rx) => {
                        fs_event_rx = Some(rx);
                        info!("Filesystem watcher started for two-way sync");
                    }
                    Err(e) => {
                        warn!("Failed to start filesystem watcher: {}", e);
                    }
                },
                Err(e) => {
                    warn!("Failed to create filesystem watcher: {}", e);
                }
            }
        }

        let mut interval = time::interval(Duration::from_secs(SYNC_INTERVAL_SECONDS));

        loop {
            tokio::select! {
                _ = interval.tick() => {

                    self.send_status(SyncControllerStatus::Syncing{events_processed: 0});

                    match self.perform_sync_batch().await {
                        Ok(ControlFlow::Continue(events_processed)) => {
                            // More events available, send status and keep syncing immediately
                            self.send_status(SyncControllerStatus::Syncing { events_processed });
                            interval = time::interval_at(
                                time::Instant::now(),
                                Duration::from_secs(SYNC_INTERVAL_SECONDS)
                            );
                        }
                        Ok(ControlFlow::Break(())) => {
                            // Sync complete
                        }
                        Err(e) => {
                            error!("Critical sync batch failure - terminating sync controller: {}", e);
                            self.send_status(SyncControllerStatus::Error {
                                message: format!("Critical sync batch failure: {}", e),
                            });
                            return;
                        }
                    }

                    self.send_status(SyncControllerStatus::Idle);
                }
                // Handle filesystem events for two-way sync
                fs_event = async {
                    if let Some(ref mut rx) = fs_event_rx {
                        rx.recv().await
                    } else {
                        std::future::pending().await
                    }
                } => {
                    if let Some(event) = fs_event {
                        match event {
                            FileSystemEvent::CreateOrModify { resource: _, path } => {
                                if let Err(e) = self.handle_local_file_change(path.clone()).await {
                                    error!("Failed to upload local file change {}: {}", path.display(), e);
                                    // Don't terminate on upload errors, just log them
                                    self.storage.write_error(
                                        &path.to_string_lossy(),
                                        &format!("Upload failed: {}", e)
                                    ).await.ok();
                                }
                            }
                            FileSystemEvent::Delete { resource } => {
                                if let Err(e) = self.handle_local_file_delete(&resource).await {
                                    error!("Failed to delete from homeserver {}: {}", resource, e);
                                    // Don't terminate on delete errors, just log them
                                    self.storage.write_error(
                                        &resource.to_string(),
                                        &format!("Delete failed: {}", e)
                                    ).await.ok();
                                }
                            }
                        }
                    }
                }
                msg = async {
                    if let Some(ref mut rx) = self.control_rx {
                        rx.recv().await
                    } else {
                        std::future::pending().await
                    }
                } => {
                    match msg {
                        Ok(SyncControllerMessage::Cancel) => {
                            info!("Sync controller task cancelled");
                            self.send_status(SyncControllerStatus::Ended);
                            // Break out of loop ending task
                            break;
                        }
                        Ok(SyncControllerMessage::ForceSync) => {
                            info!("Force sync triggered");
                            // Reset interval to trigger immediately
                            interval = time::interval_at(
                                time::Instant::now(),
                                Duration::from_secs(SYNC_INTERVAL_SECONDS)
                            );
                        }
                        Err(e) => {
                            warn!("Sync controller task closed: {}", e);
                            self.send_status(SyncControllerStatus::Ended);
                            break;
                        }
                    }
                }
            }
        }
    }

    fn send_status(&self, status: SyncControllerStatus) {
        if let Some(tx) = &self.status_tx {
            if let Err(e) = tx.send(status.clone()) {
                warn!(
                    "Failed to send status update (no receivers or lagging): {:?}",
                    e
                );
            }
        }
    }

    /// Process one batch of sync events
    async fn perform_sync_batch(&self) -> Result<ControlFlow<(), usize>, SyncError> {
        let cursor = self.storage.read_cursor(self.pubky()).await?;

        // Check if developer mode is enabled - use mock events if so
        let events_response = if is_developer_mode() {
            events::get_mock_events_response(&cursor)?
        } else {
            match events::fetch_events(&cursor, self.pubky()).await {
                Ok(response) => response,
                Err(crate::EventsError::FetchFailed(msg)) => {
                    error!("Sync events fetch failed: {}", msg);
                    // Treat network fetch failures as recoverable - retry on next sync interval
                    return Ok(ControlFlow::Break(()));
                }
                Err(e) => {
                    // Other errors (e.g., InvalidResponse) are critical
                    error!("Critical sync error: {}", e);
                    self.storage
                        .write_error("/events/", &format!("Critical error: {}", e))
                        .await?;
                    return Err(e.into());
                }
            }
        };

        let num_events = events_response.events().len();
        info!("Fetched {} events", num_events);

        if num_events > 0 {
            // Process those events
            self.process_events(events_response.events()).await?;

            // Store new cursor
            self.storage
                .write_cursor(self.pubky(), events_response.cursor.clone())
                .await?;

            Ok(ControlFlow::Continue(num_events))
        } else {
            Ok(ControlFlow::Break(()))
        }
    }

    /// Take a list of events and store the data of those which belong to a given pubky
    async fn process_events(&self, events: &[Event]) -> Result<(), SyncError> {
        for event in events {
            match event {
                Event::Invalid { url, error } => {
                    // Log invalid events and continue
                    let _ = self.storage.write_error(url, error).await;
                    warn!("Invalid event: {} - {}", url, error);
                    continue;
                }
                Event::Valid {
                    operation,
                    resource,
                } => {
                    // Skip events for other pubkys
                    // TODO: Filter server-side
                    if &resource.owner != self.pubky() {
                        continue;
                    }

                    match operation {
                        Operation::Put => {
                            debug!("Processing PUT event for: {}", resource);
                            match self.fetch_pubky_resource_data(resource).await {
                                Ok(data_vec) => {
                                    // Skip storing empty data (404 responses)
                                    if !data_vec.is_empty() {
                                        self.storage.write(resource, data_vec).await?;
                                    }
                                }
                                Err(e) => {
                                    // Log fetch errors and continue processing other events
                                    self.storage
                                        .write_error(
                                            &resource.to_string(),
                                            &format!("Fetch failed: {}", e),
                                        )
                                        .await?;
                                }
                            }
                        }
                        Operation::Delete => {
                            debug!("Processing DEL event for: {}", resource);
                            self.storage.delete(resource).await?;
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Fetch data from a PubkyResource url
    async fn fetch_pubky_resource_data(
        &self,
        resource: &PubkyResource,
    ) -> Result<Vec<u8>, SyncError> {
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
                return Err(SyncError::Internal(format!(
                    "Failed to fetch data for {}: {}",
                    resource, e
                )));
            }
        };

        let data = response.bytes().await.map_err(|e| {
            SyncError::Internal(format!(
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
        let storage = AppStorage::new_with_single_path(&temp_dir.path().to_path_buf()).unwrap();
        (Arc::new(storage), temp_dir)
    }

    #[tokio::test]
    async fn test_controller_runs_and_can_be_cancelled() {
        let (storage, _temp_dir) = create_test_storage();
        let pubky = PublicKey::from_str(DEV_MODE_PUBKY).unwrap();

        let (control_tx, control_rx) = broadcast::channel(5);
        let (status_tx, mut status_rx) = broadcast::channel(10);

        let mode = SyncMode::ReadOnly { pubky };
        let controller = SyncController::new(mode, storage, Some(control_rx), Some(status_tx));

        // Spawn the controller
        let handle = tokio::spawn(controller.run());

        // Wait for first status update (should be Syncing or Idle)
        let status = tokio::time::timeout(Duration::from_secs(5), status_rx.recv())
            .await
            .expect("Should receive status")
            .unwrap();

        match status {
            SyncControllerStatus::Syncing { .. } | SyncControllerStatus::Idle => {}
            _ => panic!("Unexpected SyncControllerStatus"),
        }

        // Send cancel message
        control_tx.send(SyncControllerMessage::Cancel).unwrap();

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

        let (control_tx, control_rx) = broadcast::channel(5);
        let (status_tx, mut status_rx) = broadcast::channel(10);

        let mode = SyncMode::ReadOnly { pubky };
        let controller = SyncController::new(mode, storage, Some(control_rx), Some(status_tx));

        tokio::spawn(controller.run());

        // Trigger force sync
        control_tx.send(SyncControllerMessage::ForceSync).unwrap();

        // Should receive Syncing status
        let status = tokio::time::timeout(Duration::from_secs(5), status_rx.recv())
            .await
            .expect("Should receive status")
            .unwrap();

        assert!(matches!(status, SyncControllerStatus::Syncing { .. }));

        // Cleanup
        control_tx.send(SyncControllerMessage::Cancel).unwrap();
    }

    #[tokio::test]
    async fn test_perform_sync_batch_initial_sync() {
        let (storage, _temp_dir) = create_test_storage();
        let pubky = PublicKey::from_str(DEV_MODE_PUBKY).unwrap();

        let mode = SyncMode::ReadOnly {
            pubky: pubky.clone(),
        };
        let controller = SyncController::new(mode, storage.clone(), None, None);

        // First sync should return Continue (more events available)
        let result = controller.perform_sync_batch().await.unwrap();
        assert!(matches!(result, ControlFlow::Continue(_)));

        // Cursor should have been updated
        let cursor = storage.read_cursor(&pubky).await.unwrap();
        assert!(!cursor.is_empty());
        assert_eq!(cursor, "cursor001");
    }

    #[tokio::test]
    async fn test_perform_sync_batch_completes() {
        let (storage, _temp_dir) = create_test_storage();
        let pubky = PublicKey::from_str(DEV_MODE_PUBKY).unwrap();

        let mode = SyncMode::ReadOnly {
            pubky: pubky.clone(),
        };
        let controller = SyncController::new(mode, storage.clone(), None, None);

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

        let mode = SyncMode::ReadOnly {
            pubky: pubky.clone(),
        };
        let controller = SyncController::new(mode, storage.clone(), None, None);

        // Perform sync
        let _ = controller.perform_sync_batch().await.unwrap();

        // Check that data was stored
        let size = storage.calculate_pubky_size(&pubky).await;
        assert!(size > 0, "Data should have been stored");
    }

    #[tokio::test]
    async fn test_process_events_handles_put() {
        let (storage, _temp_dir) = create_test_storage();
        let pubky = PublicKey::from_str(DEV_MODE_PUBKY).unwrap();

        let mode = SyncMode::ReadOnly {
            pubky: pubky.clone(),
        };
        let controller = SyncController::new(mode, storage.clone(), None, None);

        // Create a PUT event
        let resource = PubkyResource::new(pubky.clone(), "/pub/test.json").unwrap();
        let events = vec![Event::Valid {
            operation: Operation::Put,
            resource: resource.clone(),
        }];

        // Process events
        controller.process_events(&events).await.unwrap();

        // Verify data was written
        let data = storage.read(&resource).await.unwrap();
        assert!(!data.is_empty());
    }

    #[tokio::test]
    async fn test_process_events_handles_delete() {
        let (storage, _temp_dir) = create_test_storage();
        let pubky = PublicKey::from_str(DEV_MODE_PUBKY).unwrap();

        let mode = SyncMode::ReadOnly {
            pubky: pubky.clone(),
        };
        let controller = SyncController::new(mode, storage.clone(), None, None);

        // First create a resource
        let resource = PubkyResource::new(pubky.clone(), "/pub/test.json").unwrap();
        storage
            .write(&resource, b"test data".to_vec())
            .await
            .unwrap();

        // Verify it exists
        assert!(storage.read(&resource).await.is_ok());

        // Create a DELETE event
        let events = vec![Event::Valid {
            operation: Operation::Delete,
            resource: resource.clone(),
        }];

        // Process events
        controller.process_events(&events).await.unwrap();

        // Verify data was deleted
        assert!(storage.read(&resource).await.is_err());
    }

    #[tokio::test]
    async fn test_process_events_handles_invalid() {
        let (storage, _temp_dir) = create_test_storage();
        let pubky = PublicKey::from_str(DEV_MODE_PUBKY).unwrap();

        let mode = SyncMode::ReadOnly {
            pubky: pubky.clone(),
        };
        let controller = SyncController::new(mode, storage.clone(), None, None);

        // Create an invalid event
        let events = vec![Event::Invalid {
            url: "invalid://url".to_string(),
            error: "Test error".to_string(),
        }];

        // Process events - should not fail, just log
        controller.process_events(&events).await.unwrap();
    }

    #[tokio::test]
    async fn test_process_events_skips_other_pubky() {
        let (storage, _temp_dir) = create_test_storage();
        let pubky1 = PublicKey::from_str(DEV_MODE_PUBKY).unwrap();
        let pubky2 =
            PublicKey::from_str("o4dksfbqk85ogzdb5osziw6befigbuxmuxkuxq8434q89uj56uxo").unwrap();

        let mode = SyncMode::ReadOnly {
            pubky: pubky1.clone(),
        };
        let controller = SyncController::new(mode, storage.clone(), None, None);

        // Create event for a different pubky
        let resource = PubkyResource::new(pubky2.clone(), "/pub/test.json").unwrap();
        let events = vec![Event::Valid {
            operation: Operation::Put,
            resource: resource.clone(),
        }];

        // Process events
        controller.process_events(&events).await.unwrap();

        // Verify data was NOT written for the other pubky
        assert!(storage.read(&resource).await.is_err());
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

        let mode = SyncMode::ReadOnly {
            pubky: pubky.clone(),
        };
        let controller = SyncController::new(mode, storage.clone(), None, None);

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
