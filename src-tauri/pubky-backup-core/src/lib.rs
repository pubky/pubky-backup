mod error;
mod events;
mod storage;
mod utils;

pub use error::{BackupError, EventsError, StorageError};
pub use events::{Event, EventsResponse, Operation};
pub use storage::{AppStorage, get_data_directory};
pub use utils::retry_with_backoff;

use log::{debug, error, info, warn};
use pubky::{PubkyResource, PublicKey, PublicStorage};
use std::ops::ControlFlow;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;
use tokio::time;

const SYNC_INTERVAL_SECONDS: u64 = 30;

#[derive(Debug, Clone)]
pub enum BackupControllerMessage {
    Cancel,
    ForceSync,
}

#[derive(Debug, Clone)]
pub enum BackupStatus {
    Syncing,
    Idle,
    Error { message: String },
}

/// Main backup controller which manages the backup process
pub struct BackupController {
    pubky: PublicKey,
    storage: Arc<AppStorage>,
    control_rx: Option<broadcast::Receiver<BackupControllerMessage>>,
    status_tx: Option<broadcast::Sender<BackupStatus>>,
    developer_mode: bool,
}

impl BackupController {
    pub fn new(
        pubky: PublicKey,
        storage: Arc<AppStorage>,
        control_rx: Option<broadcast::Receiver<BackupControllerMessage>>,
        status_tx: Option<broadcast::Sender<BackupStatus>>,
        developer_mode: bool,
    ) -> Self {
        Self {
            pubky,
            storage,
            control_rx,
            status_tx,
            developer_mode,
        }
    }

    /// Main backup task controller:
    ///     1) Take a Public Key
    ///     2) Fetch and store all public data
    ///
    /// Currently spins up a single async task which pulls batches of /events/ and processes them immediately.
    /// Once all events have been processed it polls for more events every SYNC_INTERVAL_SECONDS.
    pub async fn run(mut self) {
        let mut interval = time::interval(Duration::from_secs(SYNC_INTERVAL_SECONDS));

        loop {
            tokio::select! {
                _ = interval.tick() => {

                    self.send_status(BackupStatus::Syncing);

                    match self.perform_sync_batch().await {
                        Ok(ControlFlow::Continue(())) => {
                            // More events available, keep syncing immediately
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
                            self.send_status(BackupStatus::Error {
                                message: format!("Critical sync batch failure: {}", e),
                            });
                            return;
                        }
                    }

                    self.send_status(BackupStatus::Idle);
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
                            self.send_status(BackupStatus::Idle);
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
                            break;
                        }
                    }
                }
            }
        }
    }

    fn send_status(&self, status: BackupStatus) {
        if let Some(tx) = &self.status_tx {
            let _ = tx.send(status);
        }
    }

    /// Process one batch of sync events
    async fn perform_sync_batch(&self) -> Result<ControlFlow<(), ()>, BackupError> {
        let cursor = self.storage.read_cursor(&self.pubky).await?;

        // Check if developer mode is enabled - use mock events if so
        let events_response = if self.developer_mode {
            events::get_mock_events_response(&cursor)?
        } else {
            match events::fetch_events(&cursor, &self.pubky).await {
                Ok(response) => response,
                Err(e) => {
                    error!("Sync events fetch failed: {}", e);
                    self.storage
                        .write_error("/events/", &format!("Fetch failed: {}", e))
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
                .write_cursor(&self.pubky, events_response.cursor.clone())
                .await?;

            Ok(ControlFlow::Continue(()))
        } else {
            Ok(ControlFlow::Break(()))
        }
    }

    /// Take a list of events and store the data of those which belong to a given pubky
    async fn process_events(&self, events: &[Event]) -> Result<(), BackupError> {
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
                    if &resource.owner != &self.pubky {
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
    async fn fetch_pubky_resource_data(&self, resource: &PubkyResource) -> Result<Vec<u8>, BackupError> {
        if self.developer_mode {
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
