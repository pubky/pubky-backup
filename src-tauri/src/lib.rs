mod http_client;

use pubky::{global::global_client, Pkdns, PubkyDrive, PubkyPath, PublicKey};
use serde::Serialize;
use std::{str::FromStr, sync::Mutex};

// Global state to store the pubky
static APP_STATE: Mutex<AppState> = Mutex::new(AppState { pubky: None, homeserver: None });

#[derive(Serialize)]
struct AppState {
    pubky: Option<String>,
    homeserver: Option<String>,
}

#[tauri::command]
async fn store_pubky(pubky_str: &str) -> Result<(), String> {

    let pubky = PublicKey::from_str(&pubky_str).map_err(|e| format!("Invalid pubky format: {}", e))?;
    let client = global_client().map_err(|e| format!("Internal error: {}", e))?;
    
    // Check pubky is discoverable
    let homeserver_pubky_str = Pkdns::with_client(&client).get_homeserver(&pubky).await
        .ok_or_else(|| "Failed to find Homeserver for pubky".to_string())?;
    
    // Check Pubky has /pub/ data on Homeserver
    let pubky_drive = PubkyDrive::public_with_client(&client);
    let path = PubkyPath::new(Some(pubky.clone()), "/pub/").map_err(|e| format!("Internal error: {}", e))?;
    // TODO: We should check the pub key has data with this, but currently incorrectly returns 401
    // let exists = match pubky_drive.exists(path.clone()).await {
        //     Ok(p) => p,
        //     Err(e) => return Err(format!("Failed to find data for pubky"))
        // };
        
    // Instead for now we can call `get` on the base pub path which will pull the urls of every item which the key has published.
    if let Err(e) = pubky_drive.get(path).await {
        println!("Error: {}", e);
        return Err(format!("Failed to find data for pubky"))
    }

    match APP_STATE.lock() {
        Ok(mut state) => {
            state.pubky = Some(pubky_str.to_string());
            state.homeserver = Some(homeserver_pubky_str);
            Ok(())
        }
        Err(_) => Err("Failed to acquire lock".to_string())
    }
}

#[tauri::command]
async fn fetch_state() -> Result<String, String> {
    match APP_STATE.lock() {
        Ok(state) => {
            serde_json::to_string(&*state).map_err(|e| format!("Serialisation error: {}", e))
        }
        Err(_) => Err("Failed to acquire lock".to_string())
    }
}



#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![store_pubky, fetch_state])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
