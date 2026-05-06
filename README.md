<p align="center">
<img width="400" height="155" alt="pubky-backup" src="https://github.com/user-attachments/assets/8b32a453-d81b-4be7-b619-edb754298232" />
</p>

A lightweight, multi-platform desktop application which maintains a local copy of a `Pubky` User's published data. 

Select a [release build](https://github.com/pubky/pubky-backup/releases), enter your pubkeys and let the backups begin!

```
chmod +x pubky-backup-*.AppImage
```

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

Integration tests use a real `pubky-testnet` with an ephemeral homeserver and embedded PostgreSQL.

```bash
cargo test -p pubky-backup-core --test integration_tests
```

Note: The first run will download PostgreSQL binaries (~50-100MB), which are cached for subsequent runs.



# Release Github Workflow

Application Build and Github Release pipelines are triggered upon tag creation.

