//! Event stream creation and handling.
//!
//! This module is a simple wrapper around pubky SDK's event streams functionality, it also provides
//! mock streams for testing and developer mode.

use super::error::EventsError;
use crate::DEV_MODE_PUBKY;
use futures_util::Stream;
use pubky::{Event, EventCursor, EventType, Pubky, PubkyResource, PublicKey};
use std::pin::Pin;
use std::str::FromStr;

/// Batch size for event stream processing and cursor save frequency.
pub(super) const EVENT_BATCH_SIZE: u16 = 100;

/// Create a mock event for testing purposes.
fn make_mock_event(pubky_z32: &str, event_type: EventType, path: &str, cursor_id: u64) -> Event {
    let url = format!("pubky://{}{}", pubky_z32, path);
    let resource = PubkyResource::from_str(&url).expect("Mock URL should be valid");
    Event {
        event_type,
        resource,
        cursor: EventCursor::new(cursor_id),
        content_hash: None,
    }
}

/// Create an event stream for a given pubky.
///
/// Returns a stream of events starting from the given cursor position.
/// If cursor is None, starts from the beginning.
pub(super) async fn create_event_stream(
    pubky_client: &Pubky,
    user: &PublicKey,
    cursor: Option<u64>,
) -> Result<Pin<Box<dyn Stream<Item = Result<Event, EventsError>> + Send>>, EventsError> {
    let cursor = cursor.map(EventCursor::new);

    let sdk_stream = pubky_client
        .event_stream()
        .add_user(user, cursor)
        .map_err(|e| {
            EventsError::FetchFailed(format!("Failed to add user to event stream: {}", e))
        })?
        .limit(EVENT_BATCH_SIZE)
        .subscribe()
        .await
        .map_err(|e| {
            EventsError::FetchFailed(format!("Failed to subscribe to event stream: {}", e))
        })?;

    // Map SDK errors to our error type
    let mapped_stream = futures_util::StreamExt::map(sdk_stream, |result| {
        result.map_err(|e| EventsError::FetchFailed(format!("Event stream error: {}", e)))
    });

    Ok(Box::pin(mapped_stream))
}

/// Create a mock event stream for developer mode.
///
/// Returns a stream of mock events starting from the given cursor position.
/// Uses the same interface as `create_event_stream` for unified processing.
pub(super) fn create_mock_event_stream(
    cursor: Option<u64>,
) -> Pin<Box<dyn Stream<Item = Result<Event, EventsError>> + Send>> {
    let mock_pubky = PublicKey::from_str(DEV_MODE_PUBKY).expect("Mock pubky should be valid");
    let z32 = mock_pubky.z32();

    // Generate events based on cursor position to simulate event progression
    let events: Vec<Event> = match cursor {
        None => {
            // Initial sync - return first batch
            vec![
                make_mock_event(&z32, EventType::Put, "/pub/posts/001", 1),
                make_mock_event(&z32, EventType::Put, "/pub/posts/002", 2),
                make_mock_event(&z32, EventType::Put, "/pub/profile", 3),
            ]
        }
        Some(c) if c < 3 => {
            // Partial first batch - return remaining events
            let mut events = vec![];
            if c < 1 {
                events.push(make_mock_event(&z32, EventType::Put, "/pub/posts/001", 1));
            }
            if c < 2 {
                events.push(make_mock_event(&z32, EventType::Put, "/pub/posts/002", 2));
            }
            if c < 3 {
                events.push(make_mock_event(&z32, EventType::Put, "/pub/profile", 3));
            }
            events
        }
        Some(3) | Some(4) => {
            // Second batch - includes a DELETE
            // cursor=3 means "after event 3", cursor=4 means "after event 4"
            let mut events = vec![];
            if cursor == Some(3) {
                events.push(make_mock_event(&z32, EventType::Put, "/pub/posts/003", 4));
            }
            events.push(make_mock_event(
                &z32,
                EventType::Delete,
                "/pub/posts/001",
                5,
            ));
            events
        }
        Some(5) => {
            // Third batch
            vec![make_mock_event(&z32, EventType::Put, "/pub/posts/004", 6)]
        }
        _ => {
            // No more events
            vec![]
        }
    };

    Box::pin(futures_util::stream::iter(events.into_iter().map(Ok)))
}

/// Create a mock event stream that fails after yielding some events.
/// Used for testing error recovery and cursor persistence.
#[cfg(test)]
pub fn create_failing_mock_event_stream(
    fail_after: usize,
) -> Pin<Box<dyn Stream<Item = Result<Event, EventsError>> + Send>> {
    let mock_pubky = PublicKey::from_str(DEV_MODE_PUBKY).expect("Mock pubky should be valid");
    let z32 = mock_pubky.z32();

    // Create events that succeed, then an error
    let mut items: Vec<Result<Event, EventsError>> = (1..=fail_after as u64)
        .map(|i| {
            let path = format!("/pub/posts/{:03}", i);
            Ok(make_mock_event(&z32, EventType::Put, &path, i))
        })
        .collect();
    items.push(Err(EventsError::FetchFailed(
        "Simulated stream error".to_string(),
    )));

    Box::pin(futures_util::stream::iter(items))
}

/// Generate mock data for a given pubky URL in developer mode.
///
/// Returns JSON-like mock data based on the URL path pattern:
/// - `/profile` paths return mock user profile data
/// - `/posts/` paths return mock post data with the post ID
/// - `/follows` paths return mock follower/following lists
/// - Other paths return generic mock data
pub(super) fn get_mock_pubky_resource_data(url: &str) -> Vec<u8> {
    if url.contains("/profile") {
        r#"{"name":"Mock User","bio":"This is mock profile data for development","avatar":"https://example.com/avatar.jpg"}"#.as_bytes().to_vec()
    } else if url.contains("/posts/") {
        let post_id = url.split('/').next_back().unwrap_or("unknown");
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

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::StreamExt;

    #[test]
    fn test_get_mock_pubky_resource_data() {
        // Test profile data
        let profile_data = get_mock_pubky_resource_data("pubky://test/pub/profile.json");
        assert!(!profile_data.is_empty());
        let profile_str = String::from_utf8(profile_data).unwrap();
        assert!(profile_str.contains("Mock User"));
        assert!(profile_str.contains("bio"));

        // Test posts data - should include the post ID
        let post_data = get_mock_pubky_resource_data("pubky://test/pub/posts/123");
        assert!(!post_data.is_empty());
        let post_str = String::from_utf8(post_data).unwrap();
        assert!(post_str.contains("123"));
        assert!(post_str.contains("content"));

        // Test follows data
        let follows_data = get_mock_pubky_resource_data("pubky://test/pub/follows");
        assert!(!follows_data.is_empty());
        let follows_str = String::from_utf8(follows_data).unwrap();
        assert!(follows_str.contains("following"));
        assert!(follows_str.contains("followers"));

        // Test generic data - should include the URL
        let generic_data = get_mock_pubky_resource_data("pubky://test/pub/other/path");
        assert!(!generic_data.is_empty());
        let generic_str = String::from_utf8(generic_data).unwrap();
        assert!(generic_str.contains("Mock data"));
        assert!(generic_str.contains("pubky://test/pub/other/path"));
    }

    #[tokio::test]
    async fn test_create_mock_event_stream_yields_events() {
        // Test initial stream (no cursor) yields 3 PUT events with cursors 1, 2, 3
        let mut stream = create_mock_event_stream(None);
        let mut events: Vec<Event> = vec![];
        while let Some(result) = stream.next().await {
            events.push(result.unwrap());
        }
        assert_eq!(events.len(), 3, "Initial batch should have 3 events");
        assert_eq!(events[0].cursor.id(), 1);
        assert_eq!(events[1].cursor.id(), 2);
        assert_eq!(events[2].cursor.id(), 3);
        assert!(events.iter().all(|e| e.event_type == EventType::Put));

        // Test stream with cursor=3 yields 2 events (PUT and DELETE)
        let mut stream = create_mock_event_stream(Some(3));
        let mut events: Vec<Event> = vec![];
        while let Some(result) = stream.next().await {
            events.push(result.unwrap());
        }
        assert_eq!(events.len(), 2, "Second batch should have 2 events");
        assert_eq!(events[0].cursor.id(), 4);
        assert_eq!(events[0].event_type, EventType::Put);
        assert_eq!(events[1].cursor.id(), 5);
        assert_eq!(events[1].event_type, EventType::Delete);

        // Test stream with cursor=5 yields 1 event (third batch)
        let mut stream = create_mock_event_stream(Some(5));
        let mut events: Vec<Event> = vec![];
        while let Some(result) = stream.next().await {
            events.push(result.unwrap());
        }
        assert_eq!(events.len(), 1, "Third batch should have 1 event");
        assert_eq!(events[0].cursor.id(), 6);

        // Test stream with cursor=6 yields 0 events (exhausted)
        let stream = create_mock_event_stream(Some(6));
        let events: Vec<_> = stream.collect().await;
        assert_eq!(events.len(), 0, "Exhausted stream should have 0 events");

        // Test stream with cursor=4 yields 1 event (DELETE at cursor 5)
        let mut stream = create_mock_event_stream(Some(4));
        let mut events: Vec<Event> = vec![];
        while let Some(result) = stream.next().await {
            events.push(result.unwrap());
        }
        assert_eq!(events.len(), 1, "cursor=4 should yield 1 remaining event");
        assert_eq!(events[0].cursor.id(), 5);
        assert_eq!(events[0].event_type, EventType::Delete);
    }

    #[tokio::test]
    async fn test_create_failing_mock_event_stream() {
        // Test that failing stream yields events then error
        let mut stream = create_failing_mock_event_stream(2);

        // Should get 2 successful events
        let event1 = stream.next().await.unwrap();
        assert!(event1.is_ok());
        let event2 = stream.next().await.unwrap();
        assert!(event2.is_ok());

        // Third item should be an error
        let error = stream.next().await.unwrap();
        assert!(error.is_err());
        assert!(error
            .unwrap_err()
            .to_string()
            .contains("Simulated stream error"));

        // Stream should be exhausted
        assert!(stream.next().await.is_none());
    }
}
