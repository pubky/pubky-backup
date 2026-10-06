mod error;

use log::{debug, error, info, warn};
use pubky::PublicKey;
use serde::Serialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use crate::error::BackupAppError;
use pubky_backup_core::{
    is_testnet_mode, parse_pubky, ActivityEntry, AppStorage, BackupManager, BackupManagerConfig,
    KeyState, KeyStatus,
};
use tauri::{
    ipc::Channel,
    menu::{MenuBuilder, MenuItemBuilder},
    tray::TrayIconBuilder,
    AppHandle, Emitter, Manager, WindowEvent,
};
use tokio::sync::{Mutex, RwLock};

/// The backup manager behind the commands, created on first use.
///
/// The write lock is held while the manager is created or replaced, e.g.
/// when the backup location moves, so commands wait for the new manager
/// instead of reaching one that is shutting down.
#[derive(Default)]
struct AppState {
    manager: RwLock<Option<Arc<BackupManager>>>,
    /// The task forwarding the manager's updates to the frontend
    event_listener: Mutex<Option<tauri::async_runtime::JoinHandle<()>>>,
}

/// Config response for frontend
#[derive(Clone, Serialize)]
pub struct AppConfig {
    sync_interval_secs: u64,
    keys_dir: String,
}

/// Create a BackupManager for mainnet, or for a local testnet in testnet mode.
async fn create_manager() -> Result<BackupManager, BackupAppError> {
    let config = BackupManagerConfig::default();
    let manager = if is_testnet_mode() {
        BackupManager::testnet(config).await?
    } else {
        BackupManager::new(config).await?
    };
    Ok(manager)
}

/// The manager, created on first use.
async fn get_manager(app: &AppHandle) -> Result<Arc<BackupManager>, BackupAppError> {
    let state = app.state::<AppState>();
    if let Some(manager) = state.manager.read().await.clone() {
        return Ok(manager);
    }

    let mut slot = state.manager.write().await;
    // Created meanwhile by whoever held the write lock first
    if let Some(manager) = slot.clone() {
        return Ok(manager);
    }

    let manager = Arc::new(create_manager().await?);
    spawn_event_listener(app, &state, &manager).await;
    *slot = Some(manager.clone());
    Ok(manager)
}

/// The manager if it has been created, without waiting for it.
fn manager_if_ready(app: &AppHandle) -> Option<Arc<BackupManager>> {
    app.state::<AppState>()
        .manager
        .try_read()
        .ok()
        .and_then(|slot| slot.clone())
}

/// Parse a pubky z32 string, mapping errors to [`BackupAppError::InvalidPubkyFormat`].
fn parse_pubky_for_command(pubky_str: &str) -> Result<PublicKey, BackupAppError> {
    parse_pubky(pubky_str).map_err(|e| BackupAppError::InvalidPubkyFormat {
        message: e.to_string(),
    })
}

/// Forward the manager's key updates to the frontend and the tray, replacing
/// the listener of any previous manager.
async fn spawn_event_listener(app: &AppHandle, state: &AppState, manager: &Arc<BackupManager>) {
    let mut rx = manager.subscribe();
    let app = app.clone();
    let manager = manager.clone();

    debug!("Event listener spawned for BackupManager");
    let handle = tauri::async_runtime::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(update) => {
                    debug!("Emitting key-update to frontend: {}", update.pubky);
                    if let Err(e) = app.emit("key-update", &update) {
                        error!("Failed to emit key-update event: {}", e);
                    }
                    update_tray_icon(&app, &manager);
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    warn!("Event listener lagged, skipped {} messages", n);
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    debug!("Event listener channel closed");
                    break;
                }
            }
        }
    });

    if let Some(previous) = state.event_listener.lock().await.replace(handle) {
        debug!("Aborting previous event listener");
        previous.abort();
    }
}

/// Add a key to start backing up, or retry it if its backup is in error.
#[tauri::command]
async fn add_key(app: AppHandle, pubky_str: &str) -> Result<String, BackupAppError> {
    let pubky = parse_pubky_for_command(pubky_str)?;
    let manager = get_manager(&app).await?;

    manager.ensure_backed_up(&pubky).await?;
    manager.write_last_pubky(&pubky)?;

    info!("Backing up key: {}", pubky);
    Ok(pubky.z32())
}

/// Get all key states for all tracked keys
#[tauri::command]
async fn get_all_key_states(app: AppHandle) -> Result<HashMap<String, KeyState>, BackupAppError> {
    Ok(get_manager(&app).await?.get_all_key_states())
}

/// Get application config (one-time fetch)
#[tauri::command]
async fn get_config(app: AppHandle) -> Result<AppConfig, BackupAppError> {
    let manager = get_manager(&app).await?;
    Ok(AppConfig {
        sync_interval_secs: manager.get_sync_interval(),
        keys_dir: manager.keys_dir().to_string_lossy().to_string(),
    })
}

/// Get list of pubky keys that have data stored.
///
/// Reads directly from storage so it returns instantly — never blocks on
/// manager initialization or network calls.
#[tauri::command]
async fn get_keys(app: AppHandle) -> Result<Vec<String>, BackupAppError> {
    if let Some(manager) = manager_if_ready(&app) {
        return Ok(manager.list_pubky_directories()?);
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
async fn get_last_pubky(app: AppHandle) -> Result<Option<String>, BackupAppError> {
    let last_pubky = if let Some(manager) = manager_if_ready(&app) {
        manager.read_last_pubky()?
    } else {
        // Manager not ready — read directly from storage (cheap, no network).
        let storage = AppStorage::new().map_err(BackupAppError::internal)?;
        storage
            .read_last_pubky()
            .map_err(BackupAppError::internal)?
    };
    Ok(last_pubky.map(|pubky| pubky.z32()))
}

/// Set the last used pubky (for restoring on next app launch)
#[tauri::command]
async fn set_last_pubky(app: AppHandle, pubky_str: &str) -> Result<(), BackupAppError> {
    let pubky = parse_pubky_for_command(pubky_str)?;
    get_manager(&app).await?.write_last_pubky(&pubky)?;
    Ok(())
}

/// Remove a key from the backup manager, stopping its backup controller.
#[tauri::command]
async fn remove_key(app: AppHandle, pubky_str: &str) -> Result<(), BackupAppError> {
    let pubky = parse_pubky_for_command(pubky_str)?;
    get_manager(&app).await?.remove_key(&pubky).await?;
    debug!("Key removed: {}", pubky);
    Ok(())
}

/// Delete a key from the backup manager and remove all backed-up data.
#[tauri::command]
async fn delete_key(app: AppHandle, pubky_str: &str) -> Result<(), BackupAppError> {
    let pubky = parse_pubky_for_command(pubky_str)?;
    get_manager(&app).await?.delete_key(&pubky).await?;
    debug!("Key deleted: {}", pubky);
    Ok(())
}

/// Start signing in to a key, so that its private data is backed up as well.
///
/// The link to show to the key's owner, and later the outcome, arrive with the
/// key's state in `key-update` events.
#[tauri::command]
async fn start_sign_in(app: AppHandle, pubky_str: &str) -> Result<(), BackupAppError> {
    let pubky = parse_pubky_for_command(pubky_str)?;
    get_manager(&app).await?.start_sign_in(&pubky).await?;
    debug!("Sign-in started for: {}", pubky);
    Ok(())
}

/// Sign in to whichever key approves the sign-in, backing it up if it isn't yet.
///
/// Sends the link to show to the key's owner through `on_authorization_url`
/// right away, then returns the key once the owner approves.
#[tauri::command]
async fn sign_in_new_key(
    app: AppHandle,
    on_authorization_url: Channel<String>,
) -> Result<String, BackupAppError> {
    let manager = get_manager(&app).await?;
    let request = manager.start_sign_in_for_new_key().await?;
    on_authorization_url
        .send(request.authorization_url().to_string())
        .map_err(BackupAppError::internal)?;

    let pubky = request.approved().await?;
    manager.write_last_pubky(&pubky)?;
    info!("Signed in to and backing up: {}", pubky);
    Ok(pubky.z32())
}

/// Cancel the sign-in started by `sign_in_new_key`, if it awaits approval.
#[tauri::command]
async fn cancel_sign_in_new_key(app: AppHandle) -> Result<(), BackupAppError> {
    get_manager(&app).await?.cancel_sign_in_for_new_key();
    Ok(())
}

/// Cancel a sign-in that is still awaiting approval.
#[tauri::command]
async fn cancel_sign_in(app: AppHandle, pubky_str: &str) -> Result<(), BackupAppError> {
    let pubky = parse_pubky_for_command(pubky_str)?;
    get_manager(&app).await?.cancel_sign_in(&pubky).await?;
    Ok(())
}

/// Sign out of a key, so that only its public data is backed up.
#[tauri::command]
async fn sign_out(app: AppHandle, pubky_str: &str) -> Result<(), BackupAppError> {
    let pubky = parse_pubky_for_command(pubky_str)?;
    get_manager(&app).await?.sign_out(&pubky).await?;
    debug!("Signed out of: {}", pubky);
    Ok(())
}

/// Start mirroring a key to a second homeserver, so that it holds the same data.
///
/// Returns the link to show to the key's owner. The outcome arrives with the
/// key's state in a `key-update` event.
#[tauri::command]
async fn start_mirror(
    app: AppHandle,
    pubky_str: &str,
    homeserver_str: &str,
) -> Result<String, BackupAppError> {
    let pubky = parse_pubky_for_command(pubky_str)?;
    let homeserver =
        parse_pubky(homeserver_str).map_err(|e| BackupAppError::InvalidPubkyFormat {
            message: format!("Invalid homeserver key: {}", e),
        })?;
    let request = get_manager(&app)
        .await?
        .start_mirror(&pubky, &homeserver)
        .await?;
    debug!("Mirror requested for {} to {}", pubky, homeserver);
    Ok(request.authorization_url().to_string())
}

/// Cancel a mirror request that is still awaiting approval.
#[tauri::command]
async fn cancel_mirror(app: AppHandle, pubky_str: &str) -> Result<(), BackupAppError> {
    let pubky = parse_pubky_for_command(pubky_str)?;
    get_manager(&app).await?.cancel_mirror(&pubky).await?;
    Ok(())
}

/// Stop mirroring a key. What was copied to the mirror stays there.
#[tauri::command]
async fn stop_mirror(app: AppHandle, pubky_str: &str) -> Result<(), BackupAppError> {
    let pubky = parse_pubky_for_command(pubky_str)?;
    get_manager(&app).await?.stop_mirror(&pubky).await?;
    debug!("Mirror stopped for: {}", pubky);
    Ok(())
}

/// Send backup controller task ForceSync message.
#[tauri::command]
async fn force_sync_now(app: AppHandle, pubky_str: &str) -> Result<(), BackupAppError> {
    let pubky = parse_pubky_for_command(pubky_str)?;
    get_manager(&app).await?.force_sync(&pubky).await?;
    debug!("Force sync triggered for: {}", pubky);
    Ok(())
}

/// Open the keys directory in the system file manager
#[tauri::command]
async fn open_data_dir(app: AppHandle) -> Result<(), BackupAppError> {
    use tauri_plugin_opener::OpenerExt;

    let keys_dir = get_manager(&app).await?.keys_dir().to_path_buf();
    app.opener()
        .reveal_item_in_dir(keys_dir)
        .map_err(BackupAppError::internal)
}

/// Open the snapshots directory for a specific key in the system file manager
#[tauri::command]
async fn open_snapshots_dir(app: AppHandle, pubky_str: &str) -> Result<(), BackupAppError> {
    use tauri_plugin_opener::OpenerExt;

    let pubky = parse_pubky_for_command(pubky_str)?;
    let snapshots_dir = get_manager(&app).await?.snapshots_dir(&pubky);
    app.opener()
        .open_path(snapshots_dir.to_string_lossy(), None::<&str>)
        .map_err(BackupAppError::internal)
}

/// Set the sync interval in seconds. Controllers pick up the new value dynamically.
#[tauri::command]
async fn set_sync_interval(app: AppHandle, interval_secs: u64) -> Result<(), BackupAppError> {
    get_manager(&app).await?.set_sync_interval(interval_secs)?;
    Ok(())
}

/// Create a snapshot (zip archive) of the specified pubky's backed-up data
#[tauri::command]
async fn create_snapshot(app: AppHandle, pubky_str: &str) -> Result<String, BackupAppError> {
    let pubky = parse_pubky_for_command(pubky_str)?;
    let path = get_manager(&app).await?.create_snapshot(&pubky).await?;
    Ok(path.to_string_lossy().to_string())
}

/// Get recent activity entries for a key.
#[tauri::command]
async fn get_activity(
    app: AppHandle,
    pubky_str: &str,
    limit: Option<usize>,
) -> Result<Vec<ActivityEntry>, BackupAppError> {
    const DEFAULT_ACTIVITY_LIMIT: usize = 100;
    let pubky = parse_pubky_for_command(pubky_str)?;
    let manager = get_manager(&app).await?;
    Ok(manager
        .get_activity(&pubky, limit.unwrap_or(DEFAULT_ACTIVITY_LIMIT))
        .await)
}

/// Move backup data to a new location.
///
/// Shuts down all controllers, moves the keys directory, then creates a new
/// manager from the new location. Other commands wait for it meanwhile.
#[tauri::command]
async fn set_backup_location(app: AppHandle, new_parent: &str) -> Result<String, BackupAppError> {
    let new_parent = PathBuf::from(new_parent);
    let state = app.state::<AppState>();
    let mut slot = state.manager.write().await;

    // The move shuts the manager down, so it is given up before anything else
    // can happen: if the move fails, the next command creates a fresh manager
    let manager = match slot.take() {
        Some(manager) => manager,
        None => Arc::new(create_manager().await?),
    };

    let new_keys_dir = manager.move_keys(&new_parent).await?;

    let new_manager = Arc::new(create_manager().await?);
    spawn_event_listener(&app, &state, &new_manager).await;
    *slot = Some(new_manager);

    info!("Backup location moved to {}", new_keys_dir.display());
    Ok(new_keys_dir.to_string_lossy().to_string())
}

/// Show the aggregate status of all keys in the tray icon.
fn update_tray_icon(app: &AppHandle, manager: &BackupManager) {
    let Some(tray) = app.tray_by_id("main") else {
        return;
    };

    let all_states = manager.get_all_key_states();
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
        ("Pubky Backup - Syncing...", "🔄")
    } else if has_starting {
        ("Pubky Backup - Starting...", "⏳")
    } else if has_error {
        ("Pubky Backup - Error", "❌")
    } else {
        ("Pubky Backup - Synced", "✅")
    };

    let _ = tray.set_tooltip(Some(tooltip));
    if let Err(e) = tray.set_title(Some(title)) {
        error!("Failed to update tray title: {}", e);
    }
}

pub fn run() {
    env_logger::init();

    if is_testnet_mode() {
        info!("Testnet mode enabled via PUBKY_TESTNET environment variable");
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
            app.manage(AppState::default());

            // Create the manager in the background, so the window shows right away
            let app_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) = get_manager(&app_handle).await {
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
            start_sign_in,
            cancel_sign_in,
            sign_in_new_key,
            cancel_sign_in_new_key,
            sign_out,
            start_mirror,
            cancel_mirror,
            stop_mirror,
            open_data_dir,
            open_snapshots_dir,
            create_snapshot,
            get_activity,
            set_sync_interval,
            set_backup_location
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
