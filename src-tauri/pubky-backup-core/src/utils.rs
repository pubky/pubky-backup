use log::warn;
use pubky::errors::RequestError;
use pubky::{PublicKey, StatusCode};
use std::str::FromStr;
use std::time::Duration;

/// Parse a z32-encoded pubky string into a [`PublicKey`].
///
/// Convenience wrapper around `PublicKey::from_str`.
pub fn parse_pubky(pubky_str: &str) -> Result<PublicKey, pubky::pkarr::errors::PublicKeyError> {
    let stripped = pubky_str.strip_prefix("pubky").unwrap_or(pubky_str);
    PublicKey::from_str(stripped)
}

/// Maximum number of attempts at a request that keeps failing transiently.
const MAX_ATTEMPTS: u32 = 3;

/// Delay before retrying a request that failed due to a transport error.
const TRANSPORT_RETRY_DELAY: Duration = Duration::from_secs(1);

/// Run a Pubky SDK request, retrying transient failures with a backoff.
///
/// Transport errors (timeouts, dropped connections) and rate limiting (429)
/// are retried up to [`MAX_ATTEMPTS`] times. Other errors, including other
/// error statuses from the homeserver, are returned right away.
pub async fn retry_with_backoff<T, F, Fut>(operation: F) -> Result<T, pubky::Error>
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = Result<T, pubky::Error>>,
{
    let mut attempt = 1;
    loop {
        let error = match operation().await {
            Ok(value) => return Ok(value),
            Err(error) => error,
        };
        let delay = retry_delay(&error, attempt);
        let Some(delay) = delay.filter(|_| attempt < MAX_ATTEMPTS) else {
            return Err(error);
        };
        warn!(
            "Request failed on attempt {}, retrying in {:?}: {}",
            attempt, delay, error
        );
        tokio::time::sleep(delay).await;
        attempt += 1;
    }
}

/// How long to wait before retrying after `error` on the given attempt, or
/// `None` if the error is not transient.
fn retry_delay(error: &pubky::Error, attempt: u32) -> Option<Duration> {
    match error {
        pubky::Error::Request(RequestError::Transport(_)) => Some(TRANSPORT_RETRY_DELAY),
        pubky::Error::Request(RequestError::Server { status, .. })
            if *status == StatusCode::TOO_MANY_REQUESTS =>
        {
            Some(Duration::from_secs(2u64.pow(attempt)))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn server_error(status: StatusCode) -> pubky::Error {
        RequestError::Server {
            status,
            message: "error".to_string(),
        }
        .into()
    }

    #[test]
    fn test_rate_limiting_is_retried_with_growing_delay() {
        let error = server_error(StatusCode::TOO_MANY_REQUESTS);

        assert_eq!(retry_delay(&error, 1), Some(Duration::from_secs(2)));
        assert_eq!(retry_delay(&error, 2), Some(Duration::from_secs(4)));
    }

    #[test]
    fn test_other_failures_are_not_retried() {
        assert_eq!(retry_delay(&server_error(StatusCode::NOT_FOUND), 1), None);
        assert_eq!(
            retry_delay(&server_error(StatusCode::INTERNAL_SERVER_ERROR), 1),
            None
        );
        let invalid = RequestError::Validation {
            message: "invalid".to_string(),
        };
        assert_eq!(retry_delay(&invalid.into(), 1), None);
    }

    #[tokio::test(start_paused = true)]
    async fn test_retries_until_success() {
        let attempts = std::sync::atomic::AtomicU32::new(0);

        let result = retry_with_backoff(|| async {
            let attempt = attempts.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
            if attempt < MAX_ATTEMPTS {
                Err(server_error(StatusCode::TOO_MANY_REQUESTS))
            } else {
                Ok(attempt)
            }
        })
        .await;

        assert_eq!(result.unwrap(), MAX_ATTEMPTS);
    }

    #[tokio::test(start_paused = true)]
    async fn test_gives_up_after_max_attempts() {
        let attempts = std::sync::atomic::AtomicU32::new(0);

        let result: Result<(), _> = retry_with_backoff(|| async {
            attempts.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            Err(server_error(StatusCode::TOO_MANY_REQUESTS))
        })
        .await;

        assert!(result.is_err());
        assert_eq!(attempts.into_inner(), MAX_ATTEMPTS);
    }
}
