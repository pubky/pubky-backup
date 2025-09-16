# Tauri + Vanilla JS

## Development

Run development server

`cargo tauri dev`


### Development mode

Development mode is useful when working on the GUI - it auto-populates `AppState` and skips network calls.

**You do not need to enter a valid pubky in this mode.**

`cargo tauri dev -- -- --developer`