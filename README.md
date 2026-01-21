# Pubky Backup App

A basic multi-platform desktop application which takes a `pubky` and downloads all of it's files to a local directory.

The idea is for this to be a lightweight background process which continually keeps a Pubky User's local backup in-sync with it's published data. 

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


## Testing

Run frontend tests (watch mode):

```
npm test
```

Run frontend tests once:

```
npm run test:run
```

Run Rust tests:

```
cargo test
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
