mod events;
mod storage;
mod utils;

use anyhow::{anyhow, Result};
use log::{debug, error, info, warn};
use pubky::{Pkdns, PubkyResource, PublicKey, PublicStorage};
use serde::Serialize;

use std::{
    env,
    ops::ControlFlow,
    str::FromStr,
    sync::{Arc, Mutex},
    time::Duration,
};
use tauri::{
    menu::{MenuBuilder, MenuItemBuilder},
    tray::TrayIconBuilder,
    AppHandle, Manager,
};
use tokio::sync::broadcast;
use tokio::time;

use crate::events::{fetch_events, EventInfo};
use crate::storage::Storage;
use crate::utils::retry_with_backoff;

const SYNC_INTERVAL_SECONDS: u64 = 30;

/// Custom error types. Only these should be exposed to the front-end.
#[derive(thiserror::Error, Debug)]
pub enum BackupAppError {
    #[error("Internal error: {0}")]
    Internal(#[from] anyhow::Error),
    #[error("Failed to find Homeserver for pubky")]
    HomeserverNotFound,
    #[error("Failed to find data for pubky")]
    DataNotFound,
    #[error("Invalid pubky format: {0}")]
    InvalidPubkyFormat(String),
}

impl BackupAppError {
    pub fn internal<E: Into<anyhow::Error>>(err: E) -> Self {
        Self::Internal(err.into())
    }

    pub fn lock_failed() -> Self {
        Self::Internal(anyhow::anyhow!("Failed to acquire lock"))
    }
}

impl From<BackupAppError> for String {
    fn from(err: BackupAppError) -> String {
        err.to_string()
    }
}

#[derive(Debug, Clone)]
enum BackupControllerMessage {
    Cancel,
    ForceSync,
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
    backup_control_tx: None,
    app_handle: None,
});

/// AppState is Tauri's Rust back-end State.
/// Here we provide an interface for the front-end and manage other application tasks (eg. The Backup task)
#[derive(Serialize, Clone)]
pub struct AppState {
    /// This session's pubky
    pubky: Option<String>,
    /// This session's pubky's homeserver. Stored only for displaying in GUI.
    homeserver: Option<String>,
    /// Dev mode is for working on the front-end - doesnt make network calls and populates with mock data.
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
    storage: Option<Arc<Storage>>,
    #[serde(skip)]
    backup_control_tx: Option<broadcast::Sender<BackupControllerMessage>>,
    #[serde(skip)]
    app_handle: Option<AppHandle>,
}

fn get_or_create_storage() -> Result<Arc<Storage>> {
    let mut state = APP_STATE
        .lock()
        .map_err(|_| anyhow!(BackupAppError::lock_failed()))?;

    if state.storage.is_none() {
        let storage = Storage::new().map_err(|e| anyhow!("Failed to create storage: {}", e))?;
        state.storage = Some(Arc::new(storage));
    }

    Ok(state.storage.clone().unwrap())
}

/// Take a pubky, verify and add to State ready for usage.
#[tauri::command]
async fn init_app_state(pubky_str: &str) -> Result<(), String> {
    if let Ok(mut state) = APP_STATE.lock() {
        if state.developer_mode {
            state.pubky = Some("g1b6wp8bhhxtsksy3td7rj6mgg7s5k8c68663sajkfscshwj8g5y".to_string());
            state.homeserver =
                Some("ufibwbmed6jeq9k4p583go95wofakh9fwpp4k734trq79pd9u1uy".to_string());
            return Ok(());
        }
    }

    let pubky = PublicKey::from_str(pubky_str)
        .map_err(|e| BackupAppError::InvalidPubkyFormat(e.to_string()))?;

    // Check pubky is discoverable
    let homeserver_pubky_str = Pkdns::new()
        .map_err(BackupAppError::internal)?
        .get_homeserver_of(&pubky)
        .await
        .ok_or_else(|| BackupAppError::HomeserverNotFound)?;

    // Check Pubky has /pub/ data on Homeserver
    let pubky_storage = PublicStorage::new().map_err(BackupAppError::internal)?;

    let path = PubkyResource::new(pubky.clone(), "/pub/").map_err(BackupAppError::internal)?;

    // TODO: We should check the pub key has data with exists(), but currently incorrectly returns 401 (https://github.com/pubky/pubky-core/issues/236)
    // if !pubky_storage.exists(path.clone()).await.map_err(|e| format!("Internal error: {}", e))? {
    //     return Err(format!("Failed to find data for pubky"));
    // }

    // Instead for now we can call `get` on the base pub path which will pull the urls of every item which the key has published.
    if (pubky_storage.get(path).await).is_err() {
        return Err(BackupAppError::DataNotFound.into());
    }
    info!("Pubky is valid for Backup: {}", pubky);

    // scope block lock for implicit drop
    {
        let mut state = APP_STATE
            .lock()
            .map_err(|_| BackupAppError::lock_failed())?;
        state.pubky = Some(pubky.to_string());
        state.homeserver = Some(homeserver_pubky_str);
    }

    // Save the last used pubky to storage (non-critical operation)
    get_or_create_storage()
        .map_err(BackupAppError::internal)?
        .write_last_pubky(pubky.to_string())
        .await
        .map_err(BackupAppError::internal)?;

    Ok(())
}

/// Fetch application state for usage in front-end
#[tauri::command]
async fn fetch_state() -> Result<AppState, String> {
    match APP_STATE.lock() {
        Ok(state) => Ok(state.clone()),
        Err(_) => Err(BackupAppError::lock_failed().into()),
    }
}

/// Get list of previously used pubky keys that have data stored
#[tauri::command]
async fn get_previous_pubky_keys() -> Result<Vec<String>, String> {
    let storage = match get_or_create_storage() {
        Ok(storage) => storage,
        Err(e) => {
            return Err(BackupAppError::internal(e).into());
        }
    };

    // List directories in the data directory to find existing pubky keys
    match storage.list_pubky_directories().await {
        Ok(keys) => Ok(keys),
        Err(e) => Err(BackupAppError::internal(e).into()),
    }
}

/// Get the last used pubky from storage
#[tauri::command]
async fn get_last_pubky() -> Result<Option<String>, String> {
    let storage = match get_or_create_storage() {
        Ok(storage) => storage,
        Err(e) => {
            return Err(BackupAppError::internal(e).into());
        }
    };

    match storage.read_last_pubky().await {
        Ok(pubky) => Ok(pubky),
        Err(e) => Err(BackupAppError::internal(e).into()),
    }
}

/// Spawn task for downloads and polling.
/// To be called by front-end upon entering main screen.
#[tauri::command]
async fn backup_controller_begin() -> Result<(), String> {
    let mut state = APP_STATE
        .lock()
        .map_err(|_| BackupAppError::lock_failed())?;

    let pubky = state
        .pubky
        .clone()
        .ok_or_else(|| BackupAppError::internal(anyhow!("Pubky not available in AppState")))?;

    let (backup_control_tx, backup_control_rx) = broadcast::channel(5);
    state.backup_control_tx = Some(backup_control_tx);

    tauri::async_runtime::spawn(backup_controller(pubky, Some(backup_control_rx)));
    info!("Backup controller task started");
    Ok(())
}

/// Send backup controller task Cancel message.
/// To be controlled by front-end on exiting main screen.
#[tauri::command]
async fn backup_controller_close() -> Result<(), String> {
    let mut state = APP_STATE
        .lock()
        .map_err(|_| BackupAppError::lock_failed())?;
    state.backup_controller_error = None;

    if let Some(control_tx) = state.backup_control_tx.take() {
        let _ = control_tx.send(BackupControllerMessage::Cancel);
        debug!("Backup controller task stop signal sent");
        Ok(())
    } else {
        Err(BackupAppError::internal(anyhow!("No Backup controller task running")).into())
    }
}

/// Send backup controller task ForceSync message.
#[tauri::command]
async fn force_sync_now() -> Result<(), String> {
    let state = APP_STATE
        .lock()
        .map_err(|_| BackupAppError::lock_failed())?;

    if let Some(control_tx) = &state.backup_control_tx {
        match control_tx.send(BackupControllerMessage::ForceSync) {
            Ok(_) => {
                debug!("Force sync signal sent");
                Ok(())
            }
            Err(_) => {
                Err(BackupAppError::internal(anyhow!("Failed to send force sync signal")).into())
            }
        }
    } else {
        Err(BackupAppError::internal(anyhow!("No Backup controller task running")).into())
    }
}

/// Main backup task controller:
///     1) Take a Public Key
///     2) Fetch and store all public data
///
/// Currently spins up a single async task which pulls batches of /events/ and processes them immediately.
/// Once all events have been processed it polls for more events every SYNC_INTERVAL_SECONDS.
/// Optionally takes a broadcast channel receiver for control operations (Cancel, ForceSync)
///
/// TODO: De-couple from AppState. Ie remove state read and writes from this logic
async fn backup_controller(
    pubky: String,
    mut control_rx: Option<broadcast::Receiver<BackupControllerMessage>>,
) {
    let mut interval = time::interval(Duration::from_secs(SYNC_INTERVAL_SECONDS));

    // Init data-dir and/or read initial cursor
    let storage = match get_or_create_storage() {
        Ok(storage) => storage,
        Err(e) => {
            error!("Failed to initialize storage: {}", e);
            return;
        }
    };

    // Calculate initial data-dir size for this pubky
    let data_dir_size = storage.calculate_pubky_size(&pubky).await;
    if let Ok(mut state) = APP_STATE.lock() {
        state.data_dir_size = data_dir_size;
    }

    loop {
        tokio::select! {
            _ = interval.tick() => {
                // Do not attempt sync if currently syncing
                if let Ok(state) = APP_STATE.lock() {
                    if state.is_syncing {
                        continue;
                    }
                } else {
                    error!("Failed to acquire app state lock");
                    return;
                };

                set_sync_status(true);

                match perform_sync_batch(&storage, &pubky).await {
                    Ok(ControlFlow::Continue(())) => {
                        // More events available, keep syncing immediately
                        interval = time::interval_at(
                            time::Instant::now(),
                            Duration::from_secs(SYNC_INTERVAL_SECONDS)
                        );
                    }
                    Ok(ControlFlow::Break(())) => {
                        // Sync complete
                    }
                    Err(e) => {
                        error!("Critical sync batch failure - terminating backup controller: {}", e);
                        set_sync_status(false);

                        // Set error message so frontend can detect failure and show alert
                        if let Ok(mut state) = APP_STATE.lock() {
                            state.backup_control_tx = None;
                            state.backup_controller_error = Some(format!("Critical sync batch failure: {}", e));
                        }
                        return;
                    }
                }

                set_sync_status(false);
                if let Ok(mut state) = APP_STATE.lock() {
                    state.next_sync_time = next_sync_time();
                }
            }
            msg = async {
                if let Some(ref mut rx) = control_rx {
                    rx.recv().await
                } else {
                    std::future::pending().await
                }
            } => {
                match msg {
                    Ok(BackupControllerMessage::Cancel) => {
                        info!("Backup controller task cancelled");
                        set_sync_status(false);
                        break;
                    }
                    Ok(BackupControllerMessage::ForceSync) => {
                        info!("Force sync triggered");
                        // Reset interval to trigger immediately
                        interval = time::interval_at(
                            time::Instant::now(),
                            Duration::from_secs(SYNC_INTERVAL_SECONDS)
                        );
                    }
                    Err(e) => {
                        warn!("Backup controller task closed: {}", e);
                        break;
                    }
                }
            }
        }
    }
}

/// Process one batch of sync events
/// Returns Ok(Continue) if more events are available, Ok(Break) if sync is complete, Err on failure
async fn perform_sync_batch(storage: &Arc<Storage>, pubky: &str) -> Result<ControlFlow<(), ()>> {
    let cursor = storage.read_cursor(pubky).await?;

    match fetch_events(&cursor, pubky).await {
        Ok(events_response) => {
            let num_events = events_response.events.len();
            info!("Fetched {} events", num_events);

            if num_events > 0 {
                // Process those events
                process_events(events_response.events()?, pubky, storage.clone()).await?;

                // Store new cursor
                storage
                    .write_cursor(pubky, events_response.cursor.clone())
                    .await?;

                // Calculate and store the data-dir size for this pubky after a batch processed
                let size = storage.calculate_pubky_size(pubky).await;
                if let Ok(mut state) = APP_STATE.lock() {
                    state.data_dir_size = size;
                }

                Ok(ControlFlow::Continue(()))
            } else {
                Ok(ControlFlow::Break(()))
            }
        }
        Err(e) => {
            error!("Sync events fetch failed: {}", e);
            storage
                .write_error("/events/", &format!("Fetch failed: {}", e))
                .await?;
            Err(e)
        }
    }
}

/// Take a list of events and store the data of those which belong to a given pubky
async fn process_events(events: Vec<EventInfo>, pubky: &str, storage: Arc<Storage>) -> Result<()> {
    for event_info in events {
        // Skip events for other pubkys
        // TODO: Filter server-side
        let resource = match PubkyResource::from_str(&event_info.url) {
            Ok(resource) => resource,
            Err(e) => {
                let _ = storage
                    .write_error(&event_info.url, &format!("Invalid URL path: {}", e))
                    .await;
                continue;
            }
        };
        if resource.owner.to_string() != pubky {
            continue;
        }

        match event_info.operation.as_str() {
            "PUT" => {
                debug!("Processing PUT event for: {}", event_info.url);
                match fetch_data_for_url(&event_info.url).await {
                    Ok(data_vec) => {
                        // Skip storing empty data (404 responses)
                        if !data_vec.is_empty() {
                            storage.write(&event_info.url, data_vec).await?;
                        }
                    }
                    Err(e) => {
                        // Log fetch errors and continue processing other events
                        storage
                            .write_error(&event_info.url, &format!("Fetch failed: {}", e))
                            .await?;
                        warn!("Failed to fetch data for {}: {}", event_info.url, e);
                    }
                }
            }
            "DEL" => {
                debug!("Processing DEL event for: {}", event_info.url);
                storage.delete(&event_info.url).await?;
            }
            _ => {
                return Err(anyhow!("Unknown event operation: {}", event_info.operation));
            }
        }
    }

    Ok(())
}

/// Fetch data for a URL, either from network or mock data
/// TODO: Check if retry logic built-in to PubkyHttpClient
async fn fetch_data_for_url(url: &str) -> Result<Vec<u8>> {
    if crate::APP_STATE
        .lock()
        .map_err(|_| anyhow!(BackupAppError::lock_failed()))?
        .developer_mode
    {
        return Ok(get_mock_data_for_url(url));
    }

    let response = match retry_with_backoff(|| async {
        PublicStorage::new()
            .map_err(BackupAppError::internal)?
            .get(url)
            .await
            .map_err(|e| anyhow!("{}", e))
    })
    .await
    {
        Ok(response) => response,
        Err(e) => {
            // TODO: Is it correct that 404s are returned as Error rather than Ok response with status = 404?
            let error_str = e.to_string();
            if error_str.contains("404") || error_str.to_lowercase().contains("not found") {
                info!("404 response: Returning empty data for {}", url);
                return Ok(Vec::new());
            }
            return Err(anyhow!("Failed to fetch data for {}: {}", url, e));
        }
    };

    let data = response
        .bytes()
        .await
        .map_err(|e| anyhow!("Failed to read response bytes for {}: {}", url, e))?;

    let data_vec = data.to_vec();
    debug!("Successfully fetched data: {} bytes", data_vec.len());
    Ok(data_vec)
}

/// Generate mock data for a given pubky URL in developer mode
fn get_mock_data_for_url(url: &str) -> Vec<u8> {
    if url.contains("/profile") {
        r#"{"name":"Mock User","bio":"This is mock profile data for development","avatar":"https://example.com/avatar.jpg"}"#.as_bytes().to_vec()
    } else if url.contains("/posts/") {
        let post_id = url.split('/').next_back().unwrap_or("unknown");
        format!(r#"{{"id":"{}","content":"This is mock post content for {}","timestamp":"2024-01-01T12:00:00Z","author":"Mock User"}}"#, post_id, post_id).as_bytes().to_vec()
    } else if url.contains("/follows") {
        r#"{"following":["pubky1","pubky2","pubky3"],"followers":["pubky4","pubky5"]}"#
            .as_bytes()
            .to_vec()
    } else {
        // Generic mock data
        format!(
            r#"{{"url":"{}","data":"Mock data for development","type":"generic"}}"#,
            url
        )
        .as_bytes()
        .to_vec()
    }
}

/// Update sync status in state and tray icon
fn set_sync_status(is_syncing: bool) {
    if let Ok(mut state) = APP_STATE.lock() {
        state.is_syncing = is_syncing;

        if let Some(app_handle) = &state.app_handle {
            if let Some(tray) = app_handle.tray_by_id("main") {
                // Update tooltip (doesnt seem to be visible on Ubuntu)
                let tooltip = if is_syncing {
                    "🔄 Pubky Backup - Syncing..."
                } else {
                    "✅ Pubky Backup - Synced"
                };
                let _ = tray.set_tooltip(Some(tooltip));

                let title = if is_syncing { "🔄" } else { "✅" };
                if let Err(e) = tray.set_title(Some(title)) {
                    error!("Failed to update tray title: {}", e);
                }
            }
        }
    }
}

pub fn run() {
    env_logger::init();

    let developer_mode = env::args()
        .collect::<Vec<String>>()
        .contains(&"--developer".to_string());

    if let Ok(mut state) = APP_STATE.lock() {
        state.developer_mode = developer_mode;
        if developer_mode {
            info!("Developer mode enabled");
        }
    }

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            // Store app handle for tray updates
            if let Ok(mut state) = APP_STATE.lock() {
                state.app_handle = Some(app.handle().clone());
            }
            let quit = MenuItemBuilder::with_id("quit", "Quit").build(app)?;
            let show = MenuItemBuilder::with_id("show", "Show").build(app)?;
            let hide = MenuItemBuilder::with_id("hide", "Hide").build(app)?;
            let menu = MenuBuilder::new(app)
                .items(&[&show, &hide, &quit])
                .build()?;

            let _tray = TrayIconBuilder::with_id("main")
                .menu(&menu)
                // TODO: Create logo
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
            force_sync_now
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::EventInfo;
    use std::sync::Once;
    use tempfile::TempDir;

    static INIT: Once = Once::new();

    fn setup_test_app_state() {
        INIT.call_once(|| {
            let mut state = APP_STATE.lock().unwrap();
            state.developer_mode = true;
        });
    }

    #[tokio::test]
    async fn test_process_events() {
        setup_test_app_state();

        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let temp_path = temp_dir.path().to_str().expect("Failed to get temp path");
        let storage =
            Arc::new(Storage::with_root(temp_path).expect("Failed to create test storage"));

        let test_pubky = "g1b6wp8bhhxtsksy3td7rj6mgg7s5k8c68663sajkfscshwj8g5y";
        let test_url_1 = format!("pubky://{}/pub/posts/001", test_pubky);
        let test_url_2 = format!("pubky://{}/pub/profile", test_pubky);
        let other_pubky_url = "pubky://other_pubky/pub/posts/001".to_string();

        let events = vec![
            EventInfo {
                operation: "PUT".to_string(),
                url: test_url_1.clone(),
            },
            EventInfo {
                operation: "PUT".to_string(),
                url: test_url_2.clone(),
            },
            EventInfo {
                operation: "PUT".to_string(),
                url: other_pubky_url.clone(), // This should be skipped (wrong pubky)
            },
            EventInfo {
                operation: "DEL".to_string(),
                url: test_url_1.clone(), // Delete the first URL
            },
        ];

        let result = process_events(events, test_pubky, storage.clone()).await;
        assert!(result.is_ok(), "process_events should succeed");

        // The first URL should have been deleted, so it shouldn't exist
        let read_result_1 = storage.read(&test_url_1).await;
        assert!(read_result_1.is_err());
        // The second URL should still exist (only PUT, no DEL)
        let read_result_2 = storage.read(&test_url_2).await;
        assert!(read_result_2.is_ok());
        // The other pubky URL should not exist (was filtered out)
        let read_result_other = storage.read(&other_pubky_url).await;
        assert!(read_result_other.is_err());

        // Test that unknown operations cause an error
        let test_url_unknown = format!("pubky://{}/pub/unknown", test_pubky);
        let unknown_events = vec![EventInfo {
            operation: "UNKNOWN".to_string(),
            url: test_url_unknown.clone(),
        }];

        let result = process_events(unknown_events, test_pubky, storage.clone()).await;
        assert!(
            result.is_err(),
            "process_events should fail on unknown operation"
        );
    }
}
