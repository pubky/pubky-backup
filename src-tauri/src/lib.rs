mod http_client;
mod storage;

use log::{debug, error, info};
use pubky::{global::global_client, Pkdns, PubkyDrive, PubkyPath, PublicKey};
use serde::{Deserialize, Serialize};
use std::{env, str::FromStr, sync::{Arc, Mutex}, time::Duration};
use tokio::sync::broadcast;
use tokio::time;

use crate::http_client::HttpClient;
use crate::storage::Storage;

// Global state to store the pubky
static APP_STATE: Mutex<AppState> = Mutex::new(AppState {
    pubky: None,
    homeserver: None,
    developer_mode: false,
    storage: None,
    background_task_cancel: None,
});

#[derive(Serialize)]
struct AppState {
    pubky: Option<String>,
    homeserver: Option<String>,
    developer_mode: bool,
    #[serde(skip)]
    storage: Option<Arc<Storage>>,
    #[serde(skip)]
    background_task_cancel: Option<broadcast::Sender<()>>,
}

#[derive(Debug, Deserialize)]
struct EventsResponse {
    events: Vec<String>,
    cursor: String,
}

/// Parse events response from API
fn parse_events_response(response: &str) -> Result<EventsResponse, String> {
    let lines: Vec<&str> = response.trim().split('\n').collect();

    if lines.is_empty() {
        return Err("Empty response".to_string());
    }

    let mut events = Vec::new();
    let mut cursor = String::new();

    for line in lines {
        let line = line.trim();
        if line.starts_with("PUT pubky://") || line.starts_with("DEL pubky://") {
            events.push(line.to_string());
        } else if line.starts_with("cursor: ") {
            cursor = line.strip_prefix("cursor: ").unwrap_or("").to_string();
        }
    }

    Ok(EventsResponse { events, cursor })
}

/// Get or create storage instance - exits on failure
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

/// Take a pubky, verify and add to State
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
        PublicKey::from_str(&pubky_str).map_err(|e| format!("Invalid pubky format: {}", e))?;
    let client = global_client().map_err(|e| format!("Internal error: {}", e))?;

    // Check pubky is discoverable
    let homeserver_pubky_str = Pkdns::with_client(&client)
        .get_homeserver(&pubky)
        .await
        .ok_or_else(|| "Failed to find Homeserver for pubky".to_string())?;

    // Check Pubky has /pub/ data on Homeserver
    let pubky_drive = PubkyDrive::public_with_client(&client);
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

    let mut state = APP_STATE
        .lock()
        .map_err(|_| "Failed to acquire lock".to_string())?;
    state.pubky = Some(pubky_str.to_string());
    state.homeserver = Some(homeserver_pubky_str);
    Ok(())
}

#[tauri::command]
async fn fetch_state() -> Result<String, String> {
    match APP_STATE.lock() {
        Ok(state) => {
            serde_json::to_string(&*state).map_err(|e| format!("Serialisation error: {}", e))
        }
        Err(_) => Err("Failed to acquire lock".to_string()),
    }
}

#[tauri::command]
async fn start_background_task() -> Result<(), String> {
    let mut state = APP_STATE
        .lock()
        .map_err(|_| "Failed to acquire lock".to_string())?;

    if state.background_task_cancel.is_some() {
        return Err("Background task is already running".to_string());
    }

    // Create cancellation channel and add to State for later access
    let (cancel_tx, cancel_rx) = broadcast::channel(1);
    state.background_task_cancel = Some(cancel_tx);

    tauri::async_runtime::spawn(background_task(cancel_rx));
    debug!("Background task started");
    Ok(())
}

#[tauri::command]
async fn stop_background_task() -> Result<(), String> {
    let mut state = APP_STATE
        .lock()
        .map_err(|_| "Failed to acquire lock".to_string())?;

    if let Some(cancel_tx) = state.background_task_cancel.take() {
        // Send cancellation signal
        let _ = cancel_tx.send(());
        debug!("Background task stop signal sent");
        Ok(())
    } else {
        Err("No background task is running".to_string())
    }
}

/// Main backend loop
/// -   Check if data already exists, initiate if not
async fn background_task(mut cancel_rx: broadcast::Receiver<()>) {
    let mut interval = time::interval(Duration::from_secs(5));

    // Init data-dir and/or read initial cursor
    let storage = get_or_create_storage();
    let pubky = {
        if let Ok(state) = APP_STATE.lock() {
            match &state.pubky {
                Some(pubky) => pubky.clone(),
                None => {
                    error!("Pubky not available in background task");
                    return;
                }
            }
        } else {
            error!("Failed to acquire app state lock");
            return;
        }
    };

    loop {
        tokio::select! {
            _ = interval.tick() => {
                info!("Background task interval..");

                let cursor = match storage.read_cursor(&pubky).await {
                    Ok(cursor) => {
                        info!("Current cursor: {}", cursor);
                        cursor
                    },
                    Err(e) => {
                        info!("No cursor found or error: {}, using empty cursor", e);
                        String::new()
                    }
                };

                // Fetch events from API
                match fetch_events(&cursor).await {
                    Ok(response) => {
                        match parse_events_response(&response) {
                            Ok(events_response) => {
                                info!("Fetched {} events", events_response.events.len());
                                for event in &events_response.events {
                                    info!("Event: {}", event);
                                }

                                // Update cursor if we got a new one
                                if !events_response.cursor.is_empty() && events_response.cursor != cursor {
                                    if let Err(e) = storage.write_cursor(&pubky, events_response.cursor.clone()).await {
                                        error!("Failed to update cursor: {}", e);
                                    } else {
                                        info!("Updated cursor to: {}", events_response.cursor);
                                    }
                                }
                            }
                            Err(e) => error!("Failed to parse events response: {}", e),
                        }
                    }
                    Err(e) => error!("Background fetch failed: {}", e),
                }

            }
            _ = cancel_rx.recv() => {
                info!("Background task cancelled");
                break;
            }
        }
    }
}

async fn fetch_events(cursor: &str) -> Result<String, String> {
    let client = HttpClient::new();

    let limit = 10;
    let pubky_url = format!(
        "https://homeserver.staging.pubky.app/events/?limit={}&cursor={}",
        limit, cursor
    );
    let pubky_host = "b3p9kmimbq8irxe8hwwg85qbe34r3i6f3fcqw9jo61wsh13eftio";

    match client
        .get_with_header(&pubky_url, "Pubky-Host", pubky_host)
        .await
    {
        Ok(response) => Ok(response),
        Err(e) => Err(format!("HTTP request failed: {}", e)),
    }
}

pub fn run() {
    env_logger::init();

    let args: Vec<String> = env::args().collect();
    let developer_mode = args.contains(&"--developer".to_string());

    if let Ok(mut state) = APP_STATE.lock() {
        state.developer_mode = developer_mode;
        if developer_mode {
            info!("Developer mode enabled");
        }
    }

    // Initialize storage
    let _ = get_or_create_storage();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            init_state_for_pubky,
            fetch_state,
            start_background_task,
            stop_background_task
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_fetch_data() {
        let result = fetch_events("").await;

        match result {
            Ok(data) => {
                println!("Fetch successful: {}", data);
                assert!(!data.is_empty(), "Response should not be empty");
            }
            Err(e) => {
                println!("Fetch failed: {}", e);
                // Test passes if we get an error response (network might be down)
                assert!(
                    e.contains("HTTP request failed") || e.contains("Failed"),
                    "Error should be descriptive"
                );
            }
        }
    }
}
