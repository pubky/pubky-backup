# pubky-backup-core

Core library for backing up Pubky data to local storage. This crate handles synchronization of data from Pubky homeservers, storage management, and multi-key backup orchestration.

## Architecture

The library is organized into three main modules:

### `sync` - Event Synchronization

Handles the core backup synchronization logic for a single pubky key. This module is responsible for:

- **Event streaming**: Connecting to homeservers and receiving real-time events
- **Event processing**: Handling PUT/DELETE operations and updating local storage
- **Cursor management**: Tracking sync progress to enable resumable backups

### `orchestrator` - Multi-Key Management

Provides high-level management of multiple pubky backups running concurrently. This module handles:

- **Key lifecycle**: Adding, removing, and validating backup keys
- **Concurrency control**: Limiting parallel sync operations via semaphores
- **Status aggregation**: Broadcasting status updates for all managed keys
- **Error recovery**: Restarting failed backups with appropriate delays


### `storage` - Persistence Layer

Manages all file system operations for backup data and application state:

- **Application storage**: Global state like the list of backed-up keys
- **Per-key storage**: Individual backup data, cursors, and error logs
- **Migration**: Upgrading from legacy storage formats
- **Snapshots**: Point-in-time backup archives (zip)


## Usage

For most use cases, use `BackupManager` which handles everything:

```rust
use pubky_backup_core::{BackupManager, BackupManagerConfig};
use pubky::PublicKey;
use std::str::FromStr;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manager = BackupManager::new(BackupManagerConfig::default()).await?;

    let pubky = PublicKey::from_str("your_pubky_here")?;
    manager.add_key(pubky).await?;

    // Subscribe to status updates
    let mut rx = manager.subscribe();
    while let Ok(update) = rx.recv().await {
        println!("Key {:?} status: {:?}", update.pubky, update.state.status);
    }

    Ok(())
}
```

For lower-level control, use `BackupController` directly to manage a single key's sync process.

## Developer Mode

Enable developer mode by setting `PUBKY_DEVELOPER_MODE=1`. This uses mock data instead of real network calls, useful for testing and development.
