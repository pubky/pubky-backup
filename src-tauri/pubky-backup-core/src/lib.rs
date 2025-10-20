mod error;
mod storage;
mod utils;

pub use error::{BackupError, EventsError, StorageError};
pub use storage::{get_data_directory, AppStorage};
pub use utils::retry_with_backoff;

use log::{debug, error, info, warn};
use pubky::{PubkySession, PublicKey};
use std::{
    collections::HashMap,
    env,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, SystemTime},
};
use tokio::{fs, sync::broadcast, time};
use walkdir::WalkDir;

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
        /// Number of operations performed in this sync batch
        events_processed: usize,
    },
    /// Controller is idle, waiting for next sync interval
    Idle,
    /// Controller has been stopped gracefully
    Ended,
    /// Controller encountered a critical error and stopped
    Error { message: String },
}

/// Main backup controller which monitors a local directory and publishes
/// changes to a Pubky homeserver for the authenticated user.
pub struct BackupController {
    pubky: PublicKey,
    session: Arc<PubkySession>,
    local_pub_dir: PathBuf,
    control_rx: Option<broadcast::Receiver<BackupControllerMessage>>,
    status_tx: Option<broadcast::Sender<BackupControllerStatus>>,
    known_files: HashMap<PathBuf, SystemTime>,
}

impl BackupController {
    /// Creates a new backup controller instance.
    pub fn new(
        pubky: PublicKey,
        session: Arc<PubkySession>,
        local_pub_dir: PathBuf,
        control_rx: Option<broadcast::Receiver<BackupControllerMessage>>,
        status_tx: Option<broadcast::Sender<BackupControllerStatus>>,
    ) -> Self {
        Self {
            pubky,
            session,
            local_pub_dir,
            control_rx,
            status_tx,
            known_files: HashMap::new(),
        }
    }

    /// Runs the backup controller loop.
    pub async fn run(mut self) {
        let mut interval = time::interval(Duration::from_secs(SYNC_INTERVAL_SECONDS));

        loop {
            let control_fut = async {
                if let Some(rx) = &mut self.control_rx {
                    match rx.recv().await {
                        Ok(msg) => Some(msg),
                        Err(_) => Some(BackupControllerMessage::Cancel),
                    }
                } else {
                    None
                }
            };

            tokio::select! {
                Some(message) = control_fut => {
                    match message {
                        BackupControllerMessage::Cancel => {
                            info!("Backup controller cancellation received for {}", self.pubky);
                            self.send_status(BackupControllerStatus::Ended);
                            break;
                        }
                        BackupControllerMessage::ForceSync => {
                            if let Err(e) = self.perform_sync().await {
                                self.report_error(e);
                                break;
                            }
                        }
                    }
                }
                _ = interval.tick() => {
                    if let Err(e) = self.perform_sync().await {
                        self.report_error(e);
                        break;
                    }
                }
            }
        }

        debug!("Backup controller loop ended for {}", self.pubky);
    }

    fn send_status(&self, status: BackupControllerStatus) {
        if let Some(tx) = &self.status_tx {
            let _ = tx.send(status);
        }
    }

    fn report_error(&self, error: BackupError) {
        error!("Backup controller error for {}: {}", self.pubky, error);
        self.send_status(BackupControllerStatus::Error {
            message: error.to_string(),
        });
    }

    async fn perform_sync(&mut self) -> Result<(), BackupError> {
        self.send_status(BackupControllerStatus::Syncing {
            events_processed: 0,
        });

        let processed = self.sync_local_changes().await?;

        self.send_status(BackupControllerStatus::Syncing {
            events_processed: processed,
        });
        self.send_status(BackupControllerStatus::Idle);

        Ok(())
    }

    async fn sync_local_changes(&mut self) -> Result<usize, BackupError> {
        let files = self.collect_local_files().await?;
        let mut new_snapshot: HashMap<PathBuf, SystemTime> = HashMap::new();
        let mut operations = 0usize;
        let session_storage = self.session.storage();

        for path in files {
            let metadata = fs::metadata(&path).await.map_err(|e| {
                BackupError::Internal(format!(
                    "Failed to read metadata for {}: {}",
                    path.display(),
                    e
                ))
            })?;

            if !metadata.is_file() {
                continue;
            }

            let modified = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);

            let relative = match path.strip_prefix(&self.local_pub_dir) {
                Ok(rel) => rel.to_path_buf(),
                Err(_) => continue,
            };

            let remote_path = Self::relative_path_to_remote(&relative)?;

            let needs_upload = match self.known_files.get(&relative) {
                Some(previous) => match modified.duration_since(*previous) {
                    Ok(duration) => duration > Duration::from_secs(0),
                    Err(_) => true,
                },
                None => true,
            };

            if needs_upload {
                let data = fs::read(&path).await.map_err(|e| {
                    BackupError::Internal(format!("Failed to read {}: {}", path.display(), e))
                })?;

                debug!("Uploading {} to /pub/{}", path.display(), remote_path);
                session_storage
                    .put(format!("/pub/{}", remote_path), data)
                    .await
                    .map_err(|e| {
                        BackupError::Internal(format!("Failed to upload {}: {}", remote_path, e))
                    })?;
                operations += 1;
            }

            new_snapshot.insert(relative, modified);
        }

        // Determine deletions
        for (relative, _) in self.known_files.iter() {
            if !new_snapshot.contains_key(relative) {
                let remote_path = Self::relative_path_to_remote(relative)?;
                debug!("Removing /pub/{} from homeserver", remote_path);
                match session_storage
                    .delete(format!("/pub/{}", remote_path))
                    .await
                {
                    Ok(_) => {
                        operations += 1;
                    }
                    Err(e) => {
                        warn!(
                            "Failed to delete {} from homeserver for {}: {}",
                            remote_path, self.pubky, e
                        );
                    }
                }
            }
        }

        self.known_files = new_snapshot;
        Ok(operations)
    }

    async fn collect_local_files(&self) -> Result<Vec<PathBuf>, BackupError> {
        let root = self.local_pub_dir.clone();
        tokio::task::spawn_blocking(move || {
            let mut files = Vec::new();
            for entry in WalkDir::new(root).into_iter().filter_map(Result::ok) {
                if entry.file_type().is_file() {
                    files.push(entry.into_path());
                }
            }
            Ok::<_, BackupError>(files)
        })
        .await
        .map_err(|e| BackupError::Internal(format!("Join error collecting files: {}", e)))??
    }

    fn relative_path_to_remote(path: &Path) -> Result<String, BackupError> {
        let mut parts = Vec::new();
        for component in path.components() {
            use std::path::Component;
            match component {
                Component::Normal(part) => parts.push(part.to_string_lossy().to_string()),
                Component::CurDir => continue,
                _ => {
                    return Err(BackupError::Internal(format!(
                        "Unsupported path component in {}",
                        path.display()
                    )))
                }
            }
        }
        Ok(parts.join("/"))
    }
}
