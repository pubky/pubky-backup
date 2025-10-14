use crate::error::EventsError;
use pubky::{PubkyResource, PublicKey};
use reqwest::Method;
use std::str::FromStr;

const EVENTS_LIMIT: u32 = 1000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operation {
    Put,
    Delete,
}

#[derive(Debug, Clone)]
pub enum Event {
    Valid {
        operation: Operation,
        resource: PubkyResource,
    },
    Invalid {
        url: String,
        error: String,
    },
}

impl Event {
    fn parse(line: &str) -> Option<Self> {
        let line = line.trim();

        // Skip empty lines
        if line.is_empty() {
            return None;
        }

        let (operation, url_str) = if let Some(url) = line.strip_prefix("PUT ") {
            (Operation::Put, url)
        } else if let Some(url) = line.strip_prefix("DEL ") {
            (Operation::Delete, url)
        } else {
            // Unrecognized operation
            return Some(Event::Invalid {
                url: line.to_string(),
                error: "Unrecognized operation (expected PUT or DEL)".to_string(),
            });
        };

        // Parse and validate the URL
        match PubkyResource::from_str(url_str) {
            Ok(resource) => Some(Event::Valid {
                operation,
                resource,
            }),
            Err(e) => Some(Event::Invalid {
                url: url_str.to_string(),
                error: format!("Invalid URL: {}", e),
            }),
        }
    }
}

#[derive(Debug)]
pub struct EventsResponse {
    events: Vec<Event>,
    pub cursor: String,
}

impl EventsResponse {
    /// Parse events response from API
    pub fn from_response(response: &str) -> Result<Self, EventsError> {
        let lines: Vec<&str> = response.split('\n').collect();

        if lines.is_empty() {
            return Err(EventsError::InvalidResponse("Empty response".to_string()));
        }

        let mut events = Vec::new();
        let mut cursor = String::new();

        for line in lines {
            if let Some(cursor_value) = line.trim_start().strip_prefix("cursor: ") {
                cursor = cursor_value.trim().to_string();
            } else if let Some(event) = Event::parse(line.trim()) {
                events.push(event);
            }
        }

        Ok(EventsResponse { events, cursor })
    }

    /// Get events
    pub fn events(&self) -> &[Event] {
        &self.events
    }
}

/// Fetch event list from given cursor
/// This fetches all events for all pubkys currently
pub async fn fetch_events(cursor: &str, pubky: &PublicKey) -> Result<EventsResponse, EventsError> {
    let base_url = format!("pubky://{}/events/", pubky);
    let url = reqwest::Url::parse_with_params(
        &base_url,
        &[
            ("limit", EVENTS_LIMIT.to_string()),
            ("cursor", cursor.to_string()),
        ],
    )
    .map_err(|e| EventsError::InvalidResponse(format!("Failed to build URL: {}", e)))?;

    let response = crate::retry_with_backoff(|| {
        let url_str = url.to_string();
        async move {
            let client = pubky::global_client()
                .map_err(|e| format!("Failed to get global client: {}", e))?;
            client
                .request(Method::GET, &url_str)
                .send()
                .await
                .map_err(|e| format!("Failed to fetch events data for {}: {}", url_str, e))
        }
    })
    .await
    .map_err(EventsError::FetchFailed)?;

    let text = response.text().await.map_err(|e| {
        EventsError::InvalidResponse(format!("Failed to read response text: {}", e))
    })?;

    EventsResponse::from_response(&text)
}

/// Developer mode mock pubky (for testing without real pubky)
const DEV_MODE_PUBKY: &str = "g1b6wp8bhhxtsksy3td7rj6mgg7s5k8c68663sajkfscshwj8g5y";

/// Generate mock events response for developer mode
pub fn get_mock_events_response(cursor: &str) -> Result<EventsResponse, EventsError> {
    let mock_pubky = DEV_MODE_PUBKY;

    fn make_valid_event(operation: Operation, url: String) -> Event {
        let resource = PubkyResource::from_str(&url).expect("Mock URL should be valid");
        Event::Valid {
            operation,
            resource,
        }
    }

    // Simulate progression through different cursors
    let (events, new_cursor) = match cursor {
        "" => {
            // First call - return initial events
            (
                vec![
                    make_valid_event(
                        Operation::Put,
                        format!("pubky://{}/pub/posts/001", mock_pubky),
                    ),
                    make_valid_event(
                        Operation::Put,
                        format!("pubky://{}/pub/posts/002", mock_pubky),
                    ),
                    make_valid_event(
                        Operation::Put,
                        format!("pubky://{}/pub/profile", mock_pubky),
                    ),
                ],
                "cursor001".to_string(),
            )
        }
        "cursor001" => {
            // Second call - return more events
            (
                vec![
                    make_valid_event(
                        Operation::Put,
                        format!("pubky://{}/pub/posts/003", mock_pubky),
                    ),
                    make_valid_event(
                        Operation::Delete,
                        format!("pubky://{}/pub/posts/001", mock_pubky),
                    ),
                ],
                "cursor002".to_string(),
            )
        }
        "cursor002" => {
            // Third call - return single event
            (
                vec![make_valid_event(
                    Operation::Put,
                    format!("pubky://{}/pub/posts/004", mock_pubky),
                )],
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
