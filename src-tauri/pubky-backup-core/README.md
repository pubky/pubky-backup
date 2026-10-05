# pubky-backup-core

Core library for backing up Pubky data to local storage. This crate handles synchronization of data from Pubky homeservers, storage management, and multi-key backup orchestration.

## Architecture

The library is organized into three main modules:

### `sync` - Event Synchronization

Handles the core backup synchronization logic for a single pubky key:

- **Event streaming**: Connecting to homeservers and receiving real-time events
- **Event processing**: Handling PUT/DELETE operations and updating local storage
- **Cursor management**: Tracking sync progress to enable resumable backups
- **Private data**: Syncing `/priv` as well as `/pub` for keys that are signed in

### `orchestrator` - Multi-Key Management

Provides high-level management of multiple pubky backups running concurrently:

- **Key lifecycle**: Adding, removing, and validating backup keys
- **Signing in**: Getting the owner's approval to back up a key's private data
- **Coordination**: Managing multiple backup controllers via message channels
- **Status aggregation**: Broadcasting status updates for all managed keys
- **Error recovery**: Restarting failed backups with appropriate delays

### `storage` - Persistence Layer

Manages all file system operations for backup data and application state:

- **Application storage**: Global state like the list of backed-up keys
- **Per-key storage**: Individual backup data, cursors, sessions, and error logs
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
    // Enable developer mode / offline mode (default: false)
    developer_mode: false,
    // Remaining options: sync_interval_secs, http_relay
    ..Default::default()
};
```

## BackupManager API

| Method | Description |
|--------|-------------|
| `new(config)` | Create a new manager, automatically resuming any stored keys |
| `testnet(config)` | Like `new`, but backs up from a local Pubky testnet instead of mainnet |
| `with_client(config, pubky_client)` | Like `new`, but with your own Pubky client |
| `add_key(pubky)` | Add a key to backup (validates and discovers homeserver) |
| `remove_key(pubky)` | Stop syncing but preserve data on disk |
| `delete_key(pubky)` | Stop syncing AND delete all backed-up data |
| `start_sign_in(pubky)` | Start signing in to a key, to back up its private data too |
| `cancel_sign_in(pubky)` | Cancel a sign-in that is still awaiting approval |
| `sign_out(pubky)` | Sign out of a key, going back to backing up public data only |
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


## Private Data (Signing In)

Anyone can read a key's public data (`/pub`), so that is all a backup covers by default. A key's private data (`/priv`) is only readable by its owner. To back it up as well, sign in to the key:

```rust
let request = manager.start_sign_in(&pubky).await?;

// Show this link (or a QR code of it) to the key's owner.
// They open it with their signer app, e.g. Pubky Ring, and approve.
println!("Approve in Pubky Ring: {}", request.authorization_url());

request.approved().await?;
// Private data is now backed up alongside public data.
```

The request only asks for read access to `/priv`, and the key's secret never leaves the signer app. The session is stored with the key's backup, so the key stays signed in across restarts.

Instead of awaiting `approved()`, you can drop the request and follow `KeyState::auth` in the status updates:

| `AuthStatus` | Meaning | Backed up |
|--------------|---------|-----------|
| `SignedOut` | Not signed in | `/pub` |
| `AwaitingApproval { authorization_url }` | Sign-in started, waiting for the owner | `/pub` |
| `SignedIn` | Signed in | `/pub` and `/priv` |
| `SessionExpired` | The homeserver no longer accepts the session; sign in again | `/pub` |
| `SignInFailed { message }` | The last sign-in attempt failed | `/pub` |

Losing the session never interrupts the backup of public data. `sign_out(pubky)` ends the session; private data that was already backed up stays on disk. Removing or deleting a key also signs it out.

Backed-up private data is stored unencrypted under `keys/<pubky>/data/priv/` and is included in snapshots, so protect the backup directory accordingly.

### ControllerCommand

| Command | Description |
|---------|-------------|
| `ForceSync` | Trigger an immediate sync (bypasses the interval timer) |
| `Cancel` | Stop the backup controller gracefully |

## Testnet Mode and Dev Tools

To develop against real homeserver behaviour without touching mainnet, run everything on a local Pubky testnet. Two examples help with that:

```bash
# A local DHT, homeserver and relays on fixed ports (needs Docker)
cargo run -p pubky-backup-core --example dev_testnet

# A stand-in for Pubky Ring: create a test user, then approve sign-in links as that user
PUBKY_TESTNET=1 cargo run -p pubky-backup-core --example dev_signer -- signup <homeserver> user.key
PUBKY_TESTNET=1 cargo run -p pubky-backup-core --example dev_signer -- approve 'pubkyauth://...' user.key
```

Create the manager with `BackupManager::testnet(config)` to back up from that network. `is_testnet_mode()` reports whether `PUBKY_TESTNET` is set, for applications that want to switch on it.

## Developer Mode (Offline/No-Network)

Enable developer mode to skip network validation and work offline:

```bash
PUBKY_DEVELOPER_MODE=1 cargo run
```

In this mode, any valid pubky key is accepted without homeserver validation, and sync operations return empty data (no network calls). Existing backed-up data on disk remains readable. This is useful for GUI development and testing without needing a real homeserver.
