use crate::BackupAppError;
use anyhow::{anyhow, Result};
use pubky::{Method, PubkyResource};
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
    pub fn from_response(response: &str) -> Result<Self> {
        let lines: Vec<&str> = response.split('\n').collect();

        if lines.is_empty() {
            return Err(anyhow!("Empty response"));
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_events() {
        // Valid PUT event
        let line = "PUT pubky://g1b6wp8bhhxtsksy3td7rj6mgg7s5k8c68663sajkfscshwj8g5y/pub/posts/001";
        let event = Event::parse(line).expect("Should parse PUT event");
        match event {
            Event::Valid {
                operation,
                resource,
            } => {
                assert_eq!(operation, Operation::Put);
                assert_eq!(
                    resource.to_string(),
                    "g1b6wp8bhhxtsksy3td7rj6mgg7s5k8c68663sajkfscshwj8g5y/pub/posts/001"
                );
                assert_eq!(
                    resource.owner.to_string(),
                    "g1b6wp8bhhxtsksy3td7rj6mgg7s5k8c68663sajkfscshwj8g5y"
                );
            }
            Event::Invalid { .. } => panic!("PUT event should be Valid"),
        }

        // Valid DEL event
        let line = "DEL pubky://g1b6wp8bhhxtsksy3td7rj6mgg7s5k8c68663sajkfscshwj8g5y/pub/profile";
        let event = Event::parse(line).expect("Should parse DEL event");
        match event {
            Event::Valid {
                operation,
                resource,
            } => {
                assert_eq!(operation, Operation::Delete);
                assert_eq!(
                    resource.to_string(),
                    "g1b6wp8bhhxtsksy3td7rj6mgg7s5k8c68663sajkfscshwj8g5y/pub/profile"
                );
                assert_eq!(
                    resource.owner.to_string(),
                    "g1b6wp8bhhxtsksy3td7rj6mgg7s5k8c68663sajkfscshwj8g5y"
                );
            }
            Event::Invalid { .. } => panic!("DEL event should be Valid"),
        }

        // Invalid URL
        let line = "PUT pubky://invalid_pubky/pub/posts/001";
        let event = Event::parse(line).expect("Should return Invalid event");
        match event {
            Event::Invalid { url, error } => {
                assert_eq!(url, "pubky://invalid_pubky/pub/posts/001");
                assert!(
                    error.contains("Invalid URL"),
                    "Error should mention invalid URL"
                );
            }
            Event::Valid { .. } => panic!("Invalid URL should create Invalid event"),
        }

        // Full response with events and cursor
        let response = r#"PUT pubky://g1b6wp8bhhxtsksy3td7rj6mgg7s5k8c68663sajkfscshwj8g5y/pub/posts/001
PUT pubky://g1b6wp8bhhxtsksy3td7rj6mgg7s5k8c68663sajkfscshwj8g5y/pub/posts/002
DEL pubky://g1b6wp8bhhxtsksy3td7rj6mgg7s5k8c68663sajkfscshwj8g5y/pub/posts/003
cursor: 0033E867HX6FE"#;

        let events_response =
            EventsResponse::from_response(response).expect("Should parse response");
        assert_eq!(events_response.cursor, "0033E867HX6FE");
        assert_eq!(events_response.events().len(), 3);
        assert!(matches!(events_response.events()[0], Event::Valid { .. }));
        assert!(matches!(events_response.events()[2], Event::Valid { .. }));
        // Response with empty cursor value
        let response = "PUT pubky://g1b6wp8bhhxtsksy3td7rj6mgg7s5k8c68663sajkfscshwj8g5y/pub/posts/001\ncursor: ";
        let events_response =
            EventsResponse::from_response(response).expect("Should parse response");
        assert_eq!(events_response.cursor, "");
        assert_eq!(events_response.events().len(), 1);
        // Response without cursor
        let response =
            r#"PUT pubky://g1b6wp8bhhxtsksy3td7rj6mgg7s5k8c68663sajkfscshwj8g5y/pub/posts/001"#;
        let events_response =
            EventsResponse::from_response(response).expect("Should parse response");
        assert_eq!(events_response.cursor, "");
        assert_eq!(events_response.events().len(), 1);
        // Response with only cursor
        let response = "cursor: 0033E867HX6FE";
        let events_response =
            EventsResponse::from_response(response).expect("Should parse cursor-only response");
        assert_eq!(events_response.cursor, "0033E867HX6FE");
        assert_eq!(events_response.events().len(), 0);
    }

    #[test]
    fn test_parse_invalid_operation() {
        // INVALID doesn't start with PUT or DEL, so it returns Event::Invalid
        let line =
            "INVALID pubky://g1b6wp8bhhxtsksy3td7rj6mgg7s5k8c68663sajkfscshwj8g5y/pub/posts/001";
        let event = Event::parse(line);
        assert!(
            matches!(event, Some(Event::Invalid { .. })),
            "Unknown operations should return Event::Invalid"
        );
        // PUTTTT begins with PUT but is invalid
        let line =
            "PUTTTT pubky://g1b6wp8bhhxtsksy3td7rj6mgg7s5k8c68663sajkfscshwj8g5y/pub/posts/001";
        let event = Event::parse(line);
        assert!(
            matches!(event, Some(Event::Invalid { .. })),
            "Unknown operations should return Event::Invalid"
        );
    }

    #[test]
    fn test_parse_unrecognized_line() {
        let line = "cursor: 0033E867HX6FE";
        let event = Event::parse(line);
        assert!(
            matches!(event, Some(Event::Invalid { .. })),
            "Should return Event::Invalid for cursor line"
        );
        let line = "some random text";
        let event = Event::parse(line);
        assert!(
            matches!(event, Some(Event::Invalid { .. })),
            "Should return Event::Invalid for random text"
        );
    }

    #[test]
    fn test_events_response_with_invalid_events() {
        let response = r#"PUT pubky://g1b6wp8bhhxtsksy3td7rj6mgg7s5k8c68663sajkfscshwj8g5y/pub/posts/001
PUT pubky://invalid_pubky/pub/posts/002
DEL pubky://g1b6wp8bhhxtsksy3td7rj6mgg7s5k8c68663sajkfscshwj8g5y/pub/posts/003
cursor: ABC123"#;

        let events_response =
            EventsResponse::from_response(response).expect("Should parse response");
        assert_eq!(events_response.cursor, "ABC123");
        assert_eq!(events_response.events().len(), 3);

        // First event should be valid
        assert!(matches!(events_response.events()[0], Event::Valid { .. }));

        // Second event should be invalid (bad pubky)
        match &events_response.events()[1] {
            Event::Invalid { url, error } => {
                assert!(url.contains("invalid_pubky"));
                assert!(error.contains("Invalid URL"));
            }
            Event::Valid { .. } => panic!("Second event should be Invalid"),
        }

        // Third event should be valid
        assert!(matches!(events_response.events()[2], Event::Valid { .. }));
    }
}
