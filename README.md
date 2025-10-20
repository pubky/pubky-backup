# Pubky Backup App

A basic multi-platform desktop application which signs in with a Pubky private key and keeps the homeserver in sync with a local folder.

Drop new or updated files into the app's data directory and they will be uploaded to the authenticated homeserver on the next sync cycle (or immediately via the "Upload now" button).

On startup the app prompts for your Pubky private key (hex encoded). The key is used only to establish a session; only the derived public key is persisted for convenience.

## Download and run

You can find pre-built packages on the [release page](https://github.com/pubky/pubky-backup/releases).

Once downloaded you'll need to set permissions for your OS to run these packages, Eg:

#### Linux

`chmod +x pubky-backup.AppImage`

#### MacOS

`xattr -dr com.apple.quarantine pubky-backup.app`


## Development

Run development server:

```
cargo tauri dev
```


### Development mode

Development mode is useful when working on the GUI: it skips network calls by auto-populating `AppState` and returning mock data from `fetch` calls.

**You do not need to enter a valid pubky in this mode. A default is preset.**

```
PUBKY_DEVELOPER_MODE=1 cargo tauri dev
```


## Build

For executable build:

```
cargo tauri build
```

## Bundle

We choose to bundle the following package formats for their portability and ease-of-use:  

- `AppImage` for Linux
- `app` for MacOS
- `msi` for Windowns

These are configured in `tauri.conf.json`.


# Release Github Workflow

Application Build and Github Release pipelines are triggered upon tag creation.


---

May the power ⚡ be with you. Powered by [pkarr](https://github.com/pubky/pkarr).
