use crate::error::SyncError;
use crate::storage::AppStorage;
use log::{debug, error, info, warn};
use notify_debouncer_full::{new_debouncer, notify::*, DebounceEventResult};
use pubky::{PubkyResource, PublicKey};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;

const DEBOUNCE_DURATION: Duration = Duration::from_millis(500);

/// Filesystem events that should be synced to homeserver
#[derive(Debug, Clone)]
pub enum FileSystemEvent {
    CreateOrModify {
        resource: PubkyResource,
        path: PathBuf,
    },
    Delete {
        resource: PubkyResource,
    },
}

/// File system watcher that monitors a pubky's backup directory for changes
pub struct FileSystemWatcher {
    pubky: PublicKey,
    storage: Arc<AppStorage>,
    watch_path: PathBuf,
}

impl FileSystemWatcher {
    /// Create a new filesystem watcher for a pubky's backup directory
    pub fn new(pubky: PublicKey, storage: Arc<AppStorage>) -> std::result::Result<Self, SyncError> {
        let watch_path = storage
            .get_pubky_directory_path(&pubky)
            .map_err(|e| SyncError::Storage(e))?;

        Ok(Self {
            pubky,
            storage,
            watch_path,
        })
    }

    /// Start watching the filesystem and return a channel receiver for events
    pub fn start(self) -> std::result::Result<mpsc::UnboundedReceiver<FileSystemEvent>, SyncError> {
        let (tx, rx) = mpsc::unbounded_channel();

        let watch_path = self.watch_path.clone();
        let pubky = self.pubky.clone();
        let storage = self.storage.clone();

        // Spawn a blocking task for the file watcher
        std::thread::spawn(move || {
            let tx_clone = tx.clone();
            let watch_path_clone = watch_path.clone();
            let pubky_clone = pubky.clone();
            let storage_clone = storage.clone();

            let mut debouncer = match new_debouncer(
                DEBOUNCE_DURATION,
                None,
                move |result: DebounceEventResult| {
                    // Process events synchronously in the watcher thread
                    let rt = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                        .expect("Failed to create event handler runtime");

                    rt.block_on(async {
                        Self::handle_debounced_events(
                            result,
                            &tx_clone,
                            &watch_path_clone,
                            &pubky_clone,
                            &storage_clone,
                        )
                        .await
                    });
                },
            ) {
                Ok(d) => d,
                Err(e) => {
                    error!("Failed to create file watcher: {}", e);
                    return;
                }
            };

            // Watch the directory recursively
            if let Err(e) = debouncer.watch(&watch_path, RecursiveMode::Recursive) {
                error!("Failed to watch directory {}: {}", watch_path.display(), e);
                return;
            }
            info!("File system watcher started for {}", watch_path.display());

            // Keep the watcher alive - it will be dropped when the thread exits
            loop {
                std::thread::sleep(Duration::from_secs(1));
            }
        });

        Ok(rx)
    }

    /// Handle debounced filesystem events
    async fn handle_debounced_events(
        result: DebounceEventResult,
        tx: &mpsc::UnboundedSender<FileSystemEvent>,
        watch_path: &Path,
        pubky: &PublicKey,
        storage: &Arc<AppStorage>,
    ) {
        match result {
            Ok(events) => {
                for event in events {
                    for path in &event.paths {
                        if let Err(e) =
                            Self::process_path(path, &event.kind, tx, watch_path, pubky, storage)
                                .await
                        {
                            warn!("Error processing path {}: {}", path.display(), e);
                        }
                    }
                }
            }
            Err(errors) => {
                for error in errors {
                    error!("File watcher error: {:?}", error);
                }
            }
        }
    }

    /// Process a single filesystem path change
    async fn process_path(
        path: &Path,
        kind: &EventKind,
        tx: &mpsc::UnboundedSender<FileSystemEvent>,
        watch_path: &Path,
        pubky: &PublicKey,
        storage: &Arc<AppStorage>,
    ) -> std::result::Result<(), SyncError> {
        // Skip directories
        if path.is_dir() {
            return Ok(());
        }
        if Self::is_internal_file(path) {
            return Ok(());
        }

        let resource = storage
            .path_to_resource(watch_path, path, pubky)
            .map_err(|e| SyncError::Storage(e))?;

        match kind {
            EventKind::Create(_) | EventKind::Modify(_) => {
                debug!("File created/modified: {} -> {}", path.display(), resource);
                if let Err(e) = tx.send(FileSystemEvent::CreateOrModify {
                    resource,
                    path: path.to_path_buf(),
                }) {
                    error!("Failed to send filesystem event: {}", e);
                }
            }
            EventKind::Remove(_) => {
                debug!("File removed: {} -> {}", path.display(), resource);
                if let Err(e) = tx.send(FileSystemEvent::Delete { resource }) {
                    error!("Failed to send filesystem event: {}", e);
                }
            }
            _ => {
                // Ignore other event types (access, metadata changes, etc.)
            }
        }

        Ok(())
    }

    /// Check if a file is an internal file that shouldn't be synced
    /// TODO: Fully separate internal files from user data
    fn is_internal_file(path: &Path) -> bool {
        if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
            // Filter out cursor, error logs, and other internal files
            matches!(
                file_name,
                "cursor" | "error.log" | ".DS_Store" | "Thumbs.db"
            )
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;
    use tempfile::TempDir;

    const TEST_PUBKY: &str = "g1b6wp8bhhxtsksy3td7rj6mgg7s5k8c68663sajkfscshwj8g5y";

    #[test]
    fn test_is_internal_file() {
        assert!(FileSystemWatcher::is_internal_file(Path::new(
            "/path/to/cursor"
        )));
        assert!(FileSystemWatcher::is_internal_file(Path::new(
            "/path/to/error.log"
        )));
        assert!(FileSystemWatcher::is_internal_file(Path::new(
            "/path/to/.DS_Store"
        )));
        assert!(!FileSystemWatcher::is_internal_file(Path::new(
            "/path/to/data.json"
        )));
    }

    #[tokio::test]
    async fn test_watcher_creation() {
        let temp_dir = TempDir::new().unwrap();
        let pubky = PublicKey::from_str(TEST_PUBKY).unwrap();
        let storage =
            Arc::new(AppStorage::new_with_single_path(&temp_dir.path().to_path_buf()).unwrap());

        // Create the pubky directory
        let pubky_dir = temp_dir.path().join(pubky.to_string());
        std::fs::create_dir_all(&pubky_dir).unwrap();

        let watcher = FileSystemWatcher::new(pubky, storage);
        assert!(watcher.is_ok());
    }
}
