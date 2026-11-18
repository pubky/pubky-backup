//! Event stream creation and handling.
//!
//! This module is a simple wrapper around pubky SDK's event streams functionality.
//! Test helpers for mock streams are available under `#[cfg(test)]`.

use super::error::{EventsError, SyncError};
use super::scope::{SyncScope, PRIVATE_PATH};
use super::session::is_session_rejection;
use crate::utils::retry_with_backoff;
use futures_util::Stream;
use log::{info, warn};
use pubky::errors::RequestError;
use pubky::{Event, EventCursor, Pubky, PublicKey, StatusCode};
use std::pin::Pin;

/// Batch size for event stream processing and cursor save frequency.
pub(super) const EVENT_BATCH_SIZE: u16 = 50;

/// A stream of a key's events, as the controller consumes it.
pub(super) type EventStream = Pin<Box<dyn Stream<Item = Result<Event, EventsError>> + Send>>;

/// Create an event stream for one scope of a given pubky's data.
///
/// Returns a stream of events starting from the given cursor position.
/// If cursor is None, starts from the beginning. Transient failures to
/// subscribe, including rate limiting, are retried.
///
/// # Errors
///
/// Returns `SyncError::SessionRejected` if the homeserver does not accept
/// the session of a private scope.
pub(super) async fn create_event_stream(
    pubky_client: &Pubky,
    user: &PublicKey,
    scope: SyncScope<'_>,
    cursor: Option<u64>,
) -> Result<EventStream, SyncError> {
    let cursor = cursor.map(EventCursor::new);

    let subscribed = retry_with_backoff(|| {
        let mut builder = pubky_client
            .event_stream_for_user(user, cursor)
            .limit(EVENT_BATCH_SIZE);
        if let SyncScope::Private(session) = scope {
            builder = builder.path(PRIVATE_PATH).session(session);
        }
        builder.subscribe()
    })
    .await;

    let stream = match subscribed {
        Ok(stream) => stream,
        Err(e) if matches!(scope, SyncScope::Private(_)) && is_session_rejection(&e) => {
            warn!("Private event stream for {} rejected: {}", user, e);
            return Err(SyncError::SessionRejected);
        }
        Err(e) => return Err(subscribe_failure(user, e)),
    };

    info!("Event stream subscribed OK for {}", user);
    Ok(Box::pin(futures_util::StreamExt::map(stream, |result| {
        result.map_err(|e| EventsError::FetchFailed(format!("Event stream error: {}", e)))
    })))
}

/// The error for a subscription that failed even after retrying.
///
/// When nothing answered, or the homeserver is limiting requests, the error
/// says so plainly and the details, which spell out the request, go to the
/// log. Other errors are kept as they are.
fn subscribe_failure(user: &PublicKey, error: pubky::Error) -> SyncError {
    match &error {
        pubky::Error::Request(RequestError::Transport(_)) => {
            warn!("Could not reach the homeserver of {}: {}", user, error);
            SyncError::Unreachable
        }
        pubky::Error::Request(RequestError::Server { status, .. })
            if *status == StatusCode::TOO_MANY_REQUESTS =>
        {
            warn!("Homeserver of {} is limiting requests: {}", user, error);
            SyncError::RateLimited
        }
        _ => EventsError::FetchFailed(format!("Failed to subscribe to event stream: {}", error))
            .into(),
    }
}

#[cfg(test)]
pub mod test_helpers {
    use super::*;
    use crate::TEST_PUBKY;
    use pubky::{EventCursor, EventType, PubkyResource};
    use std::str::FromStr;

    /// Create a PUT event type with a placeholder content hash for tests.
    pub fn put_event_type() -> EventType {
        EventType::Put {
            content_hash: [0; 32].into(),
        }
    }

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
        }
    }

    /// Create a test event stream with deterministic events.
    ///
    /// Returns a stream of events starting from the given cursor position.
    /// Used by controller pipeline tests to exercise the sync logic.
    pub fn create_test_event_stream(cursor: Option<u64>) -> EventStream {
        let mock_pubky = PublicKey::from_str(TEST_PUBKY).expect("Test pubky should be valid");
        let z32 = mock_pubky.z32();

        let events: Vec<Event> = match cursor {
            None => {
                vec![
                    make_test_event(&z32, put_event_type(), "/pub/posts/001", 1),
                    make_test_event(&z32, put_event_type(), "/pub/posts/002", 2),
                    make_test_event(&z32, put_event_type(), "/pub/profile", 3),
                ]
            }
            Some(c) if c < 3 => {
                let mut events = vec![];
                if c < 1 {
                    events.push(make_test_event(&z32, put_event_type(), "/pub/posts/001", 1));
                }
                if c < 2 {
                    events.push(make_test_event(&z32, put_event_type(), "/pub/posts/002", 2));
                }
                if c < 3 {
                    events.push(make_test_event(&z32, put_event_type(), "/pub/profile", 3));
                }
                events
            }
            Some(3) | Some(4) => {
                let mut events = vec![];
                if cursor == Some(3) {
                    events.push(make_test_event(&z32, put_event_type(), "/pub/posts/003", 4));
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
                vec![make_test_event(&z32, put_event_type(), "/pub/posts/004", 6)]
            }
            _ => {
                vec![]
            }
        };

        Box::pin(futures_util::stream::iter(events.into_iter().map(Ok)))
    }

    /// Create a test event stream with exactly `count` events starting from `cursor_start`.
    ///
    /// Used to test multi-batch accumulation: create a stream with `count >= EVENT_BATCH_SIZE`
    /// so `process_event_stream` returns `Continue`, then a smaller follow-up that returns `Break`.
    pub fn create_sized_test_event_stream(count: usize, cursor_start: u64) -> EventStream {
        let mock_pubky = PublicKey::from_str(TEST_PUBKY).expect("Test pubky should be valid");
        let z32 = mock_pubky.z32();

        let events: Vec<Event> = (0..count as u64)
            .map(|i| {
                let cursor_id = cursor_start + i + 1;
                let path = format!("/pub/posts/{:03}", cursor_id);
                make_test_event(&z32, put_event_type(), &path, cursor_id)
            })
            .collect();

        Box::pin(futures_util::stream::iter(events.into_iter().map(Ok)))
    }

    /// Create a test event stream that fails after yielding some events.
    /// Used for testing error recovery and cursor persistence.
    pub fn create_failing_test_event_stream(fail_after: usize) -> EventStream {
        let mock_pubky = PublicKey::from_str(TEST_PUBKY).expect("Test pubky should be valid");
        let z32 = mock_pubky.z32();

        let mut items: Vec<Result<Event, EventsError>> = (1..=fail_after as u64)
            .map(|i| {
                let path = format!("/pub/posts/{:03}", i);
                Ok(make_test_event(&z32, put_event_type(), &path, i))
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
    use super::*;
    use futures_util::StreamExt;
    use pubky::EventType;
    use std::str::FromStr;

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
        assert!(events
            .iter()
            .all(|e| matches!(e.event_type, EventType::Put { .. })));

        // Test stream with cursor=3 yields 2 events (PUT and DELETE)
        let mut stream = create_test_event_stream(Some(3));
        let mut events = vec![];
        while let Some(result) = stream.next().await {
            events.push(result.unwrap());
        }
        assert_eq!(events.len(), 2, "Second batch should have 2 events");
        assert_eq!(events[0].cursor.id(), 4);
        assert!(matches!(events[0].event_type, EventType::Put { .. }));
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

    fn test_pubky() -> PublicKey {
        PublicKey::from_str(crate::TEST_PUBKY).unwrap()
    }

    fn server_error(status: StatusCode) -> pubky::Error {
        RequestError::Server {
            status,
            message: "error".to_string(),
        }
        .into()
    }

    #[tokio::test]
    async fn test_subscribe_failure_without_response_is_unreachable() {
        // Nothing listens on port 1, so the request gets no response at all
        let transport = reqwest::Client::new()
            .get("http://127.0.0.1:1/")
            .send()
            .await
            .unwrap_err();
        let error = pubky::Error::from(RequestError::Transport(transport));

        let failure = subscribe_failure(&test_pubky(), error);

        assert!(matches!(failure, SyncError::Unreachable));
    }

    #[test]
    fn test_subscribe_failure_when_rate_limited() {
        let error = server_error(StatusCode::TOO_MANY_REQUESTS);

        let failure = subscribe_failure(&test_pubky(), error);

        assert!(matches!(failure, SyncError::RateLimited));
    }

    #[test]
    fn test_subscribe_failure_keeps_other_server_errors() {
        let error = server_error(StatusCode::INTERNAL_SERVER_ERROR);

        let failure = subscribe_failure(&test_pubky(), error);

        assert!(matches!(failure, SyncError::Events(_)));
        assert!(failure.to_string().contains("500"));
    }
}
