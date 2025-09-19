mod events;
mod storage;

use log::{debug, error, info};
use pubky::{global::global_client, Pkdns, PubkyDrive, PubkyHttpClient, PubkyPath, PublicKey};
use serde::Serialize;
use std::{
    env,
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

const SYNC_INTERVAL_SECONDS: u64 = 30;

/// Get the next sync time (current time + sync interval)
fn next_sync_time() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        + SYNC_INTERVAL_SECONDS
}

pub static APP_STATE: Mutex<AppState> = Mutex::new(AppState {
    pubky: None,
    homeserver: None,
    developer_mode: false,
    is_syncing: true,
    next_sync_time: 0,
    data_dir_size: 0,
    storage: None,
    pubky_drive: None,
    worker_thread_cancel: None,
    force_sync_sender: None,
    app_handle: None,
    http_client: None,
});

#[derive(Serialize)]
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
    #[serde(skip)]
    storage: Option<Arc<Storage>>,
    #[serde(skip)]
    pubky_drive: Option<PubkyDrive>,
    #[serde(skip)]
    worker_thread_cancel: Option<broadcast::Sender<()>>,
    #[serde(skip)]
    force_sync_sender: Option<broadcast::Sender<()>>,
    #[serde(skip)]
    app_handle: Option<AppHandle>,
    #[serde(skip)]
    http_client: Option<PubkyHttpClient>,
}

pub fn get_or_create_http_client() -> PubkyHttpClient {
    let mut state = match APP_STATE.lock() {
        Ok(state) => state,
        Err(_) => {
            error!("Failed to acquire lock");
            std::process::exit(1);
        }
    };
    if state.http_client.is_none() {
        let client = match PubkyHttpClient::new() {
            Ok(client) => client,
            Err(e) => {
                error!("Failed to create HTTP client: {}", e);
                std::process::exit(1);
            }
        };
        state.http_client = Some(client);
    }
    state.http_client.as_ref().unwrap().clone()
}

fn get_or_create_storage() -> Arc<Storage> {
    let mut state = match APP_STATE.lock() {
        Ok(state) => state,
        Err(_) => {
            error!("Failed to acquire lock");
            std::process::exit(1);
        }
    };
    if state.storage.is_none() {
        let storage = match Storage::new() {
            Ok(storage) => storage,
            Err(e) => {
                error!("Failed to create storage: {}", e);
                std::process::exit(1);
            }
        };
        state.storage = Some(Arc::new(storage));
    }
    state.storage.clone().expect("Storage should be available")
}

fn get_or_create_pubky_drive() -> PubkyDrive {
    let mut state = match APP_STATE.lock() {
        Ok(state) => state,
        Err(_) => {
            error!("Failed to acquire lock");
            std::process::exit(1);
        }
    };
    if state.pubky_drive.is_none() {
        let client = match global_client() {
            Ok(client) => client,
            Err(e) => {
                error!("Failed to create client: {}", e);
                std::process::exit(1);
            }
        };
        let drive = PubkyDrive::public_with_client(&client);
        state.pubky_drive = Some(drive);
    }
    state
        .pubky_drive
        .clone()
        .expect("PubkyDrive should be available")
}

/// Take a pubky, verify and add to State ready for usage.
#[tauri::command]
async fn init_state_for_pubky(pubky_str: &str) -> Result<(), String> {
    // Check if developer mode is enabled and use mock data
    if let Ok(mut state) = APP_STATE.lock() {
        if state.developer_mode {
            state.pubky = Some("g1b6wp8bhhxtsksy3td7rj6mgg7s5k8c68663sajkfscshwj8g5y".to_string());
            state.homeserver =
                Some("ufibwbmed6jeq9k4p583go95wofakh9fwpp4k734trq79pd9u1uy".to_string());
            return Ok(());
        }
    }

    let pubky =
        PublicKey::from_str(pubky_str).map_err(|e| format!("Invalid pubky format: {}", e))?;
    let client = global_client().map_err(|e| format!("Internal error: {}", e))?;

    // Check pubky is discoverable
    let homeserver_pubky_str = Pkdns::with_client(&client)
        .get_homeserver(&pubky)
        .await
        .ok_or_else(|| "Failed to find Homeserver for pubky".to_string())?;

    // Check Pubky has /pub/ data on Homeserver
    let pubky_drive = get_or_create_pubky_drive();
    let path = PubkyPath::new(Some(pubky.clone()), "/pub/")
        .map_err(|e| format!("Internal error: {}", e))?;

    // TODO: We should check the pub key has data with exists(), but currently incorrectly returns 401 (https://github.com/pubky/pubky-core/issues/236)
    // if !pubky_drive.exists(path.clone()).await.map_err(|e| format!("Internal error: {}", e))? {
    //     return Err(format!("Failed to find data for pubky"));
    // }

    // Instead for now we can call `get` on the base pub path which will pull the urls of every item which the key has published.
    if let Err(e) = pubky_drive.get(path).await {
        error!("Failed to get pubky data: {}", e);
        return Err(format!("Failed to find data for pubky"));
    }
    info!("Pubky is valid for Backup: {}", pubky);

    let mut state = APP_STATE
        .lock()
        .map_err(|_| "Failed to acquire lock".to_string())?;
    state.pubky = Some(pubky.to_string());
    state.homeserver = Some(homeserver_pubky_str);
    Ok(())
}

/// Serialise data in State for usage in front-end
#[tauri::command]
async fn fetch_state() -> Result<String, String> {
    match APP_STATE.lock() {
        Ok(state) => {
            serde_json::to_string(&*state).map_err(|e| format!("Serialisation error: {}", e))
        }
        Err(_) => Err("Failed to acquire lock".to_string()),
    }
}

/// Spawn separate worker thread for downloads and polling.
/// To be called by front-end upon entering main screen.
#[tauri::command]
async fn worker_thread_begin() -> Result<(), String> {
    let mut state = APP_STATE
        .lock()
        .map_err(|_| "Failed to acquire lock".to_string())?;

    if state.worker_thread_cancel.is_some() {
        return Err("Worker thread is already running".to_string());
    }

    // Create cancellation and force-sync channels
    let (cancel_tx, cancel_rx) = broadcast::channel(1);
    let (force_sync_tx, force_sync_rx) = broadcast::channel(1);

    state.worker_thread_cancel = Some(cancel_tx);
    state.force_sync_sender = Some(force_sync_tx);

    tauri::async_runtime::spawn(worker_thread(cancel_rx, force_sync_rx));
    debug!("Worker thread started");
    Ok(())
}

/// Close worker thread if it exists.
/// To be controlled by front-end on exiting main screen.
#[tauri::command]
async fn worker_thread_close() -> Result<(), String> {
    let mut state = APP_STATE
        .lock()
        .map_err(|_| "Failed to acquire lock".to_string())?;

    if let Some(cancel_tx) = state.worker_thread_cancel.take() {
        // Send cancellation signal
        let _ = cancel_tx.send(());
        debug!("Worker thread stop signal sent");
        Ok(())
    } else {
        Err("No worker thread is running".to_string())
    }
}

/// Force immediate sync by skipping the current interval
#[tauri::command]
async fn force_sync_now() -> Result<(), String> {
    let state = APP_STATE
        .lock()
        .map_err(|_| "Failed to acquire lock".to_string())?;

    if let Some(force_sync_tx) = &state.force_sync_sender {
        match force_sync_tx.send(()) {
            Ok(_) => {
                debug!("Force sync signal sent");
                Ok(())
            }
            Err(_) => Err("Failed to send force sync signal".to_string()),
        }
    } else {
        Err("No worker thread is running".to_string())
    }
}

/// Backend worker for downloading and polling.
async fn worker_thread(
    mut cancel_rx: broadcast::Receiver<()>,
    mut force_sync_rx: broadcast::Receiver<()>,
) {
    let mut interval = time::interval(Duration::from_secs(SYNC_INTERVAL_SECONDS));

    // Init data-dir and/or read initial cursor
    let storage = get_or_create_storage();
    let pubky = {
        if let Ok(mut state) = APP_STATE.lock() {
            // Update next sync time
            state.next_sync_time = next_sync_time();

            let pubky = match &state.pubky {
                Some(pubky) => pubky.clone(),
                None => {
                    error!("Pubky not available in worker thread");
                    return;
                }
            };

            // Calculate and store initial data-dir size for this pubky
            state.data_dir_size = storage.calculate_pubky_size(&pubky);

            pubky
        } else {
            error!("Failed to acquire app state lock");
            return;
        }
    };

    loop {
        tokio::select! {
            _ = interval.tick() => {
                // Update next sync time for countdown
                if let Ok(mut state) = APP_STATE.lock() {
                    state.next_sync_time = next_sync_time();
                }
                perform_sync(&storage, &pubky).await;
            }
            _ = force_sync_rx.recv() => {
                info!("Force sync triggered");
                perform_sync(&storage, &pubky).await;

                // Reset the interval timer after force sync
                interval = time::interval(Duration::from_secs(SYNC_INTERVAL_SECONDS));
                if let Ok(mut state) = APP_STATE.lock() {
                    state.next_sync_time = next_sync_time();
                }
            }
            _ = cancel_rx.recv() => {
                info!("Worker thread cancelled");
                set_sync_status(false);
                break;
            }
        }
    }
}

/// Perform a sync operation
async fn perform_sync(storage: &Arc<Storage>, pubky: &str) {
    let mut cursor = storage.read_cursor(&pubky).await.unwrap_or_default();
    info!("Syncing at current cursor: {}", cursor);
    set_sync_status(true);

    // Keep fetching events until there are none left
    let mut has_more_events = true;
    while has_more_events {
        match fetch_events(&cursor, pubky).await {
            Ok(events_response) => {
                let num_events = events_response.events.len();
                info!("Fetched {} events", num_events);

                if num_events > 0 {
                    // Set Sync status

                    // Process those events
                    if let Err(e) = process_events(events_response.events(), pubky).await {
                        error!("Failed to process events: {}", e);
                        break;
                    }

                    // Store new cursor
                    if let Err(e) = storage
                        .write_cursor(pubky, events_response.cursor.clone())
                        .await
                    {
                        error!("Failed to update cursor: {}", e);
                        break;
                    }

                    // Calculate and store the data-dir size for this pubky after a bacth processed
                    let size = storage.calculate_pubky_size(pubky);
                    if let Ok(mut state) = APP_STATE.lock() {
                        state.data_dir_size = size;
                    }

                    // Update cursor for next iteration
                    cursor = events_response.cursor;
                } else {
                    // No more events, sync cycle completed
                    has_more_events = false;
                }
            }
            Err(e) => {
                error!("Sync fetch failed: {}", e);
                break;
            }
        }
    }

    info!("Sync completed");
    set_sync_status(false);
}

/// Take a list of events and store the data of those which belong to a given pubky
async fn process_events(events: Vec<EventInfo>, pubky: &str) -> Result<(), String> {
    let storage = get_or_create_storage();

    for event_info in events {
        // Skip events for other pubkys
        // TODO: Filter server-side
        if !event_info.url.contains(pubky) {
            continue;
        }
        match event_info.operation.as_str() {
            "PUT" => {
                debug!("Processing PUT event for: {}", event_info.url);
                let data_vec = match fetch_data_for_url(&event_info.url).await {
                    Ok(data) => data,
                    Err(e) => {
                        error!(
                            "Failed to fetch data for PUT event {}: {}",
                            event_info.url, e
                        );
                        continue;
                    }
                };

                if let Err(e) = storage.write(&event_info.url, data_vec).await {
                    error!("Failed to store data for {}: {}", event_info.url, e);
                }
            }
            "DEL" => {
                debug!("Processing DEL event for: {}", event_info.url);
                if let Err(e) = storage.delete(&event_info.url).await {
                    error!("Failed to delete data for {}: {}", event_info.url, e);
                }
            }
            _ => {
                error!("Unknown event operation: {}", event_info.operation);
            }
        }
    }

    Ok(())
}

/// Fetch data for a URL, either from network or mock data
async fn fetch_data_for_url(url: &str) -> Result<Vec<u8>, String> {
    if crate::APP_STATE
        .lock()
        .map_err(|_| "Failed to acquire app state lock".to_string())?
        .developer_mode
    {
        return Ok(get_mock_data_for_url(url));
    }

    let pubky_drive = get_or_create_pubky_drive();

    let response = pubky_drive
        .get(PubkyPath::from_str(url).map_err(|_| "Invalid pubky URL format".to_string())?)
        .await
        .map_err(|e| format!("Failed to fetch data: {}", e))?;

    let data = response
        .bytes()
        .await
        .map_err(|e| format!("Failed to read response bytes: {}", e))?;

    let data_vec = data.to_vec();
    debug!("Successfully fetched data: {} bytes", data_vec.len());
    Ok(data_vec)
}

/// Generate mock data for a given pubky URL in developer mode
fn get_mock_data_for_url(url: &str) -> Vec<u8> {
    if url.contains("/profile") {
        r#"{"name":"Mock User","bio":"This is mock profile data for development","avatar":"https://example.com/avatar.jpg"}"#.as_bytes().to_vec()
    } else if url.contains("/posts/") {
        let post_id = url.split('/').last().unwrap_or("unknown");
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
                        windows
                            .values()
                            .next()
                            .expect("sorry, no window found")
                            .set_focus()
                            .expect("can't Bring Window to Focus");
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
            init_state_for_pubky,
            fetch_state,
            worker_thread_begin,
            worker_thread_close,
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

    static INIT: Once = Once::new();

    fn setup_test_app_state() {
        INIT.call_once(|| {
            let mut state = APP_STATE.lock().unwrap();
            state.developer_mode = true;
            state.storage = Some(Arc::new(
                Storage::new().expect("Failed to create test storage"),
            ));
        });
    }

    #[tokio::test]
    async fn test_process_events() {
        setup_test_app_state();

        let test_pubky = "test_pubky_123";
        let test_url_1 = format!("pubky://{}/pub/posts/001", test_pubky);
        let test_url_2 = format!("pubky://{}/pub/profile", test_pubky);
        let test_url_unknown = format!("pubky://{}/pub/unknown", test_pubky);
        let other_pubky_url = "pubky://other_pubky/pub/posts/001";

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
                url: other_pubky_url.to_string(), // This should be skipped (wrong pubky)
            },
            EventInfo {
                operation: "DEL".to_string(),
                url: test_url_1.clone(), // Delete the first URL
            },
            EventInfo {
                operation: "UNKNOWN".to_string(),
                url: test_url_unknown.clone(), // Unknown operation
            },
        ];

        let result = process_events(events, test_pubky).await;
        assert!(result.is_ok(), "process_events should succeed");

        let storage = get_or_create_storage();
        // The first URL should have been deleted, so it shouldn't exist
        let read_result_1 = storage.read(&test_url_1).await;
        assert!(read_result_1.is_err());
        // The second URL should still exist (only PUT, no DEL)
        let read_result_2 = storage.read(&test_url_2).await;
        assert!(read_result_2.is_ok());
        // The other pubky URL should not exist (was filtered out)
        let read_result_other = storage.read(other_pubky_url).await;
        assert!(read_result_other.is_err());
        // The unknown operation URL should not exist (unknown operations ignored)
        let read_result_unknown = storage.read(&test_url_unknown).await;
        assert!(read_result_unknown.is_err());
    }
}
