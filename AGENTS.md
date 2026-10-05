# pubky-backup

Desktop app that keeps a local copy of a Pubky user's published data. A Rust backend does the backing up; a Tauri + React GUI sits on top of it.

## The rule that matters most: backend and frontend stay separate

The backend is designed to be useful on its own, without the GUI. Someone should be able to build a CLI or a daemon on `pubky-backup-core` and get every backup feature. Keep the boundary between the layers clean, and treat it as an API rather than an implementation detail.

There are three layers, and each one only talks to the one below it:

| Layer | Path | Role |
|-------|------|------|
| Core | `src-tauri/pubky-backup-core/` | Standalone library crate. All backup behaviour lives here: sync, storage, multi-key orchestration, snapshots, activity log, config. |
| Tauri shell | `src-tauri/src/` | Thin adapter. Exposes core as Tauri commands (`lib.rs`), maps errors to `BackupAppError` (`error.rs`), forwards status updates as the `key-update` event, and owns the tray and window. |
| Frontend | `src/` | React UI. Renders the state the backend reports and sends user intents back. |

What this means when you change code:

- **Core knows nothing about the GUI.** No `tauri` dependency in `pubky-backup-core`, and no types, naming or behaviour that only make sense for this particular UI. Its public API is `BackupManager` plus the re-exports in `pubky-backup-core/src/lib.rs`.
- **Backup behaviour goes in core, not in the shell.** A Tauri command should parse its arguments, call into `BackupManager`, and map the error. If a command needs to make a decision about backup state, that decision belongs in a `BackupManager` method so a non-GUI consumer gets it too. Ask: would a CLI user need this? If yes, it is core.
- **The frontend holds no backup logic.** It does not compute sync state, validate against homeservers, or touch the data directory. It displays `KeyState` and calls commands.
- **Backend calls go through one place.** The frontend invokes commands only via the typed wrappers in `src/services/tauri-commands.ts`, and receives pushed state only via the `key-update` event (`src/hooks/useKeyUpdates`). Do not call `invoke` from components or other hooks. Direct `@tauri-apps/*` use elsewhere is for window and dialog concerns only.
- **Only `BackupAppError` crosses to the frontend.** Core has its own error types (`OrchestratorError`, `SyncError`, `StorageError`); convert them in `src-tauri/src/error.rs`.

### Changing the API

Types are mirrored by hand, with no codegen, so a change to the boundary touches every layer. Update them together:

1. Core: the `BackupManager` method or type, the re-export in `lib.rs` if it is new, and the API table and examples in `pubky-backup-core/README.md`.
2. Tauri shell: the `#[tauri::command]` in `src-tauri/src/lib.rs`, and its entry in `generate_handler!`.
3. Frontend: the wrapper in `src/services/tauri-commands.ts` (exported from `src/services/index.ts`), and the mirrored TypeScript types: `src/stores/uiStore/uiStore.types.ts` for `KeyState`, `KeyStatus`, `AuthStatus`, `KeyError`, `ActivityEntry`, and `src/types/backend-errors.ts` for `BackupAppError`.

Serialized field names are snake_case on both sides (`data_size`, `last_sync`), and enums with data are tagged with `type`.

## Commands

Run `cargo` from `src-tauri/` (the workspace root for both crates). Run `npm` from the repo root.

```bash
# Run the app; PUBKY_DEVELOPER_MODE=1 skips the network and accepts any valid pubky
PUBKY_DEVELOPER_MODE=1 cargo tauri dev

# Run the app against a local Pubky network, for real syncing and sign-in without mainnet
# (start the network and a test user first: see "Testnet mode" in README.md)
PUBKY_TESTNET=1 cargo tauri dev

# Rust checks, as CI runs them
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --workspace --exclude pubky-backup-core
cargo test -p pubky-backup-core --lib
cargo test -p pubky-backup-core --test integration_tests -- --test-threads=1

# Frontend checks, as CI runs them
npm run format:check
npm run type-check
npm run test:run
```

The integration tests start a real `pubky-testnet` with a Postgres container, so Docker must be running. They exercise core directly, with no Tauri involved, which is also the quickest way to check that a feature works without the GUI.

## Frontend layout

`src/components/` follows atomic design (`atoms`, `molecules`, `organisms`), each component in its own folder with an `index.ts` and a colocated `*.test.tsx`. One hook per folder in `src/hooks/`. Global UI state is a single Zustand store in `src/stores/uiStore`. Tests use Vitest with happy-dom.
