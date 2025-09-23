use anyhow::{anyhow, Result};
use log::warn;
use reqwest::{self, Method};
use serde::Deserialize;

const EVENTS_LIMIT: u32 = 1000;

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
pub async fn fetch_events(cursor: &str, pubky: &str) -> Result<EventsResponse> {
    if crate::APP_STATE
        .lock()
        .map_err(|_| anyhow!("Failed to acquire app state lock"))?
        .developer_mode
    {
        return get_mock_events_response(cursor);
    }

    let client = crate::get_or_create_http_client()?;
    const MAX_RETRIES: u32 = 3;

    for attempt in 1..=MAX_RETRIES {
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
                // Check for rate limiting
                if response.status() == 429 {
                    if attempt < MAX_RETRIES {
                        let sleep_time = 2 * attempt;
                        warn!(
                            "HTTP request rate limited on attempt {}, sleeping for {} seconds",
                            attempt, sleep_time
                        );
                        tokio::time::sleep(tokio::time::Duration::from_secs(sleep_time.into()))
                            .await;
                        continue; // Retry
                    } else {
                        return Err(anyhow!("Rate limited after maximum retries"));
                    }
                }

                let text = response
                    .text()
                    .await
                    .map_err(|e| anyhow!("Failed to read response: {}", e))?;
                return EventsResponse::from_response(&text);
            }
            Err(e) => {
                if attempt < MAX_RETRIES {
                    warn!(
                        "HTTP request failed on attempt {}: {}, retrying...",
                        attempt, e
                    );
                    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                    continue; // Retry
                } else {
                    return Err(anyhow!(
                        "HTTP request failed after {} attempts: {}",
                        MAX_RETRIES,
                        e
                    ));
                }
            }
        }
    }

    Err(anyhow!("Unexpected error in fetch_events retry loop"))
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
