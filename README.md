# Pubky Backup App

A basic multi-platform desktop application which takes a `pubky` and downloads all of it's files to a local directory.


## Development

Run development server:

```
cargo tauri dev
```

### Development mode

Development mode is useful when working on the GUI: it skips network calls by auto-populating `AppState` and returning mock data from `fetch` calls.

**You do not need to enter a valid pubky in this mode. A default is preset.**

```
cargo tauri dev -- -- --developer
```


## Build

For executable build:

```
cd src && npm run build
cargo tauri bundle --bundles deb,app,dmg
```

### Deb

Install:
```
sudo dpkg -i */pubky-backup/src-tauri/target/release/bundle/deb/pubky-backup_0.1.0_amd64.deb
```

Run: 
```
pubky-backup
```


