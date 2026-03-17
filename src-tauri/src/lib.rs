mod error;

use arc_swap::ArcSwap;
use log::{debug, error, info, warn};
use pubky::PublicKey;
use serde::Serialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};

use crate::error::BackupAppError;
use pubky_backup_core::{
    is_developer_mode, parse_pubky, AppStorage, BackupManager, BackupManagerConfig, KeyState,
    KeyStatus,
};
use tauri::{
    menu::{MenuBuilder, MenuItemBuilder},
    tray::TrayIconBuilder,
    AppHandle, Emitter, Manager, WindowEvent,
};
use tokio::sync::Mutex;

/// Global manager instance — `ArcSwap` for lock-free reads, swappable for relocation.
static MANAGER: OnceLock<ArcSwap<BackupManager>> = OnceLock::new();
/// Serialises manager initialisation and swap operations (relocation).
static MANAGER_WRITE: OnceLock<Mutex<()>> = OnceLock::new();
/// Handle to the current event listener task, so we can abort it on relocation.
static EVENT_LISTENER: OnceLock<Mutex<Option<tauri::async_runtime::JoinHandle<()>>>> =
    OnceLock::new();
/// Tauri app handle for tray updates and event emission
static APP_HANDLE: OnceLock<AppHandle> = OnceLock::new();
/// True while `set_backup_location` is swapping the manager. Other commands
/// that call `load_manager()` will get a clear error instead of silently
/// operating on a stale (shutdown) manager instance.
static RELOCATING: AtomicBool = AtomicBool::new(false);

fn manager_write_mutex() -> &'static Mutex<()> {
    MANAGER_WRITE.get_or_init(|| Mutex::new(()))
}

fn event_listener_mutex() -> &'static Mutex<Option<tauri::async_runtime::JoinHandle<()>>> {
    EVENT_LISTENER.get_or_init(|| Mutex::new(None))
}

/// Config response for frontend
#[derive(Clone, Serialize)]
pub struct AppConfig {
    developer_mode: bool,
    sync_interval_secs: u64,
    keys_dir: String,
}

fn create_config() -> BackupManagerConfig {
    BackupManagerConfig {
        developer_mode: is_developer_mode(),
        ..Default::default()
    }
}

/// Ensure a BackupManager exists inside the global slot.
///
/// The write-mutex serialises creation so only one caller builds the
/// manager; every other caller sees the `OnceLock` already set and
/// returns immediately.
async fn ensure_manager() -> Result<(), BackupAppError> {
    // Fast path — already initialised (lock-free).
    if MANAGER.get().is_some() {
        return Ok(());
    }

    // Slow path — hold the write-mutex while creating.
    let _guard = manager_write_mutex().lock().await;
    // Double-check after acquiring lock.
    if MANAGER.get().is_some() {
        return Ok(());
    }

    let manager = BackupManager::new(create_config())
        .await
        .map_err(BackupAppError::internal)?;

    spawn_event_listener(&manager).await;
    let _ = MANAGER.set(ArcSwap::from_pointee(manager));
    Ok(())
}

/// Get a cheap Arc handle to the current manager.
///
/// Returns an error if the manager is being relocated (backup location move
/// in progress) to prevent commands from operating on the stale instance.
fn load_manager() -> Result<Arc<BackupManager>, BackupAppError> {
    if RELOCATING.load(Ordering::Acquire) {
        return Err(BackupAppError::internal(
            "Backup location move in progress, please wait",
        ));
    }
    MANAGER
        .get()
        .map(|slot| slot.load_full())
        .ok_or_else(|| BackupAppError::internal("BackupManager not initialized"))
}

/// Ensure the manager exists and return a handle to it.
///
/// Combines the common `ensure_manager().await?; load_manager()?` pattern.
async fn get_manager() -> Result<Arc<BackupManager>, BackupAppError> {
    ensure_manager().await?;
    load_manager()
}

/// Parse a pubky z32 string, mapping errors to [`BackupAppError::InvalidPubkyFormat`].
fn parse_pubky_for_command(pubky_str: &str) -> Result<PublicKey, BackupAppError> {
    parse_pubky(pubky_str).map_err(|msg| BackupAppError::InvalidPubkyFormat { message: msg })
}

async fn spawn_event_listener(manager: &BackupManager) {
    let mut rx = manager.subscribe();
    let Some(app_handle) = APP_HANDLE.get().cloned() else {
        error!("Cannot spawn event listener — APP_HANDLE not set");
        return;
    };

    let mut guard = event_listener_mutex().lock().await;

    // Abort the previous listener if one exists.
    if let Some(prev) = guard.take() {
        debug!("Aborting previous event listener");
        prev.abort();
    }

    debug!("Event listener spawned for BackupManager");
    let handle = tauri::async_runtime::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(update) => {
                    debug!("Emitting key-update to frontend: {}", update.pubky);
                    if let Err(e) = app_handle.emit("key-update", &update) {
                        error!("Failed to emit key-update event: {}", e);
                    }
                    update_tray_icon_aggregate();
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    warn!("Event listener lagged, skipped {} messages", n);
                    continue;
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    debug!("Event listener channel closed");
                    break;
                }
            }
        }
    });
    *guard = Some(handle);
}

/// Add a key to start backing up.
#[tauri::command]
async fn add_key(pubky_str: &str) -> Result<String, BackupAppError> {
    let pubky = parse_pubky_for_command(pubky_str)?;
    let manager = get_manager().await?;

    // Check if key is already being backed up
    if let Some(state) = manager.get_key_state(&pubky) {
        if state.status == KeyStatus::Error {
            debug!("Key is in error state, retrying validation");
            manager
                .retry_errored_key(&pubky)
                .await
                .map_err(BackupAppError::internal)?;
        } else {
            debug!("Key already being backed up");
        }
        manager
            .write_last_pubky(&pubky)
            .await
            .map_err(BackupAppError::internal)?;
        return Ok(pubky.z32());
    }

    manager
        .add_key(pubky.clone())
        .await
        .map_err(BackupAppError::internal)?;

    manager
        .write_last_pubky(&pubky)
        .await
        .map_err(BackupAppError::internal)?;

    info!("Key added and backup started for: {}", pubky);
    Ok(pubky.z32())
}

/// Get all key states for all tracked keys
#[tauri::command]
async fn get_all_key_states() -> Result<HashMap<String, KeyState>, BackupAppError> {
    Ok(get_manager().await?.get_all_key_states())
}

/// Get application config (one-time fetch)
#[tauri::command]
async fn get_config() -> Result<AppConfig, BackupAppError> {
    let m = get_manager().await?;
    Ok(AppConfig {
        developer_mode: is_developer_mode(),
        sync_interval_secs: m.get_sync_interval(),
        keys_dir: m.keys_dir().to_string_lossy().to_string(),
    })
}

/// Get list of pubky keys that have data stored.
///
/// Reads directly from storage so it returns instantly — never blocks on
/// manager initialization or network calls.
#[tauri::command]
async fn get_keys() -> Result<Vec<String>, BackupAppError> {
    // Use the manager if it's already available (lock-free read).
    if let Some(slot) = MANAGER.get() {
        return slot
            .load()
            .list_pubky_directories()
            .map_err(BackupAppError::internal);
    }
    // Manager not ready — read directly from storage (cheap, no network).
    let storage = AppStorage::new().map_err(BackupAppError::internal)?;
    storage
        .list_pubky_directories()
        .map_err(BackupAppError::internal)
}

/// Get the last used pubky from storage.
///
/// Reads directly from storage so it returns instantly — never blocks on
/// manager initialization or network calls.
#[tauri::command]
async fn get_last_pubky() -> Result<Option<String>, BackupAppError> {
    // Use the manager if it's already available (lock-free read).
    if let Some(slot) = MANAGER.get() {
        return match slot.load().read_last_pubky().await {
            Ok(Some(pubky)) => Ok(Some(pubky.z32())),
            Ok(None) => Ok(None),
            Err(e) => Err(BackupAppError::internal(e)),
        };
    }
    // Manager not ready — read directly from storage (cheap, no network).
    let storage = AppStorage::new().map_err(BackupAppError::internal)?;
    match storage.read_last_pubky().await {
        Ok(Some(pubky)) => Ok(Some(pubky.z32())),
        Ok(None) => Ok(None),
        Err(e) => Err(BackupAppError::internal(e)),
    }
}

/// Set the last used pubky (for restoring on next app launch)
#[tauri::command]
async fn set_last_pubky(pubky_str: &str) -> Result<(), BackupAppError> {
    let pubky = parse_pubky_for_command(pubky_str)?;
    get_manager()
        .await?
        .write_last_pubky(&pubky)
        .await
        .map_err(BackupAppError::internal)
}

/// Remove a key from the backup manager, stopping its backup controller.
#[tauri::command]
async fn remove_key(pubky_str: &str) -> Result<(), BackupAppError> {
    let pubky = parse_pubky_for_command(pubky_str)?;
    get_manager()
        .await?
        .remove_key(&pubky)
        .await
        .map_err(BackupAppError::internal)?;
    debug!("Key removed: {}", pubky);
    Ok(())
}

/// Delete a key from the backup manager and remove all backed-up data.
#[tauri::command]
async fn delete_key(pubky_str: &str) -> Result<(), BackupAppError> {
    let pubky = parse_pubky_for_command(pubky_str)?;
    get_manager()
        .await?
        .delete_key(&pubky)
        .await
        .map_err(BackupAppError::internal)?;
    debug!("Key deleted: {}", pubky);
    Ok(())
}

/// Send backup controller task ForceSync message.
#[tauri::command]
async fn force_sync_now(pubky_str: &str) -> Result<(), BackupAppError> {
    let pubky = parse_pubky_for_command(pubky_str)?;
    get_manager()
        .await?
        .force_sync(&pubky)
        .await
        .map_err(BackupAppError::internal)?;
    debug!("Force sync triggered for: {}", pubky);
    Ok(())
}

/// Open the keys directory in the system file manager
#[tauri::command]
async fn open_data_dir(app_handle: tauri::AppHandle) -> Result<(), BackupAppError> {
    use tauri_plugin_opener::OpenerExt;

    let keys_dir = get_manager().await?.keys_dir().to_path_buf();
    app_handle
        .opener()
        .open_path(keys_dir.to_string_lossy().to_string(), None::<&str>)
        .map_err(BackupAppError::internal)
}

/// Set the sync interval in seconds. Controllers pick up the new value dynamically.
#[tauri::command]
async fn set_sync_interval(interval_secs: u64) -> Result<(), BackupAppError> {
    get_manager()
        .await?
        .set_sync_interval(interval_secs)
        .await
        .map_err(BackupAppError::internal)
}

/// Create a snapshot (zip archive) of the specified pubky's backed-up data
#[tauri::command]
async fn create_snapshot(pubky_str: &str) -> Result<String, BackupAppError> {
    let pubky = parse_pubky_for_command(pubky_str)?;
    let path = get_manager()
        .await?
        .create_snapshot(&pubky)
        .await
        .map_err(BackupAppError::internal)?;
    Ok(path.to_string_lossy().to_string())
}

/// Move backup data to a new location.
///
/// Shuts down all controllers, moves the keys directory, then recreates the manager.
/// Sets `RELOCATING` flag so concurrent commands fail fast instead of operating
/// on the stale (shutdown) manager.
#[tauri::command]
async fn set_backup_location(new_parent: &str) -> Result<String, BackupAppError> {
    let new_parent = PathBuf::from(new_parent);

    get_manager().await?;
    // Hold the write-mutex so no other caller swaps concurrently.
    let _guard = manager_write_mutex().lock().await;

    let slot = MANAGER
        .get()
        .ok_or_else(|| BackupAppError::internal("BackupManager not initialized"))?;

    // Block concurrent load_manager() calls while we swap.
    RELOCATING.store(true, Ordering::Release);

    let result: Result<PathBuf, BackupAppError> = async {
        // move_keys shuts down controllers, moves files, updates config.
        // RELOCATING flag prevents concurrent commands from getting a ref
        // to this (now stale) manager.
        let new_keys_dir = slot
            .load()
            .move_keys(&new_parent)
            .await
            .map_err(BackupAppError::internal)?;

        // Create a fresh manager — reads keys_location from config, resumes controllers
        let new_manager = BackupManager::new(create_config())
            .await
            .map_err(BackupAppError::internal)?;
        spawn_event_listener(&new_manager).await;
        slot.store(Arc::new(new_manager));

        Ok(new_keys_dir)
    }
    .await;

    RELOCATING.store(false, Ordering::Release);

    let new_keys_dir = result?;
    info!("Backup location moved to {}", new_keys_dir.display());
    Ok(new_keys_dir.to_string_lossy().to_string())
}

/// Compute aggregate status across all keys and update tray icon
fn update_tray_icon_aggregate() {
    let Some(app_handle) = APP_HANDLE.get() else {
        return;
    };
    let Some(tray) = app_handle.tray_by_id("main") else {
        return;
    };

    // Lock-free read of manager state
    let Some(slot) = MANAGER.get() else {
        return;
    };
    let all_states = slot.load().get_all_key_states();

    // Determine aggregate status
    let mut has_syncing = false;
    let mut has_error = false;
    let mut has_starting = false;

    for state in all_states.values() {
        match &state.status {
            KeyStatus::Syncing { .. } => has_syncing = true,
            KeyStatus::Starting => has_starting = true,
            KeyStatus::Error => has_error = true,
            KeyStatus::Idle | KeyStatus::Stopped => {}
        }
    }

    let (tooltip, title) = if has_syncing {
        ("Pubky Backup - Syncing...", Some("🔄"))
    } else if has_starting {
        ("Pubky Backup - Starting...", Some("⏳"))
    } else if has_error {
        ("Pubky Backup - Error", Some("❌"))
    } else {
        ("Pubky Backup - Synced", Some("✅"))
    };

    let _ = tray.set_tooltip(Some(tooltip));
    if let Some(title_str) = title {
        if let Err(e) = tray.set_title(Some(title_str)) {
            error!("Failed to update tray title: {}", e);
        }
    } else {
        let _ = tray.set_title(None::<&str>);
    }
}

pub fn run() {
    env_logger::init();

    if is_developer_mode() {
        info!("Developer mode enabled via PUBKY_DEVELOPER_MODE environment variable");
    }

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        // After a minimise the close handler loses its scope and fails
        // This is a Tauri bug - https://github.com/tauri-apps/tauri/issues/9504
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                window.hide().unwrap();
            }
        })
        .setup(|app| {
            // Store app handle for tray updates and event emission
            let app_handle = app.handle().clone();
            let _ = APP_HANDLE.set(app_handle.clone());

            // Spawn background task to initialize the manager early
            tauri::async_runtime::spawn(async move {
                if let Err(e) = ensure_manager().await {
                    error!("Failed to create backup manager: {:?}", e);
                }
            });

            let quit = MenuItemBuilder::with_id("quit", "Quit").build(app)?;
            let show = MenuItemBuilder::with_id("show", "Show").build(app)?;
            let hide = MenuItemBuilder::with_id("hide", "Hide").build(app)?;
            let menu = MenuBuilder::new(app)
                .items(&[&show, &hide, &quit])
                .build()?;

            let _tray = TrayIconBuilder::with_id("main")
                .menu(&menu)
                .icon(app.default_window_icon().unwrap().clone())
                .tooltip("pubky-backup")
                .on_menu_event(move |app, event| match event.id.as_ref() {
                    "show" => {
                        let windows = app.webview_windows();
                        if let Some(window) = windows.values().next() {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                    "hide" => {
                        let windows = app.webview_windows();
                        windows
                            .values()
                            .next()
                            .expect("sorry, no window found")
                            .hide()
                            .expect("can't Hide Window");
                    }
                    "quit" => {
                        app.exit(0);
                    }
                    _ => {}
                })
                .build(app)?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            add_key,
            get_all_key_states,
            get_config,
            get_keys,
            get_last_pubky,
            set_last_pubky,
            remove_key,
            delete_key,
            force_sync_now,
            open_data_dir,
            create_snapshot,
            set_sync_interval,
            set_backup_location
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
