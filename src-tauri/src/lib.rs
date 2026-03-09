mod error;

use log::{debug, error, info};
use pubky::PublicKey;
use serde::Serialize;
use std::collections::HashMap;

use std::{str::FromStr, sync::OnceLock};
use tauri::{
    menu::{MenuBuilder, MenuItemBuilder},
    tray::TrayIconBuilder,
    AppHandle, Emitter, Manager, WindowEvent,
};

use crate::error::BackupAppError;
use pubky_backup_core::{
    is_developer_mode, BackupManager, BackupManagerConfig, KeyState, KeyStatus,
};

/// Global manager instance
static MANAGER: OnceLock<BackupManager> = OnceLock::new();
/// Tauri app handle for tray updates and event emission
static APP_HANDLE: OnceLock<AppHandle> = OnceLock::new();

/// Config response for frontend
#[derive(Clone, Serialize)]
pub struct AppConfig {
    developer_mode: bool,
}

fn get_manager() -> Result<&'static BackupManager, BackupAppError> {
    MANAGER
        .get()
        .ok_or_else(|| BackupAppError::internal("BackupManager not initialized"))
}

/// Get or create the manager for storage access.
///
/// This is used by commands that need to access storage before the main
/// manager is initialized (e.g., get_last_pubky, get_previous_pubky_keys).
async fn get_or_create_manager() -> Result<&'static BackupManager, BackupAppError> {
    if let Some(manager) = MANAGER.get() {
        return Ok(manager);
    }

    // Create a new manager and try to set it
    let config = BackupManagerConfig {
        developer_mode: is_developer_mode(),
        ..Default::default()
    };

    let manager = BackupManager::new(config)
        .await
        .map_err(BackupAppError::internal)?;

    // Try to set it - if another thread beat us, use theirs
    let _ = MANAGER.set(manager);

    // Now it's definitely set
    MANAGER
        .get()
        .ok_or_else(|| BackupAppError::internal("Failed to initialize manager"))
}

/// Add a key to start backing up.
#[tauri::command]
async fn add_key(pubky_str: &str) -> Result<String, BackupAppError> {
    let pubky = PublicKey::from_str(pubky_str).map_err(|e| BackupAppError::InvalidPubkyFormat {
        message: e.to_string(),
    })?;

    // Get or create the manager
    let manager = get_or_create_manager().await?;

    // Check if key is already being backed up
    if manager.get_key_state(&pubky).is_some() {
        debug!("Key already being backed up");
        // Save as last pubky even if already exists
        manager
            .write_last_pubky(&pubky)
            .await
            .map_err(BackupAppError::internal)?;
        return Ok(pubky.z32());
    }

    // Add the key to start backing up
    manager
        .add_key(pubky.clone())
        .await
        .map_err(BackupAppError::internal)?;

    // Save as last pubky
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
    let manager = get_or_create_manager().await?;
    Ok(manager.get_all_key_states())
}

/// Get application config (one-time fetch)
#[tauri::command]
fn get_config() -> AppConfig {
    AppConfig {
        developer_mode: is_developer_mode(),
    }
}

/// Get list of pubky keys that have data stored
#[tauri::command]
async fn get_keys() -> Result<Vec<String>, BackupAppError> {
    // Use existing manager if available, otherwise create a temporary one
    let manager = get_or_create_manager().await?;
    manager
        .list_pubky_directories()
        .map_err(BackupAppError::internal)
}

/// Get the last used pubky from storage
#[tauri::command]
async fn get_last_pubky() -> Result<Option<String>, BackupAppError> {
    let manager = get_or_create_manager().await?;
    match manager.read_last_pubky().await {
        Ok(Some(pubky)) => Ok(Some(pubky.z32())),
        Ok(None) => Ok(None),
        Err(e) => Err(BackupAppError::internal(e)),
    }
}

/// Set the last used pubky (for restoring on next app launch)
#[tauri::command]
async fn set_last_pubky(pubky_str: &str) -> Result<(), BackupAppError> {
    let pubky = PublicKey::from_str(pubky_str).map_err(|e| BackupAppError::InvalidPubkyFormat {
        message: e.to_string(),
    })?;
    let manager = get_manager()?;
    manager
        .write_last_pubky(&pubky)
        .await
        .map_err(BackupAppError::internal)
}

/// Remove a key from the backup manager, stopping its backup controller.
#[tauri::command]
async fn remove_key(pubky_str: &str) -> Result<(), BackupAppError> {
    let pubky = PublicKey::from_str(pubky_str).map_err(|e| BackupAppError::InvalidPubkyFormat {
        message: e.to_string(),
    })?;
    let manager = get_manager()?;

    manager
        .remove_key(&pubky)
        .await
        .map_err(BackupAppError::internal)?;

    debug!("Key removed: {}", pubky);
    Ok(())
}

/// Delete a key from the backup manager and remove all backed-up data.
#[tauri::command]
async fn delete_key(pubky_str: &str) -> Result<(), BackupAppError> {
    let pubky = PublicKey::from_str(pubky_str).map_err(|e| BackupAppError::InvalidPubkyFormat {
        message: e.to_string(),
    })?;
    let manager = get_manager()?;

    manager
        .delete_key(&pubky)
        .await
        .map_err(BackupAppError::internal)?;

    debug!("Key deleted: {}", pubky);
    Ok(())
}

/// Send backup controller task ForceSync message.
#[tauri::command]
async fn force_sync_now(pubky_str: &str) -> Result<(), BackupAppError> {
    let pubky = PublicKey::from_str(pubky_str).map_err(|e| BackupAppError::InvalidPubkyFormat {
        message: e.to_string(),
    })?;
    let manager = get_manager()?;

    manager
        .force_sync(&pubky)
        .await
        .map_err(BackupAppError::internal)?;

    debug!("Force sync triggered for: {}", pubky);
    Ok(())
}

/// Get the data directory path as a string
#[tauri::command]
async fn get_data_dir_path() -> Result<String, BackupAppError> {
    let manager = get_manager()?;
    Ok(manager.data_dir().to_string_lossy().to_string())
}

/// Open the data directory in the system file manager
#[tauri::command]
async fn open_data_dir(app_handle: tauri::AppHandle) -> Result<(), BackupAppError> {
    use tauri_plugin_opener::OpenerExt;

    let manager = get_manager()?;
    let backup_dir = manager.data_dir();

    app_handle
        .opener()
        .open_path(backup_dir.to_string_lossy().to_string(), None::<&str>)
        .map_err(BackupAppError::internal)
}

/// Create a snapshot (zip archive) of the specified pubky's backed-up data
#[tauri::command]
async fn create_snapshot(pubky_str: &str) -> Result<String, BackupAppError> {
    let pubky = PublicKey::from_str(pubky_str).map_err(|e| BackupAppError::InvalidPubkyFormat {
        message: e.to_string(),
    })?;
    let manager = get_manager()?;

    let snapshot_path = manager
        .create_snapshot(&pubky)
        .await
        .map_err(BackupAppError::internal)?;

    Ok(snapshot_path.to_string_lossy().to_string())
}

/// Compute aggregate status across all keys and update tray icon
fn update_tray_icon_aggregate() {
    let Some(app_handle) = APP_HANDLE.get() else {
        return;
    };
    let Some(tray) = app_handle.tray_by_id("main") else {
        return;
    };
    let Some(manager) = MANAGER.get() else {
        return;
    };

    let all_states = manager.get_all_key_states();

    // Determine aggregate status:
    // - "Syncing" if any key is syncing or starting
    // - "Error" if any key has error (and none syncing)
    // - "Idle" if all keys are idle
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

            // Spawn background task to set up global event listener
            tauri::async_runtime::spawn(async move {
                // Get or create the manager
                let manager = match get_or_create_manager().await {
                    Ok(m) => m,
                    Err(e) => {
                        error!("Failed to create backup manager: {:?}", e);
                        return;
                    }
                };

                // Subscribe to all key updates
                let mut rx = manager.subscribe();

                // Listen for updates and emit to frontend
                while let Ok(update) = rx.recv().await {
                    // Emit event to frontend
                    if let Err(e) = app_handle.emit("key-update", &update) {
                        error!("Failed to emit key-update event: {}", e);
                    }
                    // Update tray icon with aggregate status
                    update_tray_icon_aggregate();
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
            get_data_dir_path,
            open_data_dir,
            create_snapshot
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
