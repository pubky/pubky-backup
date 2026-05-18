//! Event stream creation and handling.
//!
//! This module is a simple wrapper around pubky SDK's event streams functionality.
//! Test helpers for mock streams are available under `#[cfg(test)]`.

use super::error::EventsError;
use futures_util::Stream;
use log::{info, warn};
use pubky::{Event, EventCursor, Pubky, PublicKey};
use std::pin::Pin;

/// Batch size for event stream processing and cursor save frequency.
pub(super) const EVENT_BATCH_SIZE: u16 = 50;

/// Maximum retry attempts for 429 Too Many Requests responses.
const MAX_429_RETRIES: u32 = 3;

/// Create an event stream for a given pubky.
///
/// Returns a stream of events starting from the given cursor position.
/// If cursor is None, starts from the beginning.
/// Retries with exponential backoff on 429 Too Many Requests responses.
pub(super) async fn create_event_stream(
    pubky_client: &Pubky,
    user: &PublicKey,
    cursor: Option<u64>,
) -> Result<Pin<Box<dyn Stream<Item = Result<Event, EventsError>> + Send>>, EventsError> {
    let cursor = cursor.map(EventCursor::new);

    let mut last_error = None;

    for attempt in 1..=MAX_429_RETRIES {
        match pubky_client
            .event_stream_for_user(user, cursor)
            .limit(EVENT_BATCH_SIZE)
            .subscribe()
            .await
        {
            Ok(stream) => {
                info!("Event stream subscribed OK for {}", user);

                let mapped_stream = futures_util::StreamExt::map(stream, |result| {
                    result
                        .map_err(|e| EventsError::FetchFailed(format!("Event stream error: {}", e)))
                });

                return Ok(Box::pin(mapped_stream));
            }
            Err(e) => {
                let msg = e.to_string();
                // NOTE: String matching is fragile — the SDK doesn't expose a typed
                // status code, so we check the error message. If the SDK changes its
                // error format, this detection will silently stop working.
                if msg.contains("429") && attempt < MAX_429_RETRIES {
                    let backoff_secs = 2u64.pow(attempt);
                    warn!(
                        "Event stream 429 for {} (attempt {}/{}), retrying in {}s",
                        user, attempt, MAX_429_RETRIES, backoff_secs
                    );
                    tokio::time::sleep(tokio::time::Duration::from_secs(backoff_secs)).await;
                    last_error = Some(msg);
                    continue;
                }
                if msg.contains("429") {
                    warn!(
                        "Event stream 429 Too Many Requests for {} (exhausted retries)",
                        user
                    );
                }
                return Err(EventsError::FetchFailed(format!(
                    "Failed to subscribe to event stream: {}",
                    e
                )));
            }
        }
    }

    Err(EventsError::FetchFailed(format!(
        "Failed to subscribe to event stream after {} retries: {}",
        MAX_429_RETRIES,
        last_error.unwrap_or_default()
    )))
}

#[cfg(test)]
pub mod test_helpers {
    use super::*;
    use crate::TEST_PUBKY;
    use pubky::{EventCursor, EventType, PubkyResource};
    use std::str::FromStr;

    /// Create a test event with the given parameters.
    pub fn make_test_event(
        pubky_z32: &str,
        event_type: EventType,
        path: &str,
        cursor_id: u64,
    ) -> Event {
        let url = format!("pubky://{}{}", pubky_z32, path);
        let resource = PubkyResource::from_str(&url).expect("Test URL should be valid");
        Event {
            event_type,
            resource,
            cursor: EventCursor::new(cursor_id),
            content_hash: None,
        }
    }

    /// Create a test event stream with deterministic events.
    ///
    /// Returns a stream of events starting from the given cursor position.
    /// Used by controller pipeline tests to exercise the sync logic.
    pub fn create_test_event_stream(
        cursor: Option<u64>,
    ) -> Pin<Box<dyn Stream<Item = Result<Event, EventsError>> + Send>> {
        let mock_pubky = PublicKey::from_str(TEST_PUBKY).expect("Test pubky should be valid");
        let z32 = mock_pubky.z32();

        let events: Vec<Event> = match cursor {
            None => {
                vec![
                    make_test_event(&z32, EventType::Put, "/pub/posts/001", 1),
                    make_test_event(&z32, EventType::Put, "/pub/posts/002", 2),
                    make_test_event(&z32, EventType::Put, "/pub/profile", 3),
                ]
            }
            Some(c) if c < 3 => {
                let mut events = vec![];
                if c < 1 {
                    events.push(make_test_event(&z32, EventType::Put, "/pub/posts/001", 1));
                }
                if c < 2 {
                    events.push(make_test_event(&z32, EventType::Put, "/pub/posts/002", 2));
                }
                if c < 3 {
                    events.push(make_test_event(&z32, EventType::Put, "/pub/profile", 3));
                }
                events
            }
            Some(3) | Some(4) => {
                let mut events = vec![];
                if cursor == Some(3) {
                    events.push(make_test_event(&z32, EventType::Put, "/pub/posts/003", 4));
                }
                events.push(make_test_event(
                    &z32,
                    EventType::Delete,
                    "/pub/posts/001",
                    5,
                ));
                events
            }
            Some(5) => {
                vec![make_test_event(&z32, EventType::Put, "/pub/posts/004", 6)]
            }
            _ => {
                vec![]
            }
        };

        Box::pin(futures_util::stream::iter(events.into_iter().map(Ok)))
    }

    /// Create a test event stream that fails after yielding some events.
    /// Used for testing error recovery and cursor persistence.
    pub fn create_failing_test_event_stream(
        fail_after: usize,
    ) -> Pin<Box<dyn Stream<Item = Result<Event, EventsError>> + Send>> {
        let mock_pubky = PublicKey::from_str(TEST_PUBKY).expect("Test pubky should be valid");
        let z32 = mock_pubky.z32();

        let mut items: Vec<Result<Event, EventsError>> = (1..=fail_after as u64)
            .map(|i| {
                let path = format!("/pub/posts/{:03}", i);
                Ok(make_test_event(&z32, EventType::Put, &path, i))
            })
            .collect();
        items.push(Err(EventsError::FetchFailed(
            "Simulated stream error".to_string(),
        )));

        Box::pin(futures_util::stream::iter(items))
    }
}

#[cfg(test)]
mod tests {
    use super::test_helpers::*;
    use futures_util::StreamExt;
    use pubky::EventType;

    #[tokio::test]
    async fn test_create_test_event_stream_yields_events() {
        // Test initial stream (no cursor) yields 3 PUT events with cursors 1, 2, 3
        let mut stream = create_test_event_stream(None);
        let mut events = vec![];
        while let Some(result) = stream.next().await {
            events.push(result.unwrap());
        }
        assert_eq!(events.len(), 3, "Initial batch should have 3 events");
        assert_eq!(events[0].cursor.id(), 1);
        assert_eq!(events[1].cursor.id(), 2);
        assert_eq!(events[2].cursor.id(), 3);
        assert!(events.iter().all(|e| e.event_type == EventType::Put));

        // Test stream with cursor=3 yields 2 events (PUT and DELETE)
        let mut stream = create_test_event_stream(Some(3));
        let mut events = vec![];
        while let Some(result) = stream.next().await {
            events.push(result.unwrap());
        }
        assert_eq!(events.len(), 2, "Second batch should have 2 events");
        assert_eq!(events[0].cursor.id(), 4);
        assert_eq!(events[0].event_type, EventType::Put);
        assert_eq!(events[1].cursor.id(), 5);
        assert_eq!(events[1].event_type, EventType::Delete);

        // Test stream with cursor=6 yields 0 events (exhausted)
        let stream = create_test_event_stream(Some(6));
        let events: Vec<_> = stream.collect().await;
        assert_eq!(events.len(), 0, "Exhausted stream should have 0 events");
    }

    #[tokio::test]
    async fn test_create_failing_test_event_stream() {
        let mut stream = create_failing_test_event_stream(2);

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
