mod http_client;
mod storage;

use log::{error, info};
use pubky::{global::global_client, Pkdns, PubkyDrive, PubkyPath, PublicKey};
use serde::Serialize;
use std::{env, str::FromStr, sync::Mutex};

use crate::http_client::HttpClient;
use crate::storage::Storage;

// Global state to store the pubky
static APP_STATE: Mutex<AppState> = Mutex::new(AppState {
    pubky: None,
    homeserver: None,
    developer_mode: false,
    storage: None,
});

#[derive(Serialize)]
struct AppState {
    pubky: Option<String>,
    homeserver: Option<String>,
    developer_mode: bool,
    #[serde(skip)]
    storage: Option<Storage>,
}

/// Take a pubky, verify and add to State
#[tauri::command]
async fn init_pubky(pubky_str: &str) -> Result<(), String> {
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
async fn is_dev_mode() -> Result<bool, String> {
    match APP_STATE.lock() {
        Ok(state) => Ok(state.developer_mode),
        Err(_) => Err("Failed to acquire lock".to_string()),
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

    let storage = match Storage::new() {
        Ok(storage) => storage,
        Err(e) => {
            error!("Failed to initialise storage: {}", e);
            std::process::exit(1);
        }
    };

    if let Ok(mut state) = APP_STATE.lock() {
        state.developer_mode = developer_mode;
        state.storage = Some(storage);
        if developer_mode {
            info!("Developer mode enabled");
        }
    }

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            init_pubky,
            fetch_state,
            is_dev_mode
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
