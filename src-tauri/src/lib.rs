mod error;

use log::{debug, error, info};
use pubky::{Pkdns, PubkyResource, PublicKey, PublicStorage};
use serde::Serialize;
use serde_with::{serde_as, DisplayFromStr};

use std::{
    env,
    str::FromStr,
    sync::{Arc, Mutex},
};
use tauri::{
    menu::{MenuBuilder, MenuItemBuilder},
    tray::TrayIconBuilder,
    AppHandle, Manager, WindowEvent,
};
use tokio::sync::broadcast;

use crate::error::BackupAppError;
use pubky_backup_core::{
    is_developer_mode, AppStorage, BackupController, BackupControllerMessage,
    BackupControllerStatus, DEV_MODE_PUBKY,
};

const SYNC_INTERVAL_SECONDS: u64 = 30;

/// Represents a running backup process
#[derive(Clone)]
pub struct BackupProcess {
    backup_control_tx: broadcast::Sender<BackupControllerMessage>,
}

/// Get the next sync time (current time + sync interval)
fn next_sync_time() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        + SYNC_INTERVAL_SECONDS
}

// TODO: Mutex here feels fine for now whilst we have a single worker task and single state poller. As the App becomes more complex we should reconsider this choice.
pub static APP_STATE: Mutex<AppState> = Mutex::new(AppState {
    pubky: None,
    homeserver: None,
    developer_mode: false,
    is_syncing: false,
    next_sync_time: 0,
    data_dir_size: 0,
    backup_controller_error: None,
    storage: None,
    backup_process: None,
    app_handle: None,
});

/// AppState is Tauri's Rust back-end State.
/// Here we provide an interface for the front-end and manage other application tasks (eg. The Backup task)
#[serde_as]
#[derive(Clone, Serialize)]
pub struct AppState {
    /// This session's pubky
    #[serde_as(as = "Option<DisplayFromStr>")]
    pubky: Option<PublicKey>,
    /// This session's pubky's homeserver. Stored only for displaying in GUI.
    #[serde_as(as = "Option<DisplayFromStr>")]
    homeserver: Option<PublicKey>,
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
    #[serde(skip)]
    storage: Option<Arc<AppStorage>>,
    #[serde(skip)]
    backup_process: Option<BackupProcess>,
    #[serde(skip)]
    app_handle: Option<AppHandle>,
}

fn get_or_create_storage() -> Result<Arc<AppStorage>, BackupAppError> {
    let mut state = APP_STATE
        .lock()
        .map_err(|_| BackupAppError::lock_failed())?;

    if state.storage.is_none() {
        let storage = AppStorage::new().map_err(BackupAppError::internal)?;
        state.storage = Some(Arc::new(storage));
    }

    Ok(state.storage.clone().unwrap())
}

/// Take a pubky, verify and add to State ready for usage.
#[tauri::command]
async fn init_app_state(pubky_str: &str) -> Result<(), BackupAppError> {
    if is_developer_mode() {
        if let Ok(mut state) = APP_STATE.lock() {
            let dev_pubky = PublicKey::from_str(DEV_MODE_PUBKY).expect("Dev mode pubky is valid");
            let dev_homeserver =
                PublicKey::from_str(DEV_MODE_PUBKY).expect("Dev mode homeserver is valid");
            state.pubky = Some(dev_pubky);
            state.homeserver = Some(dev_homeserver);
            return Ok(());
        }
    }

    let pubky = PublicKey::from_str(pubky_str)
        .map_err(|e| BackupAppError::InvalidPubkyFormat { message: e.to_string() })?;

    // Check pubky is discoverable
    let homeserver_pubky_str = Pkdns::new()
        .map_err(BackupAppError::internal)?
        .get_homeserver_of(&pubky)
        .await
        .ok_or_else(|| BackupAppError::HomeserverNotFound { message: "Failed to find Homeserver for pubky".to_string() })?;
    let homeserver_pubky = PublicKey::from_str(&homeserver_pubky_str)
        .map_err(|e| BackupAppError::InvalidPubkyFormat { message: e.to_string() })?;

    // Check Pubky has /pub/ data on Homeserver
    let pubky_storage = PublicStorage::new().map_err(BackupAppError::internal)?;

    let path = PubkyResource::new(pubky.clone(), "/pub/").map_err(BackupAppError::internal)?;

    // TODO: We should check the pub key has data with exists(), but currently incorrectly returns 401 (https://github.com/pubky/pubky-core/issues/236)
    // if !pubky_storage.exists(path.clone()).await.map_err(|e| format!("Internal error: {}", e))? {
    //     return Err(format!("Failed to find data for pubky"));
    // }

    // Instead for now we can call `get` on the base pub path which will pull the urls of every item which the key has published.
    if (pubky_storage.get(path).await).is_err() {
        return Err(BackupAppError::DataNotFound { message: "Failed to find data for pubky".to_string() });
    }
    info!("Pubky is valid for Backup: {}", pubky);

    // Save the last used pubky to storage
    get_or_create_storage()
        .map_err(BackupAppError::internal)?
        .write_last_pubky(&pubky)
        .await
        .map_err(BackupAppError::internal)?;

    let mut state = APP_STATE
        .lock()
        .map_err(|_| BackupAppError::lock_failed())?;
    state.pubky = Some(pubky);
    state.homeserver = Some(homeserver_pubky);

    Ok(())
}

/// Fetch application state for usage in front-end
#[tauri::command]
async fn fetch_state() -> Result<AppState, BackupAppError> {
    match APP_STATE.lock() {
        Ok(mut state) => {
            // Always sync developer_mode from environment variable
            state.developer_mode = is_developer_mode();
            Ok(state.clone())
        }
        Err(_) => Err(BackupAppError::lock_failed()),
    }
}

/// Get list of previously used pubky keys that have data stored
#[tauri::command]
async fn get_previous_pubky_keys() -> Result<Vec<String>, BackupAppError> {
    let storage = match get_or_create_storage() {
        Ok(storage) => storage,
        Err(e) => {
            return Err(BackupAppError::internal(e));
        }
    };

    // List directories in the data directory to find existing pubky keys
    match storage.list_pubky_directories().await {
        Ok(keys) => Ok(keys),
        Err(e) => Err(BackupAppError::internal(e)),
    }
}

/// Get the last used pubky from storage
#[tauri::command]
async fn get_last_pubky() -> Result<Option<String>, BackupAppError> {
    let storage = match get_or_create_storage() {
        Ok(storage) => storage,
        Err(e) => {
            return Err(BackupAppError::internal(e));
        }
    };

    match storage.read_last_pubky().await {
        Ok(Some(pubky)) => Ok(Some(pubky.to_string())),
        Ok(None) => Ok(None),
        Err(e) => Err(BackupAppError::internal(e)),
    }
}

/// Spawn task for downloads and polling.
/// To be called by front-end upon entering main screen.
#[tauri::command]
async fn backup_controller_begin() -> Result<(), BackupAppError> {
    // Extract required data from state
    let (pubky, storage) = {
        let state = APP_STATE
            .lock()
            .map_err(|_| BackupAppError::lock_failed())?;

        let pubky = state
            .pubky
            .clone()
            .ok_or_else(|| BackupAppError::internal("Pubky not available in AppState"))?;

        let storage = state
            .storage
            .clone()
            .ok_or_else(|| BackupAppError::internal("Storage not available in AppState"))?;

        (pubky, storage)
    };

    // Initialise state
    let (backup_control_tx, backup_control_rx) = broadcast::channel(5);
    let (status_tx, mut status_rx) = broadcast::channel(5);
    let initial_size = storage.calculate_pubky_size(&pubky).await;
    {
        let mut state = APP_STATE
            .lock()
            .map_err(|_| BackupAppError::lock_failed())?;

        state.backup_process = Some(BackupProcess {
            backup_control_tx: backup_control_tx.clone(),
        });
        state.data_dir_size = initial_size;
        state.backup_controller_error = None;
    }

    // Spawn a task to listen for status updates
    let storage_clone = storage.clone();
    let pubky_clone = pubky.clone();
    tauri::async_runtime::spawn(async move {
        loop {
            match status_rx.recv().await {
                Ok(status) => match status {
                    BackupControllerStatus::Syncing { events_processed } => {
                        // Recalculate size only if events were processed (data changed)
                        let data_dir_size = if events_processed > 0 {
                            Some(storage_clone.calculate_pubky_size(&pubky_clone).await)
                        } else {
                            None
                        };
                        if let Ok(mut state) = APP_STATE.lock() {
                            state.is_syncing = true;
                            if let Some(size) = data_dir_size {
                                state.data_dir_size = size;
                            }
                            update_tray_icon(&state);
                        }
                    }
                    BackupControllerStatus::Idle => {
                        if let Ok(mut state) = APP_STATE.lock() {
                            state.is_syncing = false;
                            state.next_sync_time = next_sync_time();
                            update_tray_icon(&state);
                        }
                    }
                    BackupControllerStatus::Ended => {
                        if let Ok(mut state) = APP_STATE.lock() {
                            state.is_syncing = false;
                            state.backup_process = None;
                            update_tray_icon(&state);
                        }
                        break;
                    }
                    BackupControllerStatus::Error { message } => {
                        if let Ok(mut state) = APP_STATE.lock() {
                            state.is_syncing = false;
                            state.backup_process = None;
                            state.backup_controller_error = Some(message);
                            update_tray_icon(&state);
                        }
                        break;
                    }
                },
                Err(e) => {
                    // Channel closed unexpectedly (controller crashed/panicked)
                    if let Ok(mut state) = APP_STATE.lock() {
                        state.is_syncing = false;
                        state.backup_controller_error =
                            Some(format!("Backup controller stopped unexpectedly: {}", e));
                        state.backup_process = None;
                        update_tray_icon(&state);
                    }
                    break;
                }
            }
        }
    });

    // Spawn the backup controller
    let controller =
        BackupController::new(pubky, storage, Some(backup_control_rx), Some(status_tx));

    // Keep the sender alive by moving it into the spawned task
    // This prevents the channel from closing if state.backup_process is temporarily cleared for whatever reason
    tauri::async_runtime::spawn(async move {
        let _tx = backup_control_tx;
        controller.run().await;
    });
    info!("Backup controller task started");
    Ok(())
}

/// Send backup controller task Cancel message.
/// To be controlled by front-end on exiting main screen.
#[tauri::command]
async fn backup_controller_close() -> Result<(), BackupAppError> {
    let state = APP_STATE
        .lock()
        .map_err(|_| BackupAppError::lock_failed())?;

    if let Some(backup_process) = &state.backup_process {
        let _ = backup_process
            .backup_control_tx
            .send(BackupControllerMessage::Cancel);
        debug!("Backup controller task stop signal sent");
        Ok(())
    } else {
        Err(BackupAppError::internal(
            "No Backup controller task running",
        ))
    }
}

/// Send backup controller task ForceSync message.
#[tauri::command]
async fn force_sync_now() -> Result<(), BackupAppError> {
    let state = APP_STATE
        .lock()
        .map_err(|_| BackupAppError::lock_failed())?;

    if let Some(backup_process) = &state.backup_process {
        match backup_process
            .backup_control_tx
            .send(BackupControllerMessage::ForceSync)
        {
            Ok(_) => {
                debug!("Force sync signal sent");
                Ok(())
            }
            Err(_) => Err(BackupAppError::internal("Failed to send force sync signal")),
        }
    } else {
        Err(BackupAppError::internal(
            "No Backup controller task running",
        ))
    }
}

/// Get the data directory path as a string
#[tauri::command]
async fn get_data_dir_path() -> Result<String, BackupAppError> {
    let storage = get_or_create_storage().map_err(BackupAppError::internal)?;
    let backup_dir = storage
        .get_backup_data_dir()
        .map_err(BackupAppError::internal)?;
    Ok(backup_dir.to_string_lossy().to_string())
}

/// Open the data directory in the system file manager
#[tauri::command]
async fn open_data_dir(app_handle: tauri::AppHandle) -> Result<(), BackupAppError> {
    use tauri_plugin_opener::OpenerExt;

    let storage = get_or_create_storage().map_err(BackupAppError::internal)?;
    let backup_dir = storage
        .get_backup_data_dir()
        .map_err(BackupAppError::internal)?;

    app_handle
        .opener()
        .open_path(backup_dir.to_string_lossy().to_string(), None::<&str>)
        .map_err(BackupAppError::internal)
}

/// Update tray icon based on sync status
fn update_tray_icon(state: &AppState) {
    if let Some(app_handle) = &state.app_handle {
        if let Some(tray) = app_handle.tray_by_id("main") {
            let (tooltip, title) = match (&state.backup_process, state.is_syncing) {
                (None, _) => {
                    // Backup process not running - no icon
                    ("Pubky Backup", None)
                }
                (Some(_), true) => {
                    // Running and syncing
                    ("🔄 Pubky Backup - Syncing...", Some("🔄"))
                }
                (Some(_), false) => {
                    // Running but idle
                    ("✅ Pubky Backup - Synced", Some("✅"))
                }
            };

            let _ = tray.set_tooltip(Some(tooltip));

            if let Some(title_str) = title {
                if let Err(e) = tray.set_title(Some(title_str)) {
                    error!("Failed to update tray title: {}", e);
                }
            } else {
                // Clear title when backup process is not running
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
            // Store app handle for tray updates and sync developer mode
            if let Ok(mut state) = APP_STATE.lock() {
                state.app_handle = Some(app.handle().clone());
                state.developer_mode = is_developer_mode();
            }
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
            init_app_state,
            fetch_state,
            get_previous_pubky_keys,
            get_last_pubky,
            backup_controller_begin,
            backup_controller_close,
            force_sync_now,
            get_data_dir_path,
            open_data_dir
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
