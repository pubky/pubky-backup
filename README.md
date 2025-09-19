# Pubky Backup App

A basic multi-platform desktop application which takes a `pubky` and downloads all of it's files to a local directory.


## Development

Run development server:

`cargo tauri dev`

For full logging add:

`RUST_LOG=debug`

### Development mode

Development mode is useful when working on the GUI: it skips network calls by auto-populating `AppState` and returning mock data from `fetch` calls.

**You do not need to enter a valid pubky in this mode. A default is preset.**

`cargo tauri dev -- -- --developer`

