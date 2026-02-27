# pubky-backup-core

Core library for backing up Pubky data to local storage. This crate handles synchronization of data from Pubky homeservers, storage management, and multi-key backup orchestration.

## Architecture

The library is organized into three main modules:

### `sync` - Event Synchronization

Handles the core backup synchronization logic for a single pubky key:

- **Event streaming**: Connecting to homeservers and receiving real-time events
- **Event processing**: Handling PUT/DELETE operations and updating local storage
- **Cursor management**: Tracking sync progress to enable resumable backups

### `orchestrator` - Multi-Key Management

Provides high-level management of multiple pubky backups running concurrently:

- **Key lifecycle**: Adding, removing, and validating backup keys
- **Coordination**: Managing multiple backup controllers via message channels
- **Status aggregation**: Broadcasting status updates for all managed keys
- **Error recovery**: Restarting failed backups with appropriate delays

### `storage` - Persistence Layer

Manages all file system operations for backup data and application state:

- **Application storage**: Global state like the list of backed-up keys
- **Per-key storage**: Individual backup data, cursors, and error logs
- **Migration**: Upgrading from legacy storage formats
- **Snapshots**: Point-in-time backup archives (zip)

## Quick Start

For most use cases, use `BackupManager` which handles everything:

```rust
use pubky_backup_core::{BackupManager, BackupManagerConfig};
use pubky::PublicKey;
use std::str::FromStr;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create manager with default configuration
    let manager = BackupManager::new(BackupManagerConfig::default()).await?;

    // Add a key to backup
    let pubky = PublicKey::from_str("your_pubky_here")?;
    manager.add_key(pubky.clone()).await?;

    // Subscribe to status updates
    let mut rx = manager.subscribe();
    while let Ok(update) = rx.recv().await {
        println!("Key {:?} status: {:?}", update.pubky, update.state.status);
    }

    Ok(())
}
```

## Configuration

```rust
use pubky_backup_core::BackupManagerConfig;
use std::path::PathBuf;

let config = BackupManagerConfig {
    // Data directory path (default: ~/.pubky-backup)
    data_dir: Some(PathBuf::from("/custom/backup/path")),
    // Timeout for key validation/homeserver discovery in seconds (default: 30)
    validation_timeout_secs: 30,
    // Enable developer mode with mock data (default: false)
    developer_mode: false,
};
```

## BackupManager API

| Method | Description |
|--------|-------------|
| `new(config)` | Create a new manager, automatically resuming any stored keys |
| `add_key(pubky)` | Add a key to backup (validates and discovers homeserver) |
| `remove_key(pubky)` | Stop syncing but preserve data on disk |
| `delete_key(pubky)` | Stop syncing AND delete all backed-up data |
| `force_sync(pubky)` | Trigger immediate sync for a specific key |
| `get_key_state(pubky)` | Get current state of a specific key |
| `get_keys()` | List all managed pubkys |
| `subscribe()` | Subscribe to status updates for all keys |
| `create_snapshot(pubky)` | Create a zip snapshot of a key's data |
| `data_dir()` | Get the data directory path |
| `shutdown()` | Gracefully stop all backups |

## Status Updates

Subscribe to real-time status updates via `manager.subscribe()`:

```rust
use pubky_backup_core::KeyStatus;

let mut rx = manager.subscribe();
while let Ok(update) = rx.recv().await {
    match update.state.status {
        KeyStatus::Starting => println!("Validating key..."),
        KeyStatus::Syncing { events_processed } => {
            println!("Syncing: {} events processed", events_processed);
        }
        KeyStatus::Idle => println!("Up to date, waiting for next sync"),
        KeyStatus::Error => {
            if let Some(err) = &update.state.error {
                println!("Error: {} (recoverable: {})", err.message, err.recoverable);
            }
        }
        KeyStatus::Stopped => println!("Backup stopped"),
    }
}
```


### ControllerCommand

| Command | Description |
|---------|-------------|
| `ForceSync` | Trigger an immediate sync (bypasses the interval timer) |
| `Cancel` | Stop the backup controller gracefully |

## Developer Mode

Enable developer mode to use mock data instead of real network calls:

```bash
PUBKY_DEVELOPER_MODE=1 cargo run
```

This is useful for testing and development without needing a real homeserver.
