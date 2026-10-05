<p align="center">
<img width="400" height="155" alt="pubky-backup" src="https://github.com/user-attachments/assets/8b32a453-d81b-4be7-b619-edb754298232" />
</p>

A lightweight, multi-platform desktop application which maintains a local copy of a `Pubky` User's published data. 

Select a [release build](https://github.com/pubky/pubky-backup/releases), enter your pubkeys and let the backups begin!


#### MacOS, Debian Linux and Windows 

Simply download the release build and run.

#### Non-debian Linux

Select the `.AppImage` release. After downloading, you'll need to make it executable before running:

```
chmod +x pubky-backup-*.AppImage
```


---

May the power ⚡ be with you. Powered by [pkarr](https://github.com/pubky/pkarr).

--- 


# Development

## Build

For executable build:

```
cargo tauri build
```

## Development mode (offline/no-network)

Development mode is useful when working on the GUI without a real homeserver. It skips network validation and returns empty data instead of making network calls.

Existing backed-up data on disk is still readable in this mode.

```
PUBKY_DEVELOPER_MODE=1 cargo tauri dev
```


## Testnet mode (local network)

Testnet mode runs the app against a Pubky network on your own machine, so you can exercise real syncing and signing in without mainnet or a phone. It needs Docker.

```bash
cd src-tauri

# 1. Start a local DHT, homeserver and relays. Prints the homeserver's key.
cargo run -p pubky-backup-core --example dev_testnet

# 2. Create a test user with some public and private data. Prints the user's pubky.
PUBKY_TESTNET=1 cargo run -p pubky-backup-core --example dev_signer -- signup <homeserver> user.key

# 3. Run the app against the local network and add that pubky
PUBKY_TESTNET=1 cargo tauri dev

# 4. Click "Sign in", copy the link, and approve it in place of Pubky Ring
PUBKY_TESTNET=1 cargo run -p pubky-backup-core --example dev_signer -- approve 'pubkyauth://...' user.key
```

## Testing

### Frontend Tests

Run frontend tests:

```
npm test
```


### Rust Unit Tests

Run Rust unit tests:

```
cargo test --lib
```

### Rust Integration Tests

Integration tests use a real `pubky-testnet` with an ephemeral homeserver and a PostgreSQL container started through Docker.

```bash
cargo test -p pubky-backup-core --test integration_tests
```

Note: Docker must be running. The first run pulls the PostgreSQL image.



# Release Github Workflow

Application Build and Github Release pipelines are triggered upon tag creation.

