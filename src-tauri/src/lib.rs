mod error;

use log::{debug, error, info};
use pubky::PublicKey;
use serde::Serialize;
use serde_with::{serde_as, DisplayFromStr};

use std::{str::FromStr, sync::OnceLock, sync::RwLock};
use tauri::{
    menu::{MenuBuilder, MenuItemBuilder},
    tray::TrayIconBuilder,
    AppHandle, Manager, WindowEvent,
};

use crate::error::BackupAppError;
use pubky_backup_core::{
    is_developer_mode, BackupManager, BackupManagerConfig, KeyState, KeyStatus, KeyUpdate,
    DEV_MODE_PUBKY,
};

/// Global manager instance
static MANAGER: OnceLock<BackupManager> = OnceLock::new();
/// The pubky currently being viewed in the UI (can be switched between keys)
static VIEWED_PUBKY: RwLock<Option<PublicKey>> = RwLock::new(None);
/// Tauri app handle for tray updates
static APP_HANDLE: OnceLock<AppHandle> = OnceLock::new();

/// Default next sync time (30 seconds from now)
fn default_next_sync_time() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        + 30
}

/// AppState for the front-end.
/// This is a simplified view derived from the BackupManager's KeyState.
#[serde_as]
#[derive(Clone, Serialize)]
pub struct AppState {
    /// This session's pubky
    #[serde_as(as = "Option<DisplayFromStr>")]
    pubky: Option<PublicKey>,
    /// Developer mode for working on the front-end - doesnt make network calls and populates with mock data.
    developer_mode: bool,
    /// Current sync status
    is_syncing: bool,
    /// Next sync time in seconds since epoch (for countdown display)
    next_sync_time: u64,
    /// Size of data stored for current pubky in bytes
    data_dir_size: u64,
    /// Error message if backup controller failed, None if running normally
    backup_controller_error: Option<String>,
    /// Whether backup is currently running
    backup_running: bool,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            pubky: None,
            developer_mode: is_developer_mode(),
            is_syncing: false,
            next_sync_time: 0,
            data_dir_size: 0,
            backup_controller_error: None,
            backup_running: false,
        }
    }
}

impl AppState {
    /// Create AppState from KeyState and pubky
    fn from_key_state(pubky: &PublicKey, key_state: &KeyState) -> Self {
        let (is_syncing, backup_controller_error, backup_running) = match &key_state.status {
            KeyStatus::Starting => (false, None, true),
            KeyStatus::Syncing { .. } => (true, None, true),
            KeyStatus::Idle => (false, None, true),
            KeyStatus::Stopped => (false, None, false),
            KeyStatus::Error => (
                false,
                key_state.error.as_ref().map(|e| e.message.clone()),
                false,
            ),
        };

        Self {
            pubky: Some(pubky.clone()),
            developer_mode: is_developer_mode(),
            is_syncing,
            next_sync_time: key_state.next_sync.unwrap_or_else(default_next_sync_time),
            data_dir_size: key_state.data_size,
            backup_controller_error,
            backup_running,
        }
    }
}

fn get_manager() -> Result<&'static BackupManager, BackupAppError> {
    MANAGER
        .get()
        .ok_or_else(|| BackupAppError::internal("BackupManager not initialized"))
}

fn get_viewed_pubky() -> Result<PublicKey, BackupAppError> {
    let guard = VIEWED_PUBKY
        .read()
        .map_err(|_| BackupAppError::internal("VIEWED_PUBKY lock poisoned"))?;
    guard
        .clone()
        .ok_or_else(|| BackupAppError::internal("Viewed pubky not set"))
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
async fn add_key(pubky_str: &str) -> Result<(), BackupAppError> {
    // Handle developer mode
    let pubky = if is_developer_mode() {
        info!("Developer mode: using dev pubky");
        PublicKey::from_str(DEV_MODE_PUBKY).expect("Dev mode pubky is valid")
    } else {
        PublicKey::from_str(pubky_str).map_err(|e| BackupAppError::InvalidPubkyFormat {
            message: e.to_string(),
        })?
    };

    // Get or create the manager
    let manager = get_or_create_manager().await?;

    // Set as viewed pubky
    {
        let mut viewed = VIEWED_PUBKY
            .write()
            .map_err(|_| BackupAppError::internal("VIEWED_PUBKY lock poisoned"))?;
        *viewed = Some(pubky.clone());
    }

    // Check if key is already being backed up
    if manager.get_key_state(&pubky).is_some() {
        debug!("Key already being backed up, setting as viewed");
        // Save as last pubky even if already exists
        manager
            .write_last_pubky(&pubky)
            .await
            .map_err(BackupAppError::internal)?;
        return Ok(());
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

    // Subscribe to status updates for tray icon
    let mut rx = manager.subscribe();
    let pubky_clone = pubky.clone();

    tauri::async_runtime::spawn(async move {
        while let Ok(update) = rx.recv().await {
            if update.pubky == pubky_clone {
                update_tray_icon(&update);
            }
        }
    });

    info!("Key added and backup started for: {}", pubky);
    Ok(())
}

/// Fetch application state for usage in front-end
#[tauri::command]
async fn fetch_state() -> Result<AppState, BackupAppError> {
    let pubky = {
        let guard = VIEWED_PUBKY
            .read()
            .map_err(|_| BackupAppError::internal("VIEWED_PUBKY lock poisoned"))?;
        match guard.clone() {
            Some(p) => p,
            None => return Ok(AppState::default()),
        }
    };

    let manager = match MANAGER.get() {
        Some(m) => m,
        None => return Ok(AppState::default()),
    };

    match manager.get_key_state(&pubky) {
        Some(key_state) => Ok(AppState::from_key_state(&pubky, &key_state)),
        None => {
            // Key not added yet, return default state with pubky set
            Ok(AppState {
                pubky: Some(pubky),
                developer_mode: is_developer_mode(),
                ..Default::default()
            })
        }
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
    // Use existing manager if available, otherwise create a temporary one
    let manager = get_or_create_manager().await?;
    match manager.read_last_pubky().await {
        Ok(Some(pubky)) => {
            // Initialize VIEWED_PUBKY from stored last pubky if not already set
            {
                let mut viewed = VIEWED_PUBKY
                    .write()
                    .map_err(|_| BackupAppError::internal("VIEWED_PUBKY lock poisoned"))?;
                if viewed.is_none() {
                    *viewed = Some(pubky.clone());
                }
            }
            Ok(Some(pubky.to_string()))
        }
        Ok(None) => Ok(None),
        Err(e) => Err(BackupAppError::internal(e)),
    }
}

/// Set which pubky is currently being viewed in the UI
#[tauri::command]
fn set_viewed_pubky(pubky_str: &str) -> Result<(), BackupAppError> {
    let pubky = PublicKey::from_str(pubky_str).map_err(|e| BackupAppError::InvalidPubkyFormat {
        message: e.to_string(),
    })?;

    let mut viewed = VIEWED_PUBKY
        .write()
        .map_err(|_| BackupAppError::internal("VIEWED_PUBKY lock poisoned"))?;
    *viewed = Some(pubky);

    Ok(())
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

/// Send backup controller task ForceSync message.
#[tauri::command]
async fn force_sync_now() -> Result<(), BackupAppError> {
    let pubky = get_viewed_pubky()?;
    let manager = get_manager()?;

    manager
        .force_sync(&pubky)
        .await
        .map_err(BackupAppError::internal)?;

    debug!("Force sync triggered");
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

/// Create a snapshot (zip archive) of the current pubky's backed-up data
#[tauri::command]
async fn create_snapshot() -> Result<String, BackupAppError> {
    let pubky = get_viewed_pubky()?;
    let manager = get_manager()?;

    let snapshot_path = manager
        .create_snapshot(&pubky)
        .await
        .map_err(BackupAppError::internal)?;

    Ok(snapshot_path.to_string_lossy().to_string())
}

/// Update tray icon based on sync status
fn update_tray_icon(update: &KeyUpdate) {
    if let Some(app_handle) = APP_HANDLE.get() {
        if let Some(tray) = app_handle.tray_by_id("main") {
            let (tooltip, title) = match &update.state.status {
                KeyStatus::Starting => ("Pubky Backup - Starting...", Some("⏳")),
                KeyStatus::Syncing { .. } => ("🔄 Pubky Backup - Syncing...", Some("🔄")),
                KeyStatus::Idle => ("✅ Pubky Backup - Synced", Some("✅")),
                KeyStatus::Stopped => ("Pubky Backup - Stopped", None),
                KeyStatus::Error => ("❌ Pubky Backup - Error", Some("❌")),
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
            // Store app handle for tray updates
            let _ = APP_HANDLE.set(app.handle().clone());

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
            fetch_state,
            get_keys,
            get_last_pubky,
            set_viewed_pubky,
            remove_key,
            force_sync_now,
            get_data_dir_path,
            open_data_dir,
            create_snapshot
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
