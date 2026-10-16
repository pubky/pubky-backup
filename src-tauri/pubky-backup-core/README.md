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
- **Mirroring**: Copying what is backed up to a second homeserver

### `orchestrator` - Multi-Key Management

Provides high-level management of multiple pubky backups running concurrently:

- **Key lifecycle**: Adding, removing, and validating backup keys
- **Signing in**: Getting the owner's approval to back up a key's private data
- **Mirrors**: Getting the owner's approval to copy a key's data to a second homeserver
- **Coordination**: Managing multiple backup controllers via message channels
- **Status aggregation**: Broadcasting status updates for all managed keys
- **Error recovery**: Restarting failed backups with appropriate delays

### `storage` - Persistence Layer

Manages all file system operations for backup data and application state:

- **Application storage**: Global state like the list of backed-up keys
- **Per-key storage**: Individual backup data, cursors, and error logs
- **Sessions and mirrors**: Session secrets of signed-in keys, and the mirrors of keys with the sessions on them, kept in the app data directory rather than with the backup
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
use pubky::ClientId;
use pubky_backup_core::BackupManagerConfig;
use std::path::PathBuf;

let config = BackupManagerConfig {
    // Data directory path (default: ~/.pubky-backup)
    data_dir: Some(PathBuf::from("/custom/backup/path")),
    // Timeout for key validation/homeserver discovery in seconds (default: 30)
    validation_timeout_secs: 30,
    // Make no network calls, keeping only the data already on disk (default: false)
    offline: false,
    // Name shown to users among the apps they granted access to (default: "pubky-backup")
    client_id: ClientId::new("my-backup-cli")?,
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
| `ensure_backed_up(pubky)` | Add a key unless it is backed up already, or retry it if its backup is in error |
| `remove_key(pubky)` | Stop syncing but preserve data on disk |
| `delete_key(pubky)` | Stop syncing AND delete all backed-up data |
| `start_sign_in(pubky)` | Start signing in to a key, to back up its private data too |
| `cancel_sign_in(pubky)` | Cancel a sign-in that is still awaiting approval |
| `start_sign_in_for_new_key()` | Start a sign-in that backs up, and signs in, whichever key approves it |
| `cancel_sign_in_for_new_key()` | Cancel that sign-in if it is still awaiting approval |
| `sign_out(pubky)` | Sign out of a key, going back to backing up public data only |
| `start_mirror(pubky, homeserver)` | Start copying a key's backup to a second homeserver |
| `cancel_mirror(pubky)` | Cancel a mirror request that is still awaiting approval |
| `stop_mirror(pubky)` | Stop mirroring a key; what was copied stays on the mirror |
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
                println!("Error: {}", err.message);
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

The request only asks for read access to `/priv`, and the key's secret never leaves the signer app. If the owner approves without that access, the sign-in fails.

The approval is a grant to this app, named by `BackupManagerConfig::client_id`. The backup renews its short-lived access tokens itself, without the signer app, for as long as the grant lasts (the SDK's signer issues grants for two years). When it expires, or the owner revokes it from their signer app, the key's status becomes `SessionExpired` and the owner signs in again. The session is stored in the app data directory (`~/.pubky-backup/sessions/`, readable by its owner only), so the key stays signed in across restarts. It stays there when the backup location moves, so it is never copied into a synced or removable folder along with the backup.

The owner doesn't have to enter their pubky first. A sign-in for a new key adds whichever key approves it, if it isn't backed up yet, and signs it in:

```rust
let request = manager.start_sign_in_for_new_key().await?;
println!("Approve in Pubky Ring: {}", request.authorization_url());

let pubky = request.approved().await?;
// `pubky` is now backed up, public and private data alike.
```

Only one such sign-in is pending at a time, and until it is approved there is no key to report on, so follow it through `approved()`. `cancel_sign_in_for_new_key()` cancels it.

Instead of awaiting `approved()` for a key's own sign-in, you can drop the request and follow `KeyState::auth` in the status updates:

| `AuthStatus` | Meaning | Backed up |
|--------------|---------|-----------|
| `SignedOut` | Not signed in | `/pub` |
| `AwaitingApproval { authorization_url }` | Sign-in started, waiting for the owner | `/pub` |
| `SignedIn` | Signed in | `/pub` and `/priv` |
| `SessionExpired` | The homeserver no longer accepts the session; sign in again | `/pub` |
| `SignInFailed { message }` | The last sign-in attempt failed | `/pub` |

Losing the session never interrupts the backup of public data. `sign_out(pubky)` ends the session; private data that was already backed up stays on disk. Removing or deleting a key also signs it out.

Backed-up private data is stored unencrypted under `keys/<pubky>/data/priv/` and is included in snapshots, so protect the backup directory accordingly.

## Mirroring

A mirror is a second homeserver that holds the same data as a key's own. Mirroring copies the key's backup there after every sync, so the mirror follows along with every change, deletions included:

```rust
let request = manager.start_mirror(&pubky, &mirror_homeserver).await?;

// Show this link (or a QR code of it) to the key's owner, as for signing in.
println!("Approve in Pubky Ring: {}", request.authorization_url());

request.approved().await?;
// Everything backed up so far is copied to the mirror, then every later change.
```

The key must already have an account on the mirror, for example one created through the mirror's own website. The request asks for write access to `/pub` and `/priv`. The grant the owner approves is not tied to a homeserver, so the backup uses it to sign in to the key's account on the mirror and copies there directly; the session the approval opens on the key's own homeserver is ended right away, since the backup never writes there. The key's own homeserver in PKDNS stays the same, so mirroring never moves the key's identity. Private data reaches the mirror only while the key is signed in, because that is when it is backed up.

Follow `KeyState::mirror` in the status updates:

| `MirrorStatus` | Meaning |
|----------------|---------|
| `NotMirrored` | The key has no mirror |
| `AwaitingApproval { homeserver, authorization_url }` | Mirror requested, waiting for the owner |
| `Mirroring { homeserver }` | Changes are copied after every sync |
| `SessionExpired { homeserver }` | The mirror no longer accepts the session; `start_mirror` again to resume where it stopped |
| `Failing { homeserver, message }` | Copying fails (e.g. the mirror is unreachable or full); retried on every sync |
| `SetupFailed { homeserver, message }` | The last mirror request failed |

Problems with the mirror never hold up the backup itself. The mirror pass copies in batches of 50 events after the backup's own passes, and never gets ahead of what is backed up locally, which is where the content comes from. Its progress is kept in `state/mirror_cursor` and `state/mirror_private_cursor` next to the backup's cursors, while the mirror itself and the session on it are stored with the sign-in sessions in the app data directory (`~/.pubky-backup/mirrors/`).

Mirroring stops on its own if the mirror becomes the key's own homeserver, since every copy would then be a new change to copy.

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

The local testnet started by `dev_testnet` includes a second homeserver to mirror to.

## Offline Mode

Set `BackupManagerConfig::offline` to make no network calls. Any valid pubky is then accepted without homeserver validation, syncs find no events, and signing in is unavailable. Existing backed-up data on disk remains readable. The crate's own unit tests run this way; for anything that should behave like a real homeserver, use the testnet instead.
