mod http_client;

use http_client::HttpClient;
use pubky::{PubkyDrive, PubkyPath, PublicKey};
use std::{str::FromStr, sync::Mutex};

// Global state to store the pubky
static APP_STATE: Mutex<AppState> = Mutex::new(AppState { pubky: None });

struct AppState {
    pubky: Option<PublicKey>,
}

#[tauri::command]
async fn store_pubky(pubky_str: &str) -> Result<(), String> {
    let pubky = match PublicKey::from_str(&pubky_str) {
        Ok(key) => key,
        Err(e) => return Err(format!("Invalid pubky format: {}", e))
    };

    let client = match PubkyDrive::public() {
        Ok(c) => c,
        Err(e) => { 
            println!("Error: {}", e);
            return Err(format!("Internal error"))
        }
    };

    // Check Pubky is discoverable and has data
    let path = PubkyPath::new(Some(pubky.clone()), "/pub/").unwrap();
    
    // TODO: We should check the pub key has data with this, but currently incorrectly returns 401
    // let exists = match client.exists(path.clone()).await {
    //     Ok(p) => p,
    //     Err(e) => return Err(format!("Failed to identify pubky: {}", e))
    // };

    // Instead for now we can call `get` on the base pub path which will pull the urls of every item which the key has published.
    if let  Err(e) = client.get(path).await {
        println!("Error: {}", e);
        return Err(format!("Failed to find data for pubky"))
    }

    match APP_STATE.lock() {
        Ok(mut state) => {
            state.pubky = Some(pubky.clone());
            Ok(())
        }
        Err(_) => Err("Failed to acquire lock".to_string())
    }
}

#[tauri::command]
async fn fetch_data() -> Result<String, String> {
    let client = HttpClient::new();
    
    let pubky_url = "https://homeserver.staging.pubky.app/pub/pubky.app/hello.txt";
    let pubky_host = "b3p9kmimbq8irxe8hwwg85qbe34r3i6f3fcqw9jo61wsh13eftio";
    
    match client.get_with_header(pubky_url, "Pubky-Host", pubky_host).await {
        Ok(response) => Ok(response),
        Err(e) => Err(format!("HTTP request failed: {}", e)),
    }
}

#[tauri::command]
async fn fetch_from_state() -> Result<String, String> {
    match APP_STATE.lock() {
        Ok(state) => {
            Ok(format!("Pubky stored: {:?}", state.pubky))
        }
        Err(_) => Err("Failed to acquire lock".to_string())
    }
}



#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![store_pubky, fetch_data, fetch_from_state])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
