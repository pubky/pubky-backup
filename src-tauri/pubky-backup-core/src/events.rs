use crate::error::EventsError;
use crate::{DEV_MODE_PUBKY, EVENT_BATCH_SIZE};
use futures_util::Stream;
use pubky::{Event, EventCursor, EventType, Pubky, PubkyResource, PublicKey};
use std::pin::Pin;
use std::str::FromStr;

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

/// Create an event stream for a given pubky
///
/// Returns a stream of events starting from the given cursor position.
/// If cursor is None, starts from the beginning.
pub async fn create_event_stream(
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

/// Create a mock event stream for developer mode
///
/// Returns a stream of mock events starting from the given cursor position.
/// Uses the same interface as `create_event_stream` for unified processing.
pub fn create_mock_event_stream(
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
