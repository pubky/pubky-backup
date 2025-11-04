# Pubky Backup Core

A Rust library providing the core backup and synchronization logic for the Pubky Backup application. This library handles bidirectional sync between a Pubky homeserver and local filesystem storage.

## Features

### One-Way Sync (Download)
The one-way sync continuously polls the homeserver for new events and fetches resources to store them locally. It tracks sync progress with persistent cursors, ensuring reliable resumption after interruptions. Errors are logged without stopping the sync process. 
### Two-Way Sync (Upload)
The two-way sync is the same as One-Way with added monitoring of the local directory for changes using the `notify` crate. File changes are automatically pushed to the homeserver. 


## Usage

### One-Way Sync (Read-Only)

```rust
use pubky_backup_core::{AppStorage, BackupController};
use pubky::PublicKey;
use std::sync::Arc;
use std::str::FromStr;
use tokio::sync::broadcast;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize storage
    let storage = Arc::new(AppStorage::new()?);

    // Parse the pubky to backup
    let pubky = PublicKey::from_str("your_pubky_here")?;

    // Create channels for control and status
    let (control_tx, control_rx) = broadcast::channel(5);
    let (status_tx, mut status_rx) = broadcast::channel(5);

    // Create controller for one-way sync (read-only)
    let controller = BackupController::new(
        pubky,
        storage,
        Some(control_rx),
        Some(status_tx),
    );

    // Spawn the controller
    tokio::spawn(controller.run());

    // Listen for status updates
    tokio::spawn(async move {
        while let Ok(status) = status_rx.recv().await {
            println!("Status: {:?}", status);
        }
    });

    Ok(())
}
```

### Two-Way Sync (Read-Write)

```rust
use pubky_backup_core::{AppStorage, BackupController};
use pubky::PublicKey;
use std::sync::Arc;
use std::str::FromStr;
use tokio::sync::broadcast;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let storage = Arc::new(AppStorage::new()?);
    let pubky = PublicKey::from_str("your_pubky_here")?;

    // Your secret key as hex string (64 characters = 32 bytes)
    let secret_key_hex = "a1b2c3d4...";

    let (control_tx, control_rx) = broadcast::channel(5);
    let (status_tx, status_rx) = broadcast::channel(5);

    // Create controller for two-way sync
    let controller = BackupController::new(
        pubky,
        storage,
        Some(control_rx),
        Some(status_tx),
    );

    // Run with two-way sync enabled (secret key passed here, then discarded)
    tokio::spawn(controller.run_two_way(secret_key_hex.to_string()));

    Ok(())
}
```

## API

### BackupController

The main controller that manages sync operations.

```rust
pub struct BackupController {
    // ...
}

impl BackupController {
    pub fn new(
        pubky: PublicKey,
        storage: Arc<AppStorage>,
        control_rx: Option<broadcast::Receiver<BackupControllerMessage>>,
        status_tx: Option<broadcast::Sender<BackupControllerStatus>>,
        secret_key_hex: Option<String>, // For two-way sync
    ) -> Self;

    pub async fn run(self);
}
```

### Control Messages

```rust
pub enum BackupControllerMessage {
    Cancel,      // Stop the controller
    ForceSync,   // Trigger immediate sync
}
```

### Status Updates

```rust
pub enum BackupControllerStatus {
    Syncing { events_processed: usize },
    Idle,
    Ended,
    Error { message: String },
}
```


## Development

### Build

```bash
cargo build
```


### Developer Mode

Enable developer mode to use mock data instead of real network calls:

```bash
export PUBKY_DEVELOPER_MODE=1
```

In developer mode:
- Network calls are mocked
- A default test pubky is used
- Two-way sync is automatically disabled
- Mock events and data are returned

## Error Handling

The library includes comprehensive error handling:

```rust
pub enum BackupError {
    Storage(StorageError),
    Events(EventsError),
    Internal(String),
    Authentication(String),  // Two-way sync authentication errors
    FileWatcher(String),     // File system watcher errors
    Upload(String),          // Upload operation errors
    Delete(String),          // Delete operation errors
}
```

### Error Categories

**Non-Critical Errors** (logged to `~/.pubky-backup/error.log`, controller continues):
- Upload failures (two-way sync)
- Delete failures (two-way sync)
- Invalid events from homeserver
- Individual resource fetch failures

**Critical Errors** (emitted via status channel, controller terminates):
- Authentication/initialization failure
- Critical sync batch failures (e.g., complete network failure)

Non-critical errors are logged with the `error!()` macro and written to the error log file, allowing the sync process to continue for other files. Critical errors send a `BackupControllerStatus::Error` message and stop the controller.


## Two-Way Sync

Two-way sync is **optional** - enable it by passing a secret key hex string to `BackupController::new()`.

### How It Works

The file system watcher monitors your backup directory using the `notify` crate, with a debouncer that batches rapid changes (500ms) to prevent excessive uploads. Local filesystem paths are mapped to PubkyResource URLs. All uploads and deletes run in the background, in parallel with the download sync loop.


### Limitations

- No special handling for large files
- File watcher tests may be timing-sensitive in CI environments



## License

See the main [LICENSE](../../LICENSE) file in the repository root.

---

Part of the [Pubky Backup](https://github.com/pubky/pubky-backup) project.
Powered by [pkarr](https://github.com/pubky/pkarr) and [Pubky](https://github.com/pubky/pubky-core).
