use crate::BackupAppError;
use anyhow::{anyhow, Result};
use log::warn;
use pubky::Method;
use serde::Deserialize;

const EVENTS_LIMIT: u32 = 1000;
const PUT_OPERATION: &str = "PUT";
const DEL_OPERATION: &str = "DEL";

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
    pub fn from_response(response: &str) -> Result<Self> {
        let lines: Vec<&str> = response.trim().split('\n').collect();

        if lines.is_empty() {
            return Err(anyhow!("Empty response"));
        }

        let mut events = Vec::new();
        let mut cursor = String::new();

        for line in lines {
            let line = line.trim();
            if line.starts_with("PUT pubky://") || line.starts_with("DEL pubky://") {
                events.push(line.to_string());
            } else if line.starts_with("cursor: ") {
                cursor = line.strip_prefix("cursor: ").unwrap_or("").to_string();
            } else if !line.is_empty() {
                // [""] is a valid empty response
                warn!("Unhandled event: {:?}", line);
            }
        }

        Ok(EventsResponse { events, cursor })
    }

    /// Get parsed events as EventInfo structs
    pub fn events(&self) -> Result<Vec<EventInfo>> {
        self.events
            .iter()
            .filter_map(|event| Self::parse_event(event).transpose())
            .collect()
    }

    fn parse_event(event: &str) -> Result<Option<EventInfo>> {
        let event = event.trim();

        let (operation, url_str) = if let Some(url) = event.strip_prefix("PUT ") {
            (PUT_OPERATION, url)
        } else if let Some(url) = event.strip_prefix("DEL ") {
            (DEL_OPERATION, url)
        } else {
            return Ok(None);
        };

        Ok(Some(EventInfo {
            operation: operation.to_string(),
            url: url_str.to_string(),
        }))
    }
}

/// Fetch event list from given cursor
/// This fetches all events for all pubkys currently
pub async fn fetch_events(cursor: &str, pubky: &str) -> Result<EventsResponse> {
    if crate::APP_STATE
        .lock()
        .map_err(|_| anyhow!(BackupAppError::lock_failed()))?
        .developer_mode
    {
        return get_mock_events_response(cursor);
    }

    let base_url = format!("pubky://{}/events/", pubky);
    let url = reqwest::Url::parse_with_params(
        &base_url,
        &[
            ("limit", EVENTS_LIMIT.to_string()),
            ("cursor", cursor.to_string()),
        ],
    )
    .map_err(|e| anyhow!("Failed to build URL: {}", e))?;

    let response = crate::retry_with_backoff(|| {
        let url_str = url.to_string();
        async move {
            let client = pubky::global_client().map_err(BackupAppError::internal)?;
            client
                .request(Method::GET, &url_str)
                .send()
                .await
                .map_err(|e| anyhow!("Failed to fetch events data for {}: {}", url_str, e))
        }
    })
    .await?;

    let text = response
        .text()
        .await
        .map_err(|e| anyhow!("Failed to read response: {}", e))?;

    EventsResponse::from_response(&text)
}

/// Generate mock events response for developer mode
fn get_mock_events_response(cursor: &str) -> Result<EventsResponse> {
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
