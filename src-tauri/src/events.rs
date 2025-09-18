use log::warn;
use pubky::PubkyHttpClient;
use reqwest::{self, Method};
use serde::Deserialize;

const EVENTS_LIMIT: u32 = 100;

#[derive(Debug, Clone)]
pub struct EventInfo {
    pub operation: String,
    pub url: String,
}

#[derive(Debug, Deserialize)]
pub struct EventsResponse {
    pub events: Vec<String>,
    pub cursor: String,
}

impl EventsResponse {
    /// Parse events response from API
    pub fn from_response(response: &str) -> Result<Self, String> {
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
            } else {
                warn!("Unhandled event: {}", line);
            }
        }

        Ok(EventsResponse { events, cursor })
    }

    /// Get parsed events as EventInfo structs
    pub fn events(&self) -> Vec<EventInfo> {
        self.events
            .iter()
            .filter_map(|event| Self::parse_event(event))
            .collect()
    }

    fn parse_event(event: &str) -> Option<EventInfo> {
        let event = event.trim();

        if let Some(url) = event.strip_prefix("PUT ") {
            Some(EventInfo {
                operation: "PUT".to_string(),
                url: url.to_string(),
            })
        } else if let Some(url) = event.strip_prefix("DEL ") {
            Some(EventInfo {
                operation: "DEL".to_string(),
                url: url.to_string(),
            })
        } else {
            None
        }
    }
}

/// Fetch event list from given cursor
/// This fetches all events for all pubkys currently
pub async fn fetch_events(cursor: &str, pubky: &str) -> Result<EventsResponse, String> {
    if crate::APP_STATE
        .lock()
        .map_err(|_| "Failed to acquire app state lock".to_string())?
        .developer_mode
    {
        return get_mock_events_response(cursor);
    }

    let client = PubkyHttpClient::new().unwrap();

    match client
        .request(
            Method::GET,
            format!(
                "https://_pubky.{pubky}/events/?limit={}&cursor={}",
                EVENTS_LIMIT, cursor
            ),
        )
        .send()
        .await
    {
        Ok(response) => {
            let text = response
                .text()
                .await
                .map_err(|e| format!("Failed to read response: {}", e))?;
            EventsResponse::from_response(&text)
        }
        Err(e) => Err(format!("HTTP request failed: {}", e)),
    }
}

/// Generate mock events response for developer mode
fn get_mock_events_response(cursor: &str) -> Result<EventsResponse, String> {
    let mock_pubky = "g1b6wp8bhhxtsksy3td7rj6mgg7s5k8c68663sajkfscshwj8g5y";

    // Simulate progression through different cursors
    let (events, new_cursor) = match cursor {
        "" => {
            // First call - return initial events
            (
                vec![
                    format!("PUT pubky://{}/pub/posts/001", mock_pubky),
                    format!("PUT pubky://{}/pub/posts/002", mock_pubky),
                    format!("PUT pubky://{}/pub/profile", mock_pubky),
                ],
                "cursor001".to_string(),
            )
        }
        "cursor001" => {
            // Second call - return more events
            (
                vec![
                    format!("PUT pubky://{}/pub/posts/003", mock_pubky),
                    format!("DEL pubky://{}/pub/posts/001", mock_pubky),
                ],
                "cursor002".to_string(),
            )
        }
        "cursor002" => {
            // Third call - return single event
            (
                vec![format!("PUT pubky://{}/pub/posts/004", mock_pubky)],
                "cursor003".to_string(),
            )
        }
        _ => {
            // No more events
            (vec![], cursor.to_string())
        }
    };

    Ok(EventsResponse {
        events,
        cursor: new_cursor,
    })
}
